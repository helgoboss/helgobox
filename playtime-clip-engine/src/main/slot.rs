use crate::conversion_util::{
    adjust_duration_in_secs_anti_proportionally, adjust_pos_in_secs_anti_proportionally,
    convert_position_in_frames_to_seconds,
};
use crate::main::{
    create_api_source_from_recorded_midi_source, Clip, ClipMatrixHandler, ClipRecordDestination,
    ClipRecordHardwareInput, ClipRecordHardwareMidiInput, ClipRecordInput, ClipRecordTask,
    VirtualClipRecordAudioInput, VirtualClipRecordHardwareMidiInput,
};
use crate::rt::supplier::{
    ChainEquipment, MaterialInfo, MidiOverdubSettings, QuantizationSettings, Recorder,
    RecorderRequest, RecordingArgs, RecordingEquipment, SupplierChain,
};
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ClipRecordArgs, ColumnCommandSender, ColumnSetClipLoopedArgs,
    MidiOverdubInstruction, NormalRecordingOutcome, OverridableMatrixSettings,
    RecordNewClipInstruction, SharedColumn, SlotRecordInstruction, SlotRuntimeData,
};
use crate::source_util::create_pcm_source_from_file_based_api_source;
use crate::{clip_timeline, rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ChannelRange, ColumnClipRecordSettings, Db, MatrixClipRecordSettings, MidiClipRecordMode,
    RecordOrigin,
};
use reaper_high::{Guid, Item, OwnedSource, Project, Reaper, Take, Track, TrackRoute};
use reaper_medium::{
    Bpm, CommandId, DurationInSeconds, OwnedPcmSource, PositionInSeconds, RecordingInput,
    RequiredViewMode, SectionId, TrackArea, UiRefreshBehavior,
};
use std::mem;

#[derive(Clone, Debug)]
pub struct Slot {
    index: usize,
    /// If this is set, the slot contains a clip.
    ///
    /// This means one of the following things:
    ///
    /// - The clip is active and can be playing, stopped etc.
    /// - The clip is active and is currently being MIDI-overdubbed.
    /// - The clip is inactive, which means it's about to be replaced with different clip content
    ///   that's in the process of being recorded right now.
    content: Option<Content>,
    state: SlotState,
    /// Route which was created temporarily for recording.
    temporary_route: Option<TrackRoute>,
}

#[derive(Clone, Debug)]
struct Content {
    clip: Clip,
    runtime_data: SlotRuntimeData,
    /// A copy of the real-time MIDI source. Only set for in-project MIDI, not file MIDI.
    ///
    /// With this, in-project MIDI sources can be opened in the MIDI editor and editing there
    /// has immediate effects. For this to work, the source must be a pooled copy!
    ///
    /// Now that we have pooled MIDI anyway, we don't need to send a finished MIDI recording back
    /// to the main thread using the "mirror source" method (which we did before).
    pooled_midi_source: Option<OwnedSource>,
}

impl Content {
    pub fn length_in_seconds(
        &self,
        timeline: &HybridTimeline,
    ) -> ClipEngineResult<DurationInSeconds> {
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        let tempo_factor = self.tempo_factor(timeline_tempo);
        let tempo_adjusted_secs = adjust_duration_in_secs_anti_proportionally(
            self.runtime_data.material_info.duration(),
            tempo_factor,
        );
        Ok(tempo_adjusted_secs)
    }

    pub fn tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        let is_midi = self.runtime_data.material_info.is_midi();
        self.clip.tempo_factor(timeline_tempo, is_midi)
    }
}

impl Slot {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            content: None,
            state: Default::default(),
            temporary_route: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_none() && !self.state.is_pretty_much_recording()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns `None` if this slot doesn't need to be saved (because it's empty).
    pub fn save(&self, temporary_project: Option<Project>) -> Option<api::Slot> {
        let content = self.content.as_ref()?;
        let is_recording = self.state.is_pretty_much_recording()
            || self
                .get_content()
                .ok()?
                .runtime_data
                .play_state
                .is_somehow_recording();
        let pooled_midi_source = if is_recording {
            // When recording, we don't want to interfere with the pooled MIDI that's being
            // changed at the very moment. Also, we don't want to save "uncommitted" data, so
            // we save the last known "stable" MIDI contents.
            None
        } else {
            // When not recording, we inspect the pooled MIDI source.
            content.pooled_midi_source.as_ref()
        };
        let clip = content
            .clip
            .save(pooled_midi_source, temporary_project)
            .ok()?;
        let api_slot = api::Slot {
            row: self.index,
            clip: Some(clip),
        };
        Some(api_slot)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_clip<H: ClipMatrixHandler>(
        &mut self,
        matrix_record_settings: &MatrixClipRecordSettings,
        column_record_settings: &ColumnClipRecordSettings,
        rt_column_settings: &rt::ColumnSettings,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        handler: &H,
        containing_track: Option<&Track>,
        overridable_matrix_settings: &OverridableMatrixSettings,
        playback_track: &Track,
        rt_column: &SharedColumn,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<()> {
        if self.state.is_pretty_much_recording() {
            return Err("recording already");
        }
        // Check preconditions and prepare stuff.
        let project = playback_track.project();
        let desired_midi_overdub_instruction = if let Some(content) = &self.content {
            if content.runtime_data.play_state.is_somehow_recording() {
                return Err("recording already according to play state");
            }
            use MidiClipRecordMode::*;
            let want_midi_overdub = match matrix_record_settings.midi_settings.record_mode {
                Normal => false,
                Overdub | Replace => {
                    // Only allow MIDI overdub if existing clip is a MIDI clip already.
                    content.runtime_data.material_info.is_midi()
                }
            };
            if want_midi_overdub {
                let instruction = create_midi_overdub_instruction(
                    matrix_record_settings.midi_settings.record_mode,
                    matrix_record_settings.midi_settings.auto_quantize,
                    content.clip.api_source(),
                    Some(project),
                )?;
                Some(instruction)
            } else {
                None
            }
        } else {
            None
        };
        let (common_stuff, mode_specific_stuff) = create_record_stuff(
            self.index,
            containing_track,
            matrix_record_settings,
            column_record_settings,
            playback_track,
            rt_column,
            desired_midi_overdub_instruction,
        )?;
        match mode_specific_stuff {
            ModeSpecificRecordStuff::FromScratch(from_scratch_stuff) => self.record_from_scratch(
                column_command_sender,
                handler,
                matrix_record_settings,
                overridable_matrix_settings,
                rt_column_settings,
                recorder_request_sender,
                chain_equipment,
                project,
                common_stuff,
                from_scratch_stuff,
            ),
            ModeSpecificRecordStuff::MidiOverdub(midi_overdub_stuff) => self
                .record_as_midi_overdub(
                    column_command_sender,
                    handler,
                    project,
                    common_stuff,
                    midi_overdub_stuff,
                ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_from_scratch<H: ClipMatrixHandler>(
        &mut self,
        column_command_sender: &ColumnCommandSender,
        handler: &H,
        matrix_record_settings: &MatrixClipRecordSettings,
        overridable_matrix_settings: &OverridableMatrixSettings,
        rt_column_settings: &rt::ColumnSettings,
        recorder_request_sender: &Sender<RecorderRequest>,
        chain_equipment: &ChainEquipment,
        project: Project,
        common_stuff: CommonRecordStuff,
        specific_stuff: FromScratchRecordStuff,
    ) -> ClipEngineResult<()> {
        // Build slot instruction
        let args = ClipRecordArgs {
            recording_equipment: specific_stuff.recording_equipment,
            settings: *matrix_record_settings,
        };
        let instruction = if self.content.is_some() {
            // There's a clip already. That makes it easy because we have the clip struct
            // already, including the complete clip supplier chain, and can reuse it.
            SlotRecordInstruction::ExistingClip(args)
        } else {
            // There's no clip yet so we need to create the clip including the complete supplier
            // chain from scratch. We need to do create much of the stuff here already because
            // we must not allocate in the real-time thread. However, we can't create the
            // complete clip because we don't have enough information (block length, timeline
            // frame rate) available at this point to resolve the initial recording position.
            let recording_args = RecordingArgs::from_stuff(
                Some(project),
                rt_column_settings,
                overridable_matrix_settings,
                &args.settings,
                args.recording_equipment,
            );
            let timeline = clip_timeline(Some(project), false);
            let timeline_cursor_pos = timeline.cursor_pos();
            let recorder = Recorder::recording(recording_args, recorder_request_sender.clone());
            let supplier_chain = SupplierChain::new(recorder, chain_equipment.clone())?;
            let new_clip_instruction = RecordNewClipInstruction {
                supplier_chain,
                project: Some(project),
                shared_pos: Default::default(),
                timeline,
                timeline_cursor_pos,
                settings: *matrix_record_settings,
            };
            SlotRecordInstruction::NewClip(new_clip_instruction)
        };
        let next_state = SlotState::RequestedRecording(RequestedRecordingState {
            pooled_midi_source: specific_stuff.pooled_midi_source,
        });
        // Above code was only for checking preconditions and preparing stuff.
        // Here we can't fail anymore, do the actual state changes and distribute tasks.
        self.initiate_recording(
            column_command_sender,
            handler,
            next_state,
            instruction,
            common_stuff.temporary_route,
            common_stuff.task,
        );
        Ok(())
    }

    fn record_as_midi_overdub<H: ClipMatrixHandler>(
        &mut self,
        column_command_sender: &ColumnCommandSender,
        handler: &H,
        project: Project,
        common_stuff: CommonRecordStuff,
        specific_stuff: MidiOverdubRecordStuff,
    ) -> ClipEngineResult<()> {
        // If we had a file-based source before and now have an in-project source, make a pooled
        // copy of the in-project source.
        let content = self
            .content
            .as_mut()
            .expect("content not set although overdubbing");
        let new_pooled_midi_source =
            if let Some(s) = &specific_stuff.instruction.in_project_midi_source {
                let s = Reaper::get().with_pref_pool_midi_when_duplicating(true, || s.clone());
                Some(OwnedSource::new(s))
            } else {
                None
            };
        let pooled_midi_source = new_pooled_midi_source.as_ref().unwrap_or_else(|| {
            content
                .pooled_midi_source
                .as_ref()
                .expect("pooled MIDI source not set although overdubbing")
        });
        let fresh_api_source =
            create_api_source_from_recorded_midi_source(pooled_midi_source, Some(project))?;
        // Above code was only for checking preconditions and preparing stuff.
        // Here we can't fail anymore, do the actual state changes and distribute tasks.
        if let Some(s) = new_pooled_midi_source {
            content.pooled_midi_source = Some(s);
        }
        content
            .clip
            .update_api_source_before_midi_overdubbing(fresh_api_source);
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

    fn initiate_recording<H: ClipMatrixHandler>(
        &mut self,
        column_command_sender: &ColumnCommandSender,
        handler: &H,
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

    fn get_content(&self) -> ClipEngineResult<&Content> {
        self.content.as_ref().ok_or(SLOT_NOT_FILLED)
    }

    pub fn start_editing_clip(&self, temporary_project: Project) -> ClipEngineResult<()> {
        let content = self.get_content()?;
        if !content.runtime_data.material_info.is_midi() {
            return Err("editing not supported for audio");
        }
        let timeline = clip_timeline(Some(temporary_project), false);
        let item_length = content.length_in_seconds(&timeline)?;
        let editor_track = find_or_create_editor_track(temporary_project);
        let midi_source = if let Some(s) = content.pooled_midi_source.as_ref() {
            Reaper::get().with_pref_pool_midi_when_duplicating(true, || s.clone())
        } else {
            OwnedSource::new(content.clip.create_pcm_source(Some(temporary_project))?)
        };
        let item = editor_track.add_item().map_err(|e| e.message())?;
        let take = item.add_take().map_err(|e| e.message())?;
        take.set_source(midi_source);
        item.set_position(PositionInSeconds::new(0.0), UiRefreshBehavior::NoRefresh)
            .unwrap();
        item.set_length(item_length, UiRefreshBehavior::NoRefresh)
            .unwrap();
        // open_midi_editor_via_action(temporary_project, item);
        open_midi_editor_directly(editor_track, take);
        Ok(())
    }

    pub fn stop_editing_clip(&self, temporary_project: Project) -> ClipEngineResult<()> {
        let content = self.get_content()?;
        let editor_track = find_editor_track(temporary_project).ok_or("editor track not found")?;
        let clip_item = find_clip_item(content, &editor_track).ok_or("clip item not found")?;
        let _ = unsafe {
            Reaper::get()
                .medium_reaper()
                .delete_track_media_item(editor_track.raw(), clip_item.raw())
        };
        if editor_track.item_count() == 0 {
            editor_track.project().remove_track(&editor_track);
        }
        Ok(())
    }

    pub fn is_editing_clip(&self, temporary_project: Project) -> bool {
        if let Some(content) = self.content.as_ref() {
            if let Some(editor_track) = find_editor_track(temporary_project) {
                find_clip_item(content, &editor_track).is_some()
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn clip_volume(&self) -> ClipEngineResult<Db> {
        Ok(self.get_content()?.clip.volume())
    }

    pub fn clip_looped(&self) -> ClipEngineResult<bool> {
        Ok(self.get_content()?.clip.looped())
    }

    pub fn set_clip_volume(
        &mut self,
        volume: Db,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<()> {
        let content = get_content_mut(&mut self.content)?;
        content.clip.set_volume(volume);
        column_command_sender.set_clip_volume(self.index, volume);
        Ok(())
    }

    pub fn toggle_clip_looped(
        &mut self,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<ClipChangedEvent> {
        let content = get_content_mut(&mut self.content)?;
        let looped = content.clip.toggle_looped();
        let args = ColumnSetClipLoopedArgs {
            slot_index: self.index,
            looped,
        };
        column_command_sender.set_clip_looped(args);
        Ok(ClipChangedEvent::ClipLooped(looped))
    }

    pub fn play_state(&self) -> ClipEngineResult<ClipPlayState> {
        use SlotState::*;
        match &self.state {
            Normal => Ok(self.get_content()?.runtime_data.play_state),
            RequestedOverdubbing | RequestedRecording(_) => {
                Ok(ClipPlayState::ScheduledForRecordingStart)
            }
            Recording(s) => Ok(s.runtime_data.play_state),
        }
    }

    fn runtime_data(&self) -> ClipEngineResult<&SlotRuntimeData> {
        if let SlotState::Recording(s) = &self.state {
            Ok(&s.runtime_data)
        } else {
            Ok(&self.get_content()?.runtime_data)
        }
    }

    fn runtime_data_mut(&mut self) -> ClipEngineResult<&mut SlotRuntimeData> {
        if let SlotState::Recording(s) = &mut self.state {
            Ok(&mut s.runtime_data)
        } else {
            Ok(&mut get_content_mut(&mut self.content)?.runtime_data)
        }
    }

    pub fn update_play_state(&mut self, play_state: ClipPlayState) -> ClipEngineResult<()> {
        self.runtime_data_mut()?.play_state = play_state;
        Ok(())
    }

    pub fn update_material_info(&mut self, material_info: MaterialInfo) -> ClipEngineResult<()> {
        self.runtime_data_mut()?.material_info = material_info;
        Ok(())
    }

    pub fn proportional_pos(&self) -> ClipEngineResult<UnitValue> {
        let runtime_data = self.runtime_data()?;
        let pos = runtime_data.pos.get();
        if pos < 0 {
            return Err("count-in phase");
        }
        let frame_count = runtime_data.material_info.frame_count();
        if frame_count == 0 {
            return Err("frame count is zero");
        }
        let mod_pos = pos as usize % frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Ok(proportional)
    }

    pub fn position_in_seconds(
        &self,
        timeline: &HybridTimeline,
    ) -> ClipEngineResult<PositionInSeconds> {
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        let (runtime_data, tempo_factor) = if let SlotState::Recording(s) = &self.state {
            let tempo_factor = s
                .runtime_data
                .material_info
                .tempo_factor_during_recording(timeline_tempo);
            (&s.runtime_data, tempo_factor)
        } else {
            let content = self.get_content()?;
            (&content.runtime_data, content.tempo_factor(timeline_tempo))
        };
        let pos_in_source_frames = runtime_data.mod_frame();
        let pos_in_secs = convert_position_in_frames_to_seconds(
            pos_in_source_frames,
            runtime_data.material_info.frame_rate(),
        );
        let tempo_adjusted_secs = adjust_pos_in_secs_anti_proportionally(pos_in_secs, tempo_factor);
        Ok(tempo_adjusted_secs)
    }

    pub(crate) fn fill_with(
        &mut self,
        clip: Clip,
        rt_clip: &rt::Clip,
        pooled_midi_source: Option<OwnedSource>,
    ) {
        let content = Content {
            clip,
            runtime_data: SlotRuntimeData {
                play_state: Default::default(),
                pos: rt_clip.shared_pos(),
                material_info: rt_clip.material_info().unwrap(),
            },
            pooled_midi_source,
        };
        self.content = Some(content);
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
                        pooled_midi_source: s.pooled_midi_source,
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
        mirror_source: OwnedPcmSource,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<()> {
        self.remove_temporary_route();
        get_content_mut(&mut self.content)?
            .clip
            .notify_midi_overdub_finished(&OwnedSource::new(mirror_source), temporary_project)
    }

    pub fn slot_cleared(&mut self) -> Option<ClipChangedEvent> {
        self.content.take().map(|_| ClipChangedEvent::Removed)
    }

    pub fn notify_normal_recording_finished(
        &mut self,
        outcome: NormalRecordingOutcome,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<Option<ClipChangedEvent>> {
        self.remove_temporary_route();
        match outcome {
            NormalRecordingOutcome::Committed(recording) => match mem::take(&mut self.state) {
                SlotState::Normal => Err("slot was not recording"),
                SlotState::RequestedOverdubbing => Err("requested overdubbing"),
                SlotState::RequestedRecording(_) => Err("clip recording was not yet acknowledged"),
                SlotState::Recording(mut s) => {
                    let clip = Clip::from_recording(
                        recording.kind_specific,
                        recording.clip_settings,
                        temporary_project,
                        s.pooled_midi_source.as_ref(),
                    )?;
                    s.runtime_data.material_info = recording.material_info;
                    debug!("Fill slot with clip: {:#?}", &clip);
                    let content = Content {
                        clip,
                        runtime_data: s.runtime_data,
                        pooled_midi_source: s.pooled_midi_source,
                    };
                    self.content = Some(content);
                    self.state = SlotState::Normal;
                    Ok(None)
                }
            },
            NormalRecordingOutcome::Canceled => {
                debug!("Recording canceled");
                self.state = SlotState::Normal;
                Ok(Some(ClipChangedEvent::Removed))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum SlotState {
    /// Either empty or filled.
    ///
    /// Can be overdubbing (check play state).
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
    pooled_midi_source: Option<OwnedSource>,
}

#[derive(Clone, Debug)]
struct RecordingState {
    /// This must be set for MIDI recordings.
    pooled_midi_source: Option<OwnedSource>,
    runtime_data: SlotRuntimeData,
}

impl SlotState {
    pub fn is_pretty_much_recording(&self) -> bool {
        !matches!(self, Self::Normal)
    }
}

impl Default for SlotState {
    fn default() -> Self {
        SlotState::Normal
    }
}

fn get_content_mut(content: &mut Option<Content>) -> ClipEngineResult<&mut Content> {
    content.as_mut().ok_or(SLOT_NOT_FILLED)
}

struct CommonRecordStuff {
    task: ClipRecordTask,
    temporary_route: Option<TrackRoute>,
}

enum ModeSpecificRecordStuff {
    FromScratch(FromScratchRecordStuff),
    MidiOverdub(MidiOverdubRecordStuff),
}

struct FromScratchRecordStuff {
    recording_equipment: RecordingEquipment,
    pooled_midi_source: Option<OwnedSource>,
}

struct MidiOverdubRecordStuff {
    instruction: MidiOverdubInstruction,
}

fn create_record_stuff(
    slot_index: usize,
    containing_track: Option<&Track>,
    matrix_record_settings: &MatrixClipRecordSettings,
    column_settings: &ColumnClipRecordSettings,
    playback_track: &Track,
    column_source: &SharedColumn,
    desired_midi_overdub_instruction: Option<MidiOverdubInstruction>,
) -> ClipEngineResult<(CommonRecordStuff, ModeSpecificRecordStuff)> {
    let (input, temporary_route) = {
        use RecordOrigin::*;
        match &column_settings.origin {
            TrackInput => {
                debug!("Input: track input");
                let track = resolve_recording_track(column_settings, playback_track)?;
                let track_input = track
                    .recording_input()
                    .ok_or("track doesn't have any recording input")?;
                let hw_input = translate_track_input_to_hw_input(track_input)?;
                (ClipRecordInput::HardwareInput(hw_input), None)
            }
            TrackAudioOutput => {
                debug!("Input: track audio output");
                let track = resolve_recording_track(column_settings, playback_track)?;
                let containing_track = containing_track.ok_or(
                    "can't recording track audio output if Playtime runs in monitoring FX chain",
                )?;
                let route = track.add_send_to(containing_track);
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
                    channel_count: track.channel_count(),
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
        Some(playback_track.project()),
        matrix_record_settings.midi_settings.auto_quantize,
    );
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
        let pooled_midi_source = match &recording_equipment {
            RecordingEquipment::Midi(e) => {
                Some(OwnedSource::new(e.create_pooled_copy_of_midi_source()))
            }
            RecordingEquipment::Audio(_) => None,
        };
        ModeSpecificRecordStuff::FromScratch(FromScratchRecordStuff {
            recording_equipment,
            pooled_midi_source,
        })
    };
    let common_stuff = CommonRecordStuff {
        task,
        temporary_route,
    };
    Ok((common_stuff, mode_specific_stuff))
}

fn resolve_recording_track(
    column_settings: &ColumnClipRecordSettings,
    playback_track: &Track,
) -> ClipEngineResult<Track> {
    if let Some(track_id) = &column_settings.track {
        let track_guid = Guid::from_string_without_braces(track_id.get())?;
        let track = playback_track.project().track_by_guid(&track_guid);
        if track.is_available() {
            Ok(track)
        } else {
            Err("track not available")
        }
    } else {
        Ok(playback_track.clone())
    }
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
    mode: MidiClipRecordMode,
    auto_quantize: bool,
    api_source: &api::Source,
    temporary_project: Option<Project>,
) -> ClipEngineResult<MidiOverdubInstruction> {
    let quantization_settings = if auto_quantize {
        // TODO-high Use project quantization settings
        Some(QuantizationSettings {})
    } else {
        None
    };
    let in_project_midi_source = match api_source {
        api::Source::File(file_based_api_source) => {
            // We have a file-based MIDI source only. In the real-time clip, we need to replace
            // it with an equivalent in-project MIDI source first. Create it!
            let in_project_source = create_pcm_source_from_file_based_api_source(
                temporary_project,
                file_based_api_source,
                true,
            )?;
            Some(in_project_source.into_raw())
        }
        api::Source::MidiChunk(_) => {
            // We have an in-project MIDI source already. Great!
            None
        }
    };

    let instruction = MidiOverdubInstruction {
        in_project_midi_source,
        settings: MidiOverdubSettings {
            mode,
            quantization_settings,
        },
    };
    Ok(instruction)
}

fn find_or_create_editor_track(project: Project) -> Track {
    find_editor_track(project).unwrap_or_else(|| {
        let track = project.add_track();
        track.set_shown(TrackArea::Mcp, false);
        track.set_shown(TrackArea::Tcp, false);
        track.set_name(EDITOR_TRACK_NAME);
        track
    })
}

fn find_editor_track(project: Project) -> Option<Track> {
    project.tracks().find(|t| {
        if let Some(name) = t.name() {
            name.to_str() == EDITOR_TRACK_NAME
        } else {
            false
        }
    })
}

const EDITOR_TRACK_NAME: &str = "playtime-editor-track";

fn item_refers_to_clip_content(item: Item, content: &Content) -> bool {
    let take = match item.active_take() {
        None => return false,
        Some(t) => t,
    };
    let source = match take.source() {
        None => return false,
        Some(s) => s,
    };
    if let Some(clip_source) = &content.pooled_midi_source {
        // TODO-medium Checks can be optimized (in terms of performance)
        clip_source.pooled_midi_id().map(|res| res.id) == source.pooled_midi_id().map(|res| res.id)
    } else if let api::Source::File(s) = &content.clip.api_source() {
        source
            .as_ref()
            .as_raw()
            .get_file_name(|n| if let Some(n) = n { n == s.path } else { false })
    } else {
        false
    }
}

fn item_is_open_in_midi_editor(item: Item) -> bool {
    let item_take = match item.active_take() {
        None => return false,
        Some(t) => t,
    };
    let reaper = Reaper::get().medium_reaper();
    let active_editor = match reaper.midi_editor_get_active() {
        None => return false,
        Some(e) => e,
    };
    let open_take = match unsafe { reaper.midi_editor_get_take(active_editor) } {
        Err(_) => return false,
        Ok(t) => t,
    };
    open_take == item_take.raw()
}

fn open_midi_editor_directly(editor_track: Track, take: Take) {
    if let Some(source) = take.source() {
        unsafe {
            source
                .as_raw()
                .ext_open_editor(Reaper::get().main_window(), editor_track.index().unwrap())
                .unwrap();
        }
        configure_midi_editor();
    }
}

#[allow(dead_code)]
fn open_midi_editor_via_action(project: Project, item: Item) {
    project.select_item_exclusively(item);
    let open_midi_editor_command_id = CommandId::new(40153);
    // let open_midi_editor_command_id = CommandId::new(40109);
    Reaper::get()
        .main_section()
        .action_by_command_id(open_midi_editor_command_id)
        .invoke_as_trigger(item.project());
    configure_midi_editor();
}

fn configure_midi_editor() {
    let reaper = Reaper::get().medium_reaper();
    let midi_editor_section_id = SectionId::new(32060);
    let source_beats_command_id = CommandId::new(40470);
    let zoom_command_id = CommandId::new(40466);
    let required_view_mode = RequiredViewMode::Normal;
    // Switch piano roll time base to "Source beats" if not already happened.
    if reaper.get_toggle_command_state_ex(midi_editor_section_id, source_beats_command_id)
        != Some(true)
    {
        let _ =
            reaper.midi_editor_last_focused_on_command(source_beats_command_id, required_view_mode);
    }
    // Zoom to content
    let _ = reaper.midi_editor_last_focused_on_command(zoom_command_id, required_view_mode);
}

fn find_clip_item(content: &Content, editor_track: &Track) -> Option<Item> {
    editor_track.items().find(|item| {
        item_refers_to_clip_content(*item, content) && item_is_open_in_midi_editor(*item)
    })
}
