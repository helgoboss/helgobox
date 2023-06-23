use crate::base::clip_edit_session::{AudioClipEditSession, ClipEditSession, MidiClipEditSession};
use crate::base::clip_manifestation::manifest_clip_on_track;
use crate::base::{
    Clip, ClipMatrixHandler, ClipRecordDestination, ClipRecordHardwareInput,
    ClipRecordHardwareMidiInput, ClipRecordInput, ClipRecordTask, CreateRtClipEquipment,
    EssentialColumnRecordClipArgs, VirtualClipRecordAudioInput, VirtualClipRecordHardwareMidiInput,
};
use crate::conversion_util::adjust_duration_in_secs_anti_proportionally;
use crate::rt::supplier::{
    MaterialInfo, MidiOverdubOutcome, MidiOverdubSettings, MidiSequence, QuantizationSettings,
    Recorder, RecordingArgs, RecordingEquipment, SupplierChain,
};

use crate::rt::{
    ClipChangeEvent, ClipRecordArgs, ColumnCommandSender, ColumnLoadClipArgs, ColumnLoadSlotArgs,
    ColumnSetClipLoopedArgs, ColumnSetClipSettingsArgs, FillSlotMode, InternalClipPlayState,
    MidiOverdubInstruction, NormalRecordingOutcome, RecordNewClipInstruction, RtClipId, RtSlot,
    RtSlotId, SharedRtColumn, SlotChangeEvent, SlotRecordInstruction, SlotRuntimeData,
};
use crate::source_util::{create_file_api_source, create_pcm_source_from_file_based_api_source};
use crate::{clip_timeline, rt, ClipEngineResult, HybridTimeline, Timeline};

use helgoboss_learn::UnitValue;
use indexmap::IndexMap;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    ChannelRange, ClipId, ColumnClipRecordSettings, Db, MatrixClipRecordSettings, MidiChunkSource,
    MidiClipRecordMode, PositiveSecond, RecordOrigin, SlotId,
};
use playtime_api::runtime::ClipPlayState;
use reaper_high::{Item, Project, Reaper, Take, Track, TrackRoute};
use reaper_medium::{
    Bpm, CommandId, DurationInSeconds, Hwnd, PositionInSeconds, RecordingInput, RequiredViewMode,
};
use std::mem;
use std::ptr::null_mut;
use xxhash_rust::xxh3::Xxh3Builder;

#[derive(Clone, Debug)]
pub struct Slot {
    id: SlotId,
    rt_id: RtSlotId,
    index: usize,
    play_state: InternalClipPlayState,
    /// If this is set, the slot contains a clip.
    ///
    /// This means one of the following things:
    ///
    /// - The clip is active and can be playing, stopped etc.
    /// - The clip is active and is currently being MIDI-overdubbed.
    /// - The clip is inactive, which means it's about to be replaced with different clip content
    ///   that's in the process of being recorded right now.
    contents: Contents,
    state: SlotState,
    /// Route which was created temporarily for recording.
    temporary_route: Option<TrackRoute>,
}

type Contents = IndexMap<RtClipId, Content, Xxh3Builder>;

#[derive(Clone, Debug)]
pub struct Content {
    pub clip: Clip,
    pub online_data: Option<OnlineData>,
}

#[derive(Clone, Debug)]
pub struct OnlineData {
    /// The frame count in the material info is supposed to take the section bounds into account.
    pub runtime_data: SlotRuntimeData,
    pub edit_session: Option<ClipEditSession>,
}

impl OnlineData {
    pub fn new(rt_clip: &rt::RtClip) -> ClipEngineResult<Self> {
        let data = Self {
            runtime_data: SlotRuntimeData::new(rt_clip, false)?,
            edit_session: None,
        };
        Ok(data)
    }

    pub fn midi_edit_session_mut(&mut self) -> ClipEngineResult<&mut MidiClipEditSession> {
        let edit_session = self.edit_session.as_mut().ok_or("no edit session")?;
        let ClipEditSession::Midi(midi_edit_session) = edit_session else {
            return Err("no MIDI edit session");
        };
        Ok(midi_edit_session)
    }

    /// Returns the effective length (tempo adjusted and taking the section into account).
    pub fn effective_length_in_seconds(
        &self,
        clip: &Clip,
        timeline: &HybridTimeline,
    ) -> ClipEngineResult<DurationInSeconds> {
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        let tempo_factor = self.tempo_factor(clip, timeline_tempo);
        let tempo_adjusted_secs = adjust_duration_in_secs_anti_proportionally(
            self.runtime_data.material_info.duration(),
            tempo_factor,
        );
        Ok(tempo_adjusted_secs)
    }

    pub fn tempo_factor(&self, clip: &Clip, timeline_tempo: Bpm) -> f64 {
        let is_midi = self.runtime_data.material_info.is_midi();
        clip.tempo_factor(timeline_tempo, is_midi)
    }

    pub fn proportional_position(&self) -> ClipEngineResult<UnitValue> {
        self.runtime_data.proportional_position()
    }

    pub fn position_in_seconds(&self, clip: &Clip, timeline_tempo: Bpm) -> PositionInSeconds {
        let tempo_factor = self.tempo_factor(clip, timeline_tempo);
        self.runtime_data.position_in_seconds(tempo_factor)
    }

    pub fn peak(&self) -> UnitValue {
        self.runtime_data.peak()
    }

    pub fn is_freezable(&self) -> bool {
        // At the moment, we only freeze MIDI to audio.
        self.runtime_data.material_info.is_midi()
    }
}

impl Content {
    pub fn new(clip: Clip) -> Self {
        Content {
            clip,
            online_data: None,
        }
    }

    pub fn duplicate(&self) -> Self {
        Self {
            clip: self.clip.duplicate(),
            online_data: None,
        }
    }

    pub fn is_freezable(&self) -> bool {
        if let Some(od) = self.online_data.as_ref() {
            od.is_freezable()
        } else {
            false
        }
    }

    fn midi_edit_session(&self) -> ClipEngineResult<&MidiClipEditSession> {
        let online_data = self.online_data.as_ref().ok_or("clip offline")?;
        let edit_session = online_data.edit_session.as_ref().ok_or("no edit session")?;
        let ClipEditSession::Midi(midi_edit_session) = edit_session else {
            return Err("no MIDI edit session");
        };
        Ok(midi_edit_session)
    }

    pub async fn freeze(&mut self, playback_track: &Track) -> ClipEngineResult<()> {
        // TODO-high-clip-engine CONTINUE Get the clip-to-item layout 100% right.
        // TODO-high-clip-engine CONTINUE Sync the frozen clips to the real-time thread when finished.
        // TODO-high-clip-engine CONTINUE Provide a header panel action to go back to unfrozen version.
        // TODO-high-clip-engine CONTINUE Provide a header panel action to go back to frozen version.
        // TODO-high-clip-engine CONTINUE Don't freeze tracks whose FX chain contains ReaLearn FX only.
        // TODO-high-clip-engine CONTINUE Take relevant FX offline/online when freezing/unfreezing.
        let project = playback_track.project();
        let online_data = self.online_data.as_ref().ok_or("clip not online")?;
        let clip = &self.clip;
        let manifestation = manifest_clip_on_track(clip, online_data, playback_track)?;
        project.select_item_exclusively(manifestation.item);
        // Item: Apply track/take FX to items
        let apply_fx_id = CommandId::new(40209);
        Reaper::get()
            .main_section()
            .action_by_command_id(apply_fx_id)
            .invoke_as_trigger(Some(project))?;
        let frozen_take = manifestation
            .item
            .active_take()
            .expect("frozen item doesn't have frozen take");
        let frozen_pcm_source = frozen_take
            .source()
            .expect("frozen take doesn't have a source");
        let file_name = frozen_pcm_source
            .file_name()
            .expect("frozen source doesn't have file name");
        let frozen_api_source = create_file_api_source(Some(project), &file_name);
        self.clip
            .activate_frozen_source(frozen_api_source, manifestation.tempo);
        Ok(())
    }

    /// Moves cursor in REAPER's MIDI editor.
    pub(crate) fn notify_pos_changed(
        &self,
        bpm: Bpm,
        seconds: PositionInSeconds,
    ) -> ClipEngineResult<()> {
        let midi_edit_session = self.midi_edit_session()?;
        let source = midi_edit_session.clip_manifestation().source()?;
        let bps = bpm.get() / 60.0;
        let beats = seconds.get() * bps;
        // TODO-medium Read PPQ from MIDI file. The best thing is if we use the MidiSequence as
        //  source representation in the main thread and read from it.
        let ppq = 960.0;
        let ticks = beats * ppq;
        const PCM_SOURCE_EXT_SET_PREVIEW_POS_OVERRIDE: i32 = 0xC0101;
        // REAPER v6.73+dev1230
        unsafe {
            source.as_raw().extended(
                PCM_SOURCE_EXT_SET_PREVIEW_POS_OVERRIDE,
                &ticks as *const _ as *mut _,
                null_mut(),
                null_mut(),
            )
        };
        Ok(())
    }
}

impl Slot {
    pub fn new(id: SlotId, index: usize) -> Self {
        Self {
            rt_id: RtSlotId::from_slot_id(&id),
            id,
            index,
            play_state: Default::default(),
            contents: Default::default(),
            state: Default::default(),
            temporary_route: None,
        }
    }

    pub fn id(&self) -> &SlotId {
        &self.id
    }

    pub fn rt_id(&self) -> RtSlotId {
        self.rt_id
    }

    pub fn duplicate(&self, new_index: usize) -> Self {
        let new_id = SlotId::random();
        Self {
            rt_id: RtSlotId::from_slot_id(&new_id),
            id: new_id,
            index: new_index,
            play_state: Default::default(),
            contents: self
                .contents
                .values()
                .map(|content| {
                    let duplicate_content = content.duplicate();
                    (duplicate_content.clip.rt_id(), duplicate_content)
                })
                .collect(),
            state: Default::default(),
            temporary_route: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_empty() && !self.state.is_pretty_much_recording()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub(crate) fn set_index(&mut self, new_index: usize) {
        self.index = new_index;
    }

    /// Returns `None` if this slot doesn't need to be saved (because it's empty).
    pub fn save(&self) -> Option<api::Slot> {
        let clips: Vec<_> = self
            .contents
            .values()
            .filter_map(|content| content.clip.save().ok())
            .collect();
        if clips.is_empty() {
            return None;
        }
        let api_slot = api::Slot {
            id: self.id.clone(),
            row: self.index,
            clip_old: None,
            clips: Some(clips),
        };
        Some(api_slot)
    }

    /// Loads the given clips into the slot but doesn't bring them online yet.
    ///
    /// Keeps the clip IDs, doesn't generate new ones.
    pub fn load(&mut self, api_clips: Vec<api::Clip>) {
        self.contents = load_api_clips(api_clips, IdMode::KeepIds).collect();
    }

    /// Brings the previously loaded clips online.
    pub fn bring_online(&mut self, equipment: CreateRtClipEquipment) -> RtSlot {
        let rt_clips = self.contents.values_mut().filter_map(|content| {
            let rt_clip = content.clip.create_real_time_clip(equipment).ok()?;
            let online_data = OnlineData::new(&rt_clip).ok()?;
            content.online_data = Some(online_data);
            Some((rt_clip.id(), rt_clip))
        });
        RtSlot::new(self.rt_id, rt_clips.collect())
    }

    pub fn apply_edited_contents_if_necessary(
        &mut self,
        equipment: CreateRtClipEquipment,
        column_command_sender: &ColumnCommandSender,
    ) {
        for (i, content) in self.contents.values_mut().enumerate() {
            let Some(online_data) = content.online_data.as_mut() else {
                continue;
            };
            if apply_edited_content_if_necessary(online_data, &mut content.clip) == Ok(true) {
                let Ok(rt_clip) = content.clip.create_real_time_clip(equipment) else {
                    continue;
                };
                let Ok(runtime_data) = SlotRuntimeData::new(&rt_clip, false) else {
                    continue;
                };
                online_data.runtime_data = runtime_data;
                let args = ColumnLoadClipArgs {
                    slot_index: self.index,
                    clip_index: i,
                    clip: rt_clip,
                };
                column_command_sender.load_clip(args);
            }
        }
    }

    /// Immediately syncs to real-time column.
    #[allow(clippy::too_many_arguments)]
    pub fn fill(
        &mut self,
        api_clips: Vec<api::Clip>,
        equipment: CreateRtClipEquipment,
        rt_command_sender: &ColumnCommandSender,
        fill_mode: FillSlotMode,
        id_mode: IdMode,
    ) {
        // Load clips
        let contents = load_api_clips(api_clips, id_mode);
        match fill_mode {
            FillSlotMode::Add => {
                self.contents.extend(contents);
            }
            FillSlotMode::Replace => {
                self.contents = contents.collect();
            }
        }
        // Bring slot online
        let rt_slot = self.bring_online(equipment);
        // Send real-time slot to the real-time column
        let args = ColumnLoadSlotArgs {
            slot_index: self.index,
            clips: rt_slot.clips,
        };
        rt_command_sender.load_slot(Box::new(Some(args)));
    }

    /// Immediately syncs to real-time column.
    pub fn set_clip_data(
        &mut self,
        clip_index: usize,
        api_clip: api::Clip,
        rt_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<()> {
        // Apply data to existing clip
        let content = get_content_mut(&mut self.contents, clip_index)?;
        let clip = &mut content.clip;
        let new_clip = Clip::load(api_clip);
        clip.set_data(new_clip);
        // Sync to real-time column
        let args = ColumnSetClipSettingsArgs {
            slot_index: self.index,
            clip_index,
            settings: *clip.rt_settings(),
        };
        rt_command_sender.set_clip_settings(args);
        Ok(())
    }

    pub fn is_recording(&self) -> bool {
        self.state.is_pretty_much_recording()
    }

    pub(crate) fn midi_overdub_clip(
        &mut self,
        clip_index: usize,
        args: EssentialSlotRecordClipArgs,
    ) -> ClipEngineResult<()> {
        let content = self.get_content(clip_index)?;
        let overdub_instruction = self.create_midi_overdub_instruction_internal(
            content,
            args.column_args.matrix_record_settings,
            args.recording_track.project(),
        )?;
        self.record_or_overdub_internal(args, Some(overdub_instruction))
    }

    /// Chooses dynamically whether to do normal recording or overdub.
    pub(crate) fn record_clip(
        &mut self,
        args: EssentialSlotRecordClipArgs,
    ) -> ClipEngineResult<()> {
        let overdub_instruction = self.create_midi_overdub_instruction_if_applicable(
            args.column_args.matrix_record_settings,
            args.recording_track.project(),
        );
        self.record_or_overdub_internal(args, overdub_instruction)
    }

    /// Decides whether to use MIDI overdub recording and if yes, returns an instruction.
    ///
    /// Decides for MIDI overdub under the following conditions:
    ///
    /// - MIDI recording mode in matrix record settings is set to overdub/replace
    /// - Slot has exactly one clip
    fn create_midi_overdub_instruction_if_applicable(
        &self,
        matrix_record_settings: &MatrixClipRecordSettings,
        project: Project,
    ) -> Option<MidiOverdubInstruction> {
        if matrix_record_settings.midi_settings.record_mode == MidiClipRecordMode::Normal {
            return None;
        }
        if self.contents.len() > 1 {
            return None;
        }
        let (_, content) = self.contents.first()?;
        self.create_midi_overdub_instruction_internal(content, matrix_record_settings, project)
            .ok()
    }

    fn create_midi_overdub_instruction_internal(
        &self,
        content: &Content,
        matrix_record_settings: &MatrixClipRecordSettings,
        project: Project,
    ) -> ClipEngineResult<MidiOverdubInstruction> {
        let Some(online_data) = &content.online_data else {
            return Err("clip not online");
        };
        if !online_data.runtime_data.material_info.is_midi() {
            return Err("not a MIDI clip");
        }
        let instruction = create_midi_overdub_instruction(
            0,
            matrix_record_settings.midi_settings.record_mode,
            matrix_record_settings.midi_settings.auto_quantize,
            content.clip.api_source(),
            Some(project),
        )?;
        Ok(instruction)
    }

    #[allow(clippy::too_many_arguments)]
    fn record_or_overdub_internal(
        &mut self,
        args: EssentialSlotRecordClipArgs,
        desired_midi_overdub_instruction: Option<MidiOverdubInstruction>,
    ) -> ClipEngineResult<()> {
        if self.state.is_pretty_much_recording() {
            return Err("recording already");
        }
        if self.contents.len() > 1 {
            return Err("recording on slots with multiple clips is not supported");
        }
        // Check preconditions and prepare stuff.
        let project = args.recording_track.project();
        if self.play_state.is_somehow_recording() {
            return Err("recording already according to play state");
        }
        let (common_stuff, mode_specific_stuff) = create_record_stuff(
            self.index,
            args.column_args.containing_track,
            args.column_args.matrix_record_settings,
            args.column_record_settings,
            args.recording_track,
            args.rt_column,
            desired_midi_overdub_instruction,
        )?;
        match mode_specific_stuff {
            ModeSpecificRecordStuff::FromScratch(from_scratch_stuff) => {
                self.record_from_scratch(args, project, common_stuff, from_scratch_stuff)
            }
            ModeSpecificRecordStuff::MidiOverdub(midi_overdub_stuff) => self
                .record_as_midi_overdub(
                    args.column_command_sender,
                    args.column_args.handler,
                    common_stuff,
                    midi_overdub_stuff,
                ),
        }
    }

    fn record_from_scratch(
        &mut self,
        args: EssentialSlotRecordClipArgs,
        project: Project,
        common_stuff: CommonRecordStuff,
        specific_stuff: FromScratchRecordStuff,
    ) -> ClipEngineResult<()> {
        // Build slot instruction
        let clip_args = ClipRecordArgs {
            recording_equipment: specific_stuff.recording_equipment,
            settings: *args.column_args.matrix_record_settings,
        };
        let (clip_id, instruction) = if let Some((_, content)) = self.contents.first() {
            // There's a clip already. That makes it easy because we have the clip struct
            // already, including the complete clip supplier chain, and can reuse it.
            (
                content.clip.id().clone(),
                SlotRecordInstruction::ExistingClip(clip_args),
            )
        } else {
            // There's no clip yet so we need to create the clip including the complete supplier
            // chain from scratch. We need to do create much of the stuff here already because
            // we must not allocate in the real-time thread. However, we can't create the
            // complete clip because we don't have enough information (block length, timeline
            // frame rate) available at this point to resolve the initial recording position.
            let recording_args = RecordingArgs::from_stuff(
                Some(project),
                args.rt_column_settings,
                args.column_args.overridable_matrix_settings,
                &clip_args.settings,
                clip_args.recording_equipment,
            );
            let timeline = clip_timeline(Some(project), false);
            let timeline_cursor_pos = timeline.cursor_pos();
            let recorder = Recorder::recording(
                recording_args,
                args.column_args.recorder_request_sender.clone(),
            );
            let supplier_chain =
                SupplierChain::new(recorder, args.column_args.chain_equipment.clone())?;
            let clip_id = ClipId::random();
            let new_clip_instruction = RecordNewClipInstruction {
                clip_id: RtClipId::from_clip_id(&clip_id),
                supplier_chain,
                project: Some(project),
                shared_pos: Default::default(),
                shared_peak: Default::default(),
                timeline,
                timeline_cursor_pos,
                settings: *args.column_args.matrix_record_settings,
            };
            (
                clip_id,
                SlotRecordInstruction::NewClip(new_clip_instruction),
            )
        };
        let next_state = SlotState::RequestedRecording(RequestedRecordingState { clip_id });
        // Above code was only for checking preconditions and preparing stuff.
        // Here we can't fail anymore, do the actual state changes and distribute tasks.
        self.initiate_recording(
            args.column_command_sender,
            args.column_args.handler,
            next_state,
            instruction,
            common_stuff.temporary_route,
            common_stuff.task,
        );
        Ok(())
    }

    fn record_as_midi_overdub(
        &mut self,
        column_command_sender: &ColumnCommandSender,
        handler: &dyn ClipMatrixHandler,
        common_stuff: CommonRecordStuff,
        specific_stuff: MidiOverdubRecordStuff,
    ) -> ClipEngineResult<()> {
        let content = get_content_mut(&mut self.contents, specific_stuff.instruction.clip_index)?;
        content
            .online_data
            .as_ref()
            .ok_or("clip to be overdubbed is offline")?;
        self.initiate_recording(
            column_command_sender,
            handler,
            SlotState::RequestedOverdubbing,
            SlotRecordInstruction::MidiOverdub(specific_stuff.instruction),
            common_stuff.temporary_route,
            common_stuff.task,
        );
        Ok(())
    }

    fn initiate_recording(
        &mut self,
        column_command_sender: &ColumnCommandSender,
        handler: &dyn ClipMatrixHandler,
        next_state: SlotState,
        instruction: SlotRecordInstruction,
        temporary_route: Option<TrackRoute>,
        task: ClipRecordTask,
    ) {
        // 1. The main slot needs to know what's going on.
        self.state = next_state;
        // 2. The real-time slot needs to be prepared.
        column_command_sender.record_clip(self.index, instruction);
        // 3. The context needs to deliver our input.
        handler.request_recording_input(task);
        // 4. When recording track output, we must set up a send.
        // TODO-medium For reasons of clean rollback, we should create the route here, not above.
        self.temporary_route = temporary_route;
    }

    fn remove_temporary_route(&mut self) {
        if let Some(route) = self.temporary_route.take() {
            route.delete().unwrap();
        }
    }

    /// Adjusts the section length of all contained clips that are online.
    ///
    /// # Errors
    ///
    /// Returns an error if this slot doesn't contain any clip.
    pub fn adjust_section_length(
        &mut self,
        factor: f64,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<()> {
        for (i, content) in get_contents_mut(&mut self.contents)?
            .values_mut()
            .enumerate()
        {
            let Some(online_data) = content.online_data.as_mut() else {
                continue;
            };
            let current_section = content.clip.section();
            let current_length = if let Some(current_length) = current_section.length {
                current_length.get()
            } else {
                online_data.runtime_data.material_info.duration().get()
            };
            let new_section = api::Section {
                start_pos: current_section.start_pos,
                length: Some(PositiveSecond::new(current_length * factor)?),
            };
            content.clip.set_section(new_section);
            // TODO-high-multiclips CONTINUE Pass clip index
            column_command_sender.set_clip_section(self.index, i, new_section);
        }
        Ok(())
    }

    /// Returns whether this slot contains freezable clips.
    pub fn is_freezeable(&self) -> bool {
        self.contents.values().any(|content| content.is_freezable())
    }

    /// Freezes all clips in this slot.
    ///
    /// Doesn't error if the slot is empty.
    pub async fn freeze(&mut self, playback_track: &Track) -> ClipEngineResult<()> {
        for content in self.contents.values_mut() {
            if !content.is_freezable() {
                continue;
            }
            content.freeze(playback_track).await?;
        }
        Ok(())
    }

    /// Starts editing of all online clips contained in this slot.
    pub fn start_editing_clip(
        &mut self,
        clip_index: usize,
        playback_track: &Track,
    ) -> ClipEngineResult<()> {
        let content = get_content_mut(&mut self.contents, clip_index)?;
        let online_data = content.online_data.as_mut().ok_or("clip not online")?;
        let is_midi = online_data.runtime_data.material_info.is_midi();
        let clip_manifestation =
            manifest_clip_on_track(&content.clip, online_data, playback_track)?;
        let edit_session = if is_midi {
            // open_midi_editor_via_action(temporary_project, item);
            let hwnd = open_midi_editor_directly(playback_track, clip_manifestation.take)?;
            ClipEditSession::Midi(MidiClipEditSession::new(clip_manifestation, hwnd))
        } else {
            open_audio_editor(playback_track.project(), clip_manifestation.item)?;
            ClipEditSession::Audio(AudioClipEditSession { clip_manifestation })
        };
        online_data.edit_session = Some(edit_session);
        Ok(())
    }

    /// Stops editing of all online clips contained in this slot.
    pub fn stop_editing_clip(&mut self, clip_index: usize) -> ClipEngineResult<()> {
        let content = get_content_mut(&mut self.contents, clip_index)?;
        let online_data = content.online_data.as_mut().ok_or("clip not online")?;
        online_data.edit_session = None;
        Ok(())
    }

    pub fn is_editing_clip(&self, clip_index: usize) -> bool {
        self.get_content(clip_index)
            .ok()
            .and_then(|c| c.online_data.as_ref())
            .map(|d| d.edit_session.is_some())
            .unwrap_or(false)
    }

    /// Returns all clips in this slot. Can be empty.
    pub fn clips(&self) -> impl Iterator<Item = &Clip> {
        self.contents.values().map(|c| &c.clip)
    }

    /// Returns all clips in this slot, converted to standalone API clips. Can be empty.
    pub fn api_clips(&self, _permanent_project: Option<Project>) -> Vec<api::Clip> {
        self.contents
            .values()
            .filter_map(|c| c.clip.save().ok())
            .collect()
    }

    /// Returns clip at the given index.
    ///
    /// # Errors
    ///
    /// Returns an error if there's no clip at that index.
    pub fn get_clip(&self, index: usize) -> ClipEngineResult<&Clip> {
        Ok(&self.get_content(index)?.clip)
    }

    /// Returns the content at the given clip index.
    fn get_content(&self, index: usize) -> ClipEngineResult<&Content> {
        Ok(self.contents.get_index(index).ok_or(CLIP_DOESNT_EXIST)?.1)
    }

    /// Returns the clip at the given index, mutable.
    pub fn get_clip_mut(&mut self, index: usize) -> ClipEngineResult<&mut Clip> {
        let content = get_content_mut(&mut self.contents, index)?;
        Ok(&mut content.clip)
    }

    /// Returns volume of the first clip.
    ///
    /// # Errors
    ///
    /// Returns an error if this slot is empty.
    pub fn volume(&self) -> ClipEngineResult<Db> {
        Ok(self.get_content(0)?.clip.volume())
    }

    /// Returns looped setting of the first clip.
    ///
    /// # Errors
    ///
    /// Returns an error if this slot is empty.
    pub fn looped(&self) -> ClipEngineResult<bool> {
        Ok(self.get_content(0)?.clip.looped())
    }

    /// Sets volume of all clips.
    ///
    /// # Errors
    ///
    /// Returns an error if this slot is empty.
    pub fn set_volume(
        &mut self,
        volume: Db,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<ClipChangeEvent> {
        for (i, content) in get_contents_mut(&mut self.contents)?
            .values_mut()
            .enumerate()
        {
            content.clip.set_volume(volume);
            column_command_sender.set_clip_volume(self.index, i, volume);
        }
        Ok(ClipChangeEvent::Volume(volume))
    }

    /// Toggles the looped setting of all clips, using the setting of the first one as reference.
    ///
    /// # Errors
    ///
    /// Returns an error if this slot is empty.
    pub fn toggle_looped(
        &mut self,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<ClipChangeEvent> {
        let new_looped_value = !self.get_content(0)?.clip.looped();
        for (i, content) in self.contents.values_mut().enumerate() {
            content.clip.set_looped(new_looped_value);
            let args = ColumnSetClipLoopedArgs {
                slot_index: self.index,
                clip_index: i,
                looped: new_looped_value,
            };
            column_command_sender.set_clip_looped(args);
        }
        Ok(ClipChangeEvent::Looped(new_looped_value))
    }

    pub fn play_state(&self) -> InternalClipPlayState {
        use SlotState::*;
        match &self.state {
            Normal => self.play_state,
            RequestedOverdubbing | RequestedRecording(_) => {
                ClipPlayState::ScheduledForRecordingStart.into()
            }
            // TODO-high-clip-engine Couldn't we use the slot play state here, too?
            Recording(s) => s.runtime_data.play_state,
        }
    }

    /// Returns whether this slot is playing/recording something at the moment and therefore can be
    /// stopped.
    pub fn is_stoppable(&self) -> bool {
        self.play_state().is_stoppable()
    }

    pub fn update_play_state(&mut self, play_state: InternalClipPlayState) {
        self.play_state = play_state;
    }

    pub fn update_material_info(
        &mut self,
        clip_id: RtClipId,
        material_info: MaterialInfo,
    ) -> ClipEngineResult<()> {
        let content = self
            .contents
            .get_mut(&clip_id)
            .ok_or("clip doesn't exist")?;
        let online_data = content.online_data.as_mut().ok_or("clip is not online")?;
        online_data.runtime_data.material_info = material_info;
        Ok(())
    }

    /// Returns the currently relevant content.
    ///
    /// If the slot is recording, that's the runtime data of the recording. Otherwise, it's
    /// the content for each clip. Doesn't error if there's no clip.
    pub fn relevant_contents(
        &self,
    ) -> RelevantContent<impl Iterator<Item = &Content> + ExactSizeIterator> {
        if let SlotState::Recording(s) = &self.state {
            RelevantContent::Recording(&s.runtime_data)
        } else {
            RelevantContent::Normal(self.contents.values())
        }
    }

    pub fn notify_recording_request_acknowledged(
        &mut self,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    ) -> ClipEngineResult<()> {
        let runtime_data = match result {
            Ok(r) => r,
            Err(_) => {
                debug!("Recording request acknowledged with negative result");
                self.remove_temporary_route();
                self.state = SlotState::Normal;
                return Ok(());
            }
        };
        use SlotState::*;
        match mem::replace(&mut self.state, Normal) {
            Normal => Err("recording was not requested"),
            RequestedOverdubbing => {
                debug!("Acknowledged overdubbing");
                Ok(())
            }
            RequestedRecording(s) => {
                debug!("Acknowledged real recording");
                let runtime_data = runtime_data.expect("no runtime data sent back");
                self.state = {
                    // This must be a real recording, not overdub.
                    let recording_state = RecordingState {
                        clip_id: s.clip_id,
                        runtime_data,
                    };
                    SlotState::Recording(recording_state)
                };
                Ok(())
            }
            Recording(_) => Err("recording already"),
        }
    }

    pub fn notify_midi_overdub_finished(
        &mut self,
        clip_id: RtClipId,
        outcome: MidiOverdubOutcome,
    ) -> ClipEngineResult<()> {
        self.remove_temporary_route();
        get_content_mut_by_id(&mut self.contents, clip_id)?
            .clip
            .notify_midi_overdub_finished(outcome);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.contents.clear();
    }

    pub fn slot_cleared(&mut self) -> Option<SlotChangeEvent> {
        if self.is_empty() {
            return None;
        }
        self.contents.clear();
        Some(SlotChangeEvent::Clips("clip removed"))
    }

    pub fn notify_normal_recording_finished(
        &mut self,
        outcome: NormalRecordingOutcome,
        temporary_project: Option<Project>,
        recording_track: &Track,
    ) -> ClipEngineResult<SlotChangeEvent> {
        self.remove_temporary_route();
        match outcome {
            NormalRecordingOutcome::Committed(recording) => match mem::take(&mut self.state) {
                SlotState::Normal => Err("slot was not recording"),
                SlotState::RequestedOverdubbing => Err("requested overdubbing"),
                SlotState::RequestedRecording(_) => Err("clip recording was not yet acknowledged"),
                SlotState::Recording(mut s) => {
                    let clip = Clip::from_recording(
                        s.clip_id,
                        recording.kind_specific,
                        recording.clip_settings,
                        temporary_project,
                        recording_track,
                    )?;
                    s.runtime_data.material_info = recording.material_info;
                    debug!("Record slot with clip: {:#?}", &clip);
                    let content = Content {
                        clip,
                        online_data: Some(OnlineData {
                            runtime_data: s.runtime_data,
                            edit_session: None,
                        }),
                    };
                    self.contents.clear();
                    self.contents.insert(content.clip.rt_id(), content);
                    self.state = SlotState::Normal;
                    Ok(SlotChangeEvent::Clips("clip recording finished"))
                }
            },
            NormalRecordingOutcome::Canceled => {
                debug!("Recording canceled");
                self.state = SlotState::Normal;
                Ok(SlotChangeEvent::Clips("recording canceled"))
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
enum SlotState {
    /// Either empty or filled.
    ///
    /// Can be overdubbing (check play state).
    #[default]
    Normal,
    /// Used to prevent double invocation during overdubbing acknowledgement phase.
    RequestedOverdubbing,
    /// Used to prevent double invocation during recording acknowledgement phase.
    RequestedRecording(RequestedRecordingState),
    /// Recording (not overdubbing).
    Recording(RecordingState),
}

#[derive(Clone, Debug)]
struct RequestedRecordingState {
    clip_id: ClipId,
}

#[derive(Clone, Debug)]
struct RecordingState {
    clip_id: ClipId,
    runtime_data: SlotRuntimeData,
}

impl SlotState {
    pub fn is_pretty_much_recording(&self) -> bool {
        !matches!(self, Self::Normal)
    }
}

fn get_contents_mut(contents: &mut Contents) -> ClipEngineResult<&mut Contents> {
    if contents.is_empty() {
        return Err(SLOT_NOT_FILLED);
    }
    Ok(contents)
}

fn get_content_mut(contents: &mut Contents, clip_index: usize) -> ClipEngineResult<&mut Content> {
    let content = contents
        .get_index_mut(clip_index)
        .ok_or(CLIP_DOESNT_EXIST)?
        .1;
    Ok(content)
}

fn get_content_mut_by_id(
    contents: &mut Contents,
    clip_id: RtClipId,
) -> ClipEngineResult<&mut Content> {
    contents.get_mut(&clip_id).ok_or(CLIP_DOESNT_EXIST)
}

struct CommonRecordStuff {
    task: ClipRecordTask,
    temporary_route: Option<TrackRoute>,
}

// TODO-high-clip-engine Maybe fix the clippy warning
#[allow(clippy::large_enum_variant)]
enum ModeSpecificRecordStuff {
    FromScratch(FromScratchRecordStuff),
    MidiOverdub(MidiOverdubRecordStuff),
}

struct FromScratchRecordStuff {
    recording_equipment: RecordingEquipment,
}

struct MidiOverdubRecordStuff {
    instruction: MidiOverdubInstruction,
}

fn create_record_stuff(
    slot_index: usize,
    containing_track: Option<&Track>,
    matrix_record_settings: &MatrixClipRecordSettings,
    column_settings: &ColumnClipRecordSettings,
    recording_track: &Track,
    column_source: &SharedRtColumn,
    desired_midi_overdub_instruction: Option<MidiOverdubInstruction>,
) -> ClipEngineResult<(CommonRecordStuff, ModeSpecificRecordStuff)> {
    let (input, temporary_route) = {
        use RecordOrigin::*;
        match &column_settings.origin {
            TrackInput => {
                debug!("Input: track input");
                let track_input = recording_track
                    .recording_input()
                    .ok_or("track doesn't have any recording input")?;
                let hw_input = translate_track_input_to_hw_input(track_input)?;
                (ClipRecordInput::HardwareInput(hw_input), None)
            }
            TrackAudioOutput => {
                debug!("Input: track audio output");
                let containing_track = containing_track.ok_or(
                    "can't recording track audio output if Playtime runs in monitoring FX chain",
                )?;
                let route = recording_track.add_send_to(containing_track);
                // TODO-medium At the moment, we support stereo routes only. In order to support
                //  multi-channel routes, the user must increase the ReaLearn track channel count.
                //  And we have to:
                //  1. Create multi-channel sends (I_SRCCHAN, I_DSTCHAN)
                //  2. Make sure our ReaLearn instance has enough input pins. Roughly like this:
                // // In VST plug-in
                // let low_context = reaper_low::VstPluginContext::new(self.host.raw_callback().unwrap());
                // let context = VstPluginContext::new(&low_context);
                // let channel_count = unsafe {
                //     context.request_containing_track_channel_count(
                //         NonNull::new(self.host.raw_effect()).unwrap(),
                //     )
                // };
                // unsafe {
                //     (*self.host.raw_effect()).numInputs = channel_count;
                // }
                let channel_range = ChannelRange {
                    first_channel_index: 0,
                    channel_count: recording_track.channel_count(),
                };
                let fx_input = VirtualClipRecordAudioInput::Specific(channel_range);
                (ClipRecordInput::FxInput(fx_input), Some(route))
            }
            FxAudioInput(range) => {
                debug!("Input: FX audio input");
                let fx_input = VirtualClipRecordAudioInput::Specific(*range);
                (ClipRecordInput::FxInput(fx_input), None)
            }
        }
    };
    let recording_equipment = input.create_recording_equipment(
        Some(recording_track.project()),
        matrix_record_settings.midi_settings.auto_quantize,
    )?;
    let final_midi_overdub_instruction = if recording_equipment.is_midi() {
        desired_midi_overdub_instruction
    } else {
        // Want overdub but we have a audio input, so don't use overdub mode after all.
        None
    };
    let task = ClipRecordTask {
        input,
        destination: ClipRecordDestination {
            column_source: column_source.downgrade(),
            slot_index,
            is_midi_overdub: final_midi_overdub_instruction.is_some(),
        },
    };
    let mode_specific_stuff = if let Some(instruction) = final_midi_overdub_instruction {
        ModeSpecificRecordStuff::MidiOverdub(MidiOverdubRecordStuff { instruction })
    } else {
        ModeSpecificRecordStuff::FromScratch(FromScratchRecordStuff {
            recording_equipment,
        })
    };
    let common_stuff = CommonRecordStuff {
        task,
        temporary_route,
    };
    Ok((common_stuff, mode_specific_stuff))
}

const SLOT_NOT_FILLED: &str = "slot not filled";

fn translate_track_input_to_hw_input(
    track_input: RecordingInput,
) -> ClipEngineResult<ClipRecordHardwareInput> {
    let hw_input = match track_input {
        RecordingInput::Mono(i) => {
            let range = ChannelRange {
                first_channel_index: i,
                channel_count: 1,
            };
            ClipRecordHardwareInput::Audio(VirtualClipRecordAudioInput::Specific(range))
        }
        RecordingInput::Stereo(i) => {
            let range = ChannelRange {
                first_channel_index: i,
                channel_count: 2,
            };
            ClipRecordHardwareInput::Audio(VirtualClipRecordAudioInput::Specific(range))
        }
        RecordingInput::Midi { device_id, channel } => {
            let input = ClipRecordHardwareMidiInput { device_id, channel };
            ClipRecordHardwareInput::Midi(VirtualClipRecordHardwareMidiInput::Specific(input))
        }
        _ => return Err(""),
    };
    Ok(hw_input)
}

pub fn create_midi_overdub_instruction(
    clip_index: usize,
    mode: MidiClipRecordMode,
    auto_quantize: bool,
    api_source: &api::Source,
    temporary_project: Option<Project>,
) -> ClipEngineResult<MidiOverdubInstruction> {
    let quantization_settings = if auto_quantize {
        // TODO-high-clip-engine Use project quantization settings
        Some(QuantizationSettings {})
    } else {
        None
    };
    let (mirror_source, source_replacement) = match api_source {
        api::Source::File(file_based_api_source) => {
            // We have a file-based MIDI source only. In the real-time clip, we need to replace
            // it with an equivalent in-project MIDI source first. Create it!
            let in_project_source = create_pcm_source_from_file_based_api_source(
                temporary_project,
                file_based_api_source,
                true,
            )?;
            let chunk = in_project_source.state_chunk();
            let midi_sequence = MidiSequence::parse_from_reaper_midi_chunk(&chunk)
                .map_err(|_| "couldn't parse MidiSequence from API MIDI file")?;
            (midi_sequence.clone(), Some(midi_sequence))
        }
        api::Source::MidiChunk(s) => {
            // We have an in-project MIDI source already. Great!
            let midi_sequence = MidiSequence::parse_from_reaper_midi_chunk(&s.chunk)
                .map_err(|_| "couldn't parse MidiSequence from API chunk")?;
            (midi_sequence, None)
        }
    };
    // TODO-high We need to enlarge capacity of the MidiSequences in the recorder in order to avoid
    //  allocation. That also means we should probably ALWAYS send a source replacement!
    let instruction = MidiOverdubInstruction {
        clip_index,
        source_replacement,
        settings: MidiOverdubSettings {
            mode,
            quantization_settings,
            mirror_source,
        },
    };
    Ok(instruction)
}

fn open_midi_editor_directly(editor_track: &Track, take: Take) -> ClipEngineResult<Hwnd> {
    let source = take.source().ok_or("take has no source")?;
    unsafe {
        source
            .as_raw()
            .ext_open_editor(Reaper::get().main_window(), editor_track.index().unwrap())
            .unwrap();
    }
    configure_midi_editor();
    let hwnd = Reaper::get()
        .medium_reaper()
        .midi_editor_get_active()
        .ok_or("couldn't find focused MIDI editor")?;
    Ok(hwnd)
}

#[allow(dead_code)]
fn open_midi_editor_via_action(project: Project, item: Item) -> ClipEngineResult<()> {
    project.select_item_exclusively(item);
    // Open built-in MIDI editor
    let open_midi_editor_command_id = CommandId::new(40153);
    // Open items in primary external editor
    // let open_midi_editor_command_id = CommandId::new(40109);
    Reaper::get()
        .main_section()
        .action_by_command_id(open_midi_editor_command_id)
        .invoke_as_trigger(item.project())?;
    configure_midi_editor();
    Ok(())
}

fn open_audio_editor(project: Project, item: Item) -> ClipEngineResult<()> {
    project.select_item_exclusively(item);
    // Toggle zoom to selected items
    let open_midi_editor_command_id = CommandId::new(41622);
    Reaper::get()
        .main_section()
        .action_by_command_id(open_midi_editor_command_id)
        .invoke_as_trigger(item.project())?;
    Ok(())
}

fn configure_midi_editor() {
    let reaper = Reaper::get().medium_reaper();
    let required_view_mode = RequiredViewMode::Normal;
    // // Switch piano roll time base to "Source beats" if not already happened.
    // let midi_editor_section_id = SectionId::new(32060);
    // let source_beats_command_id = CommandId::new(40470);
    // if reaper.get_toggle_command_state_ex(midi_editor_section_id, source_beats_command_id)
    //     != Some(true)
    // {
    //     let _ =
    //         reaper.midi_editor_last_focused_on_command(source_beats_command_id, required_view_mode);
    // }
    // Zoom to content
    let zoom_command_id = CommandId::new(40466);
    let _ = reaper.midi_editor_last_focused_on_command(zoom_command_id, required_view_mode);
}

const CLIP_DOESNT_EXIST: &str = "clip doesn't exist";

pub enum RelevantContent<'a, T> {
    /// Contains one content per clip.
    Normal(T),
    /// Contains runtime data of the recording.
    Recording(&'a SlotRuntimeData),
}

impl<'a, T: Iterator<Item = &'a Content>> RelevantContent<'a, T> {
    /// Returns the proportional position of the recording or first clip.
    pub fn primary_proportional_position(self) -> ClipEngineResult<UnitValue> {
        match self {
            RelevantContent::Normal(mut contents) => {
                // We use the position of the first clip only.
                contents
                    .next()
                    .ok_or("slot empty")?
                    .online_data
                    .as_ref()
                    .ok_or("clip offline")?
                    .proportional_position()
            }
            RelevantContent::Recording(runtime_data) => runtime_data.proportional_position(),
        }
    }

    /// Returns the position of the recording or first clip.
    pub fn primary_position_in_seconds(
        self,
        timeline_tempo: Bpm,
    ) -> ClipEngineResult<PositionInSeconds> {
        let res = match self {
            RelevantContent::Normal(mut contents) => {
                // We use the position of the first clip only.
                let content = contents.next().ok_or("slot empty")?;
                content
                    .online_data
                    .as_ref()
                    .ok_or("clip offline")?
                    .position_in_seconds(&content.clip, timeline_tempo)
            }
            RelevantContent::Recording(runtime_data) => {
                runtime_data.position_in_seconds_during_recording(timeline_tempo)
            }
        };
        Ok(res)
    }
}

fn load_api_clips(
    api_clips: Vec<api::Clip>,
    id_mode: IdMode,
) -> impl Iterator<Item = (RtClipId, Content)> {
    api_clips.into_iter().map(move |mut api_clip| {
        if id_mode == IdMode::AssignNewIds {
            api_clip.id = ClipId::random();
        }
        let clip = Clip::load(api_clip);
        (clip.rt_id(), Content::new(clip))
    })
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum IdMode {
    /// Keep object IDs.
    ///
    /// This should be used if it's important to track the "journey" of objects. This is then
    /// used in the real-time column to decide whether a change affects an existing clip or is
    /// something new. This in turn makes interruption-free playing possible in many cases, e.g.
    /// undo/redo.
    KeepIds,
    /// Assign new objects IDs.
    ///
    /// This should be used whenever there's the danger of duplicate IDs. The rule is:
    /// IDs must be unique across the whole matrix. E.g. clip IDs should not just be unique within
    /// one slot but across all columns!
    AssignNewIds,
}

pub struct EssentialSlotRecordClipArgs<'a> {
    pub column_args: EssentialColumnRecordClipArgs<'a>,
    pub column_record_settings: &'a ColumnClipRecordSettings,
    pub rt_column_settings: &'a rt::RtColumnSettings,
    pub recording_track: &'a Track,
    pub rt_column: &'a SharedRtColumn,
    pub column_command_sender: &'a ColumnCommandSender,
}

/// Returns `true` if the clip source has changed and needs to be synced to the real-time
/// column.
pub fn apply_edited_content_if_necessary(
    online_data: &mut OnlineData,
    clip: &mut Clip,
) -> ClipEngineResult<bool> {
    // Check if content changed
    let midi_edit_session = online_data.midi_edit_session_mut()?;
    let changed = if midi_edit_session.update_source_hash() {
        let chunk = midi_edit_session.clip_manifestation().state_chunk()?;
        let updated_source = api::Source::MidiChunk(MidiChunkSource { chunk });
        clip.set_source(updated_source);
        true
    } else {
        false
    };
    // Remove edit session if MIDI editor closed
    if !midi_edit_session.midi_editor_window().is_open() {
        online_data.edit_session = None;
    }
    Ok(changed)
}
