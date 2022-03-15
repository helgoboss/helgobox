use crate::main::{
    Clip, ClipMatrixHandler, ClipRecordDestination, ClipRecordFxInput, ClipRecordHardwareInput,
    ClipRecordHardwareMidiInput, ClipRecordInput, ClipRecordTask, MatrixSettings, Slot, SlotState,
    VirtualClipRecordAudioInput, VirtualClipRecordHardwareMidiInput,
};
use crate::rt::supplier::{
    ChainEquipment, RecordTiming, Recorder, RecorderRequest, RecordingArgs, SupplierChain,
};
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ClipRecordArgs, ColumnCommandSender, ColumnEvent,
    ColumnFillSlotArgs, ColumnPlayClipArgs, ColumnSetClipLoopedArgs, ColumnStopClipArgs,
    MidiOverdubInstruction, OverridableMatrixSettings, RecordNewClipInstruction, SharedColumn,
    SlotRecordInstruction, WeakColumn,
};
use crate::{clip_timeline, rt, ClipEngineResult, Timeline};
use crossbeam_channel::{Receiver, Sender};
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ChannelRange, ColumnClipPlayAudioSettings, ColumnClipPlaySettings, ColumnClipRecordSettings,
    Db, MatrixClipRecordSettings, MidiClipRecordMode, RecordOrigin,
};
use reaper_high::{Guid, OrCurrentProject, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, Bpm, CustomPcmSource, FlexibleOwnedPcmSource, MeasureAlignment,
    OwnedPreviewRegister, PositionInSeconds, ReaperMutex, ReaperVolumeValue, RecordingInput,
};
use std::ptr::NonNull;
use std::sync::Arc;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    settings: ColumnSettings,
    rt_settings: rt::ColumnSettings,
    rt_command_sender: ColumnCommandSender,
    rt_column: SharedColumn,
    preview_register: Option<PlayingPreviewRegister>,
    slots: Vec<Slot>,
    event_receiver: Receiver<ColumnEvent>,
    project: Option<Project>,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    pub clip_record_settings: ColumnClipRecordSettings,
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    _preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
    track: Option<Track>,
}

impl Column {
    pub fn new(permanent_project: Option<Project>) -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(500);
        let (event_sender, event_receiver) = crossbeam_channel::bounded(500);
        let source = rt::Column::new(permanent_project, command_receiver, event_sender);
        let shared_source = SharedColumn::new(source);
        Self {
            settings: Default::default(),
            rt_settings: Default::default(),
            // preview_register: {
            //     PlayingPreviewRegister::new(shared_source.clone(), track.as_ref())
            // },
            preview_register: None,
            rt_column: shared_source,
            rt_command_sender: ColumnCommandSender::new(command_sender),
            slots: vec![],
            event_receiver,
            project: permanent_project,
        }
    }

    pub fn rt_command_sender(&self) -> ColumnCommandSender {
        self.rt_command_sender.clone()
    }

    pub fn load(
        &mut self,
        api_column: api::Column,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &MatrixSettings,
    ) -> ClipEngineResult<()> {
        self.clear_slots();
        // Track
        let track = if let Some(id) = api_column.clip_play_settings.track.as_ref() {
            let guid = Guid::from_string_without_braces(id.get())?;
            Some(permanent_project.or_current_project().track_by_guid(&guid))
        } else {
            None
        };
        self.preview_register = Some(PlayingPreviewRegister::new(self.rt_column.clone(), track));
        // Settings
        self.rt_settings.audio_resample_mode =
            api_column.clip_play_settings.audio_settings.resample_mode;
        self.rt_settings.audio_time_stretch_mode = api_column
            .clip_play_settings
            .audio_settings
            .time_stretch_mode;
        self.rt_settings.audio_cache_behavior =
            api_column.clip_play_settings.audio_settings.cache_behavior;
        self.rt_settings.play_mode = api_column.clip_play_settings.mode.unwrap_or_default();
        self.rt_settings.clip_play_start_timing = api_column.clip_play_settings.start_timing;
        self.rt_settings.clip_play_stop_timing = api_column.clip_play_settings.stop_timing;
        self.rt_command_sender
            .update_settings(self.rt_settings.clone());
        self.rt_command_sender
            .update_matrix_settings(matrix_settings.overridable.clone());
        // Slots
        for api_slot in api_column.slots.unwrap_or_default() {
            if let Some(api_clip) = api_slot.clip {
                let clip = Clip::load(api_clip);
                self.fill_slot_internal(
                    api_slot.row,
                    clip,
                    permanent_project,
                    chain_equipment,
                    recorder_request_sender,
                    matrix_settings,
                )?;
            }
        }
        Ok(())
    }

    pub fn clear_slots(&mut self) {
        self.slots.clear();
        self.rt_command_sender.clear_slots();
    }

    /// Is mutable because empty slots are created lazily up to `row_count`.
    pub(super) fn slot(&mut self, index: usize, row_count: usize) -> Option<&Slot> {
        self.upsize_if_necessary(row_count);
        self.slots.get(index)
    }

    fn upsize_if_necessary(&mut self, row_count: usize) {
        if self.slots.len() < row_count {
            self.slots.resize_with(row_count, Default::default);
        }
    }

    /// Returns the actual number of slots in this column.
    ///
    /// Just interesting for internal usage. For external usage, the matrix row count is important.
    pub(super) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn save(&self) -> api::Column {
        let track_id = self.preview_register.as_ref().and_then(|reg| {
            reg.track
                .as_ref()
                .map(|t| t.guid().to_string_without_braces())
                .map(api::TrackId::new)
        });
        api::Column {
            clip_play_settings: ColumnClipPlaySettings {
                mode: Some(self.rt_settings.play_mode),
                track: track_id,
                start_timing: self.rt_settings.clip_play_start_timing,
                stop_timing: self.rt_settings.clip_play_stop_timing,
                audio_settings: ColumnClipPlayAudioSettings {
                    resample_mode: self.rt_settings.audio_resample_mode.clone(),
                    time_stretch_mode: self.rt_settings.audio_time_stretch_mode.clone(),
                    cache_behavior: self.rt_settings.audio_cache_behavior.clone(),
                },
            },
            clip_record_settings: ColumnClipRecordSettings {
                track: None,
                origin: RecordOrigin::TrackInput,
            },
            slots: {
                let slots = self
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        if let Some(clip) = s.clip() {
                            let api_slot = api::Slot {
                                row: i,
                                clip: Some(clip.save()),
                            };
                            Some(api_slot)
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(slots)
            },
        }
    }

    pub fn rt_column(&self) -> WeakColumn {
        self.rt_column.downgrade()
    }

    fn fill_slot_internal(
        &mut self,
        row: usize,
        mut clip: Clip,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &MatrixSettings,
    ) -> ClipEngineResult<()> {
        let rt_clip = clip.create_and_connect_real_time_clip(
            permanent_project,
            chain_equipment,
            recorder_request_sender,
            &matrix_settings.overridable,
            &self.rt_settings,
        )?;
        let slot = get_slot_mut_insert(&mut self.slots, row);
        slot.fill_with(clip);
        let args = ColumnFillSlotArgs {
            slot_index: row,
            clip: rt_clip,
        };
        self.rt_command_sender.fill_slot(Box::new(Some(args)));
        Ok(())
    }

    pub fn poll(&mut self, _timeline_tempo: Bpm) -> Vec<(usize, ClipChangedEvent)> {
        // Process source events and generate clip change events
        let mut change_events = vec![];
        while let Ok(evt) = self.event_receiver.try_recv() {
            use ColumnEvent::*;
            let change_event = match evt {
                ClipPlayStateChanged {
                    slot_index,
                    play_state,
                } => {
                    if let Ok(clip) = get_clip_mut_insert_slot(&mut self.slots, slot_index) {
                        let _ = clip.update_play_state(play_state);
                    }
                    Some((slot_index, ClipChangedEvent::PlayState(play_state)))
                }
                ClipMaterialInfoChanged {
                    slot_index,
                    material_info,
                } => {
                    if let Ok(clip) = get_clip_mut_insert_slot(&mut self.slots, slot_index) {
                        let _ = clip.update_material_info(material_info);
                    }
                    None
                }
                Dispose(_) => None,
                RecordRequestAcknowledged {
                    slot_index,
                    successful,
                    ..
                } => {
                    let slot = get_slot_mut_insert(&mut self.slots, slot_index);
                    slot.notify_recording_request_acknowledged(successful)
                        .unwrap();
                    None
                }
                MidiOverdubFinished {
                    slot_index,
                    mirror_source,
                } => {
                    let slot = get_slot_mut_insert(&mut self.slots, slot_index);
                    slot.notify_midi_overdub_finished(mirror_source, self.project)
                        .unwrap();
                    None
                }
                ColumnEvent::NormalRecordingFinished {
                    slot_index,
                    outcome,
                } => {
                    let slot = get_slot_mut_insert(&mut self.slots, slot_index);
                    slot.notify_normal_recording_finished(outcome, self.project)
                        .unwrap();
                    None
                }
            };
            if let Some(evt) = change_event {
                change_events.push(evt);
            }
        }
        // Add position updates
        let pos_change_events = self.slots.iter().enumerate().filter_map(|(row, slot)| {
            let clip = slot.clip()?;
            if clip.play_state().ok()?.is_advancing() {
                let proportional_pos = clip.proportional_pos().unwrap_or(UnitValue::MIN);
                let event = ClipChangedEvent::ClipPosition(proportional_pos);
                Some((row, event))
            } else {
                None
            }
        });
        change_events.extend(pos_change_events);
        change_events
    }

    pub fn play_clip(&self, args: ColumnPlayClipArgs) {
        self.rt_command_sender.play_clip(args);
    }

    pub fn stop_clip(&self, args: ColumnStopClipArgs) {
        self.rt_command_sender.stop_clip(args);
    }

    pub fn pause_clip(&self, slot_index: usize) {
        self.rt_command_sender.pause_clip(slot_index);
    }

    pub fn seek_clip(&self, slot_index: usize, desired_pos: UnitValue) {
        self.rt_command_sender.seek_clip(slot_index, desired_pos);
    }

    pub fn set_clip_volume(&mut self, slot_index: usize, volume: Db) -> ClipEngineResult<()> {
        let clip = get_clip_mut_insert_slot(&mut self.slots, slot_index)?;
        clip.set_volume(volume);
        self.rt_command_sender.set_clip_volume(slot_index, volume);
        Ok(())
    }

    pub fn toggle_clip_looped(&mut self, slot_index: usize) -> ClipEngineResult<ClipChangedEvent> {
        let clip = get_clip_mut_insert_slot(&mut self.slots, slot_index)?;
        let looped = clip.toggle_looped();
        let args = ColumnSetClipLoopedArgs { slot_index, looped };
        self.rt_command_sender.set_clip_looped(args);
        Ok(ClipChangedEvent::ClipLooped(looped))
    }

    pub fn clip_position_in_seconds(
        &self,
        slot_index: usize,
    ) -> ClipEngineResult<PositionInSeconds> {
        let clip = get_clip(&self.slots, slot_index)?;
        let timeline = clip_timeline(self.project, false);
        clip.position_in_seconds(&timeline)
    }

    pub fn clip_volume(&self, slot_index: usize) -> ClipEngineResult<Db> {
        let clip = get_clip(&self.slots, slot_index)?;
        Ok(clip.volume())
    }

    pub fn clip_play_state(&self, slot_index: usize) -> ClipEngineResult<ClipPlayState> {
        let slot = get_slot(&self.slots, slot_index)?;
        slot.play_state()
    }

    pub fn clip_looped(&self, slot_index: usize) -> ClipEngineResult<bool> {
        let clip = get_clip(&self.slots, slot_index)?;
        Ok(clip.looped())
    }

    pub fn proportional_clip_position(&self, slot_index: usize) -> ClipEngineResult<UnitValue> {
        let clip = get_clip(&self.slots, slot_index)?;
        clip.proportional_pos()
    }

    pub fn record_clip<H: ClipMatrixHandler>(
        &mut self,
        slot_index: usize,
        matrix_record_settings: &MatrixClipRecordSettings,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        handler: &H,
        containing_track: Option<&Track>,
        overridable_matrix_settings: &OverridableMatrixSettings,
    ) -> ClipEngineResult<()> {
        // Insert slot if it doesn't exist already.
        let slot = get_slot_mut_insert(&mut self.slots, slot_index);
        // Check preconditions.
        let (has_existing_clip, midi_overdub_mirror_source) = match slot.state() {
            SlotState::Empty => (false, None),
            SlotState::RecordingFromScratchRequested => {
                return Err("recording requested already (from scratch)");
            }
            SlotState::RecordingFromScratch => {
                return Err("recording already (from scratch)");
            }
            SlotState::Filled(clip) => {
                if clip.recording_requested() {
                    return Err("recording requested already (with existing clip)");
                }
                if clip.play_state() == Ok(ClipPlayState::Recording) {
                    return Err("recording already (with existing clip)");
                }
                use MidiClipRecordMode::*;
                let want_midi_overdub = match matrix_record_settings.midi_settings.record_mode {
                    Normal => false,
                    Overdub => {
                        // Only allow MIDI overdub is existing clip is a MIDI clip already.
                        clip.material_info().map(|i| i.is_midi()).unwrap_or(false)
                    }
                    Replace => todo!(),
                };
                let mirror_source = if want_midi_overdub {
                    Some(clip.create_mirror_source_for_midi_overdub(self.project)?)
                } else {
                    None
                };
                (true, mirror_source)
            }
        };
        // Prepare tasks, equipment, instructions.
        let record_task = create_clip_record_task(
            slot_index,
            containing_track,
            &self.settings,
            self.project,
            self.preview_register.as_ref(),
            &self.rt_column,
        )?;
        let recording_equipment = record_task.input.create_recording_equipment(self.project);
        let input_is_midi = recording_equipment.is_midi();
        let midi_overdub_mirror_source = if input_is_midi {
            midi_overdub_mirror_source
        } else {
            // Want overdub but we have a audio input, so don't use overdub mode after all.
            None
        };
        let instruction = if let Some(mirror_source) = midi_overdub_mirror_source {
            // We can do MIDI overdub. This is the easiest thing and needs almost no preparation.
            let args = MidiOverdubInstruction { mirror_source };
            SlotRecordInstruction::MidiOverdub(args)
        } else {
            // We record completely new material.
            let args = ClipRecordArgs {
                recording_equipment,
                settings: matrix_record_settings.clone(),
            };
            if has_existing_clip {
                // There's a clip already. That makes it easy because we have the clip struct
                // already, including the complete clip supplier chain, and can reuse it.
                SlotRecordInstruction::ExistingClip(args)
            } else {
                // There's no clip yet so we need to create the clip including the complete supplier
                // chain from scratch. We need to do create much of the stuff here already because
                // we must not allocate in the real-time thread. However, we can't create the
                // complete clip because we don't have enough information (block length, timeline
                // frame rate) available at this point to resolve the initial recording position.
                let timeline = clip_timeline(self.project, false);
                let timeline_cursor_pos = timeline.cursor_pos();
                let tempo = timeline.tempo_at(timeline_cursor_pos);
                let initial_play_start_timing = self
                    .rt_settings
                    .clip_play_start_timing
                    .unwrap_or(overridable_matrix_settings.clip_play_start_timing);
                let timing = RecordTiming::from_args(
                    &args,
                    &timeline,
                    timeline_cursor_pos,
                    initial_play_start_timing,
                );
                let recording_args = RecordingArgs {
                    equipment: args.recording_equipment,
                    project: self.project,
                    timeline_cursor_pos,
                    tempo,
                    time_signature: timeline.time_signature_at(timeline_cursor_pos),
                    detect_downbeat: matrix_record_settings
                        .downbeat_detection_enabled(input_is_midi),
                    timing,
                };
                let recorder = Recorder::recording(recording_args, recorder_request_sender.clone());
                let supplier_chain = SupplierChain::new(recorder, chain_equipment.clone())?;
                let new_clip_instruction = RecordNewClipInstruction {
                    supplier_chain,
                    project: self.project,
                    shared_pos: Default::default(),
                    timeline,
                    timeline_cursor_pos,
                    timing,
                    is_midi: input_is_midi,
                    settings: matrix_record_settings.clone(),
                    initial_play_start_timing,
                };
                SlotRecordInstruction::NewClip(new_clip_instruction)
            }
        };
        // Above code was only for checking preconditions and preparing stuff.
        // Here we do the actual state changes and distribute tasks.
        slot.notify_recording_requested()?;
        self.rt_command_sender.record_clip(slot_index, instruction);
        handler.request_recording_input(record_task);
        Ok(())
    }
}

impl Drop for PlayingPreviewRegister {
    fn drop(&mut self) {
        self.stop_playing_preview();
    }
}

impl PlayingPreviewRegister {
    pub fn new(source: impl CustomPcmSource + 'static, track: Option<Track>) -> Self {
        let mut register = OwnedPreviewRegister::default();
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        let (out_chan, preview_track) = if let Some(t) = track.as_ref() {
            (-1, Some(t.raw()))
        } else {
            (0, None)
        };
        register.set_out_chan(out_chan);
        register.set_preview_track(preview_track);
        let source = create_custom_owned_pcm_source(source);
        register.set_src(Some(FlexibleOwnedPcmSource::Custom(source)));
        let preview_register = Arc::new(ReaperMutex::new(register));
        let play_handle = start_playing_preview(&preview_register, track.as_ref());
        Self {
            _preview_register: preview_register,
            play_handle,
            track,
        }
    }

    fn stop_playing_preview(&mut self) {
        if let Some(track) = &self.track {
            // Check prevents error message on project close.
            let project = track.project();
            // If not successful this probably means it was stopped already, so okay.
            let _ = Reaper::get()
                .medium_session()
                .stop_track_preview_2(project.context(), self.play_handle);
        } else {
            // If not successful this probably means it was stopped already, so okay.
            let _ = Reaper::get()
                .medium_session()
                .stop_preview(self.play_handle);
        };
    }
}

fn start_playing_preview(
    reg: &SharedRegister,
    track: Option<&Track>,
) -> NonNull<preview_register_t> {
    debug!("Starting preview on track {:?}", &track);
    let buffering_behavior = BitFlags::empty();
    let measure_alignment = MeasureAlignment::PlayImmediately;
    let result = if let Some(track) = track {
        Reaper::get().medium_session().play_track_preview_2_ex(
            track.project().context(),
            reg.clone(),
            buffering_behavior,
            measure_alignment,
        )
    } else {
        Reaper::get().medium_session().play_preview_ex(
            reg.clone(),
            buffering_behavior,
            measure_alignment,
        )
    };
    result.unwrap()
}

fn get_clip(slots: &[Slot], slot_index: usize) -> ClipEngineResult<&Clip> {
    get_slot(slots, slot_index)?.clip().ok_or(SLOT_NOT_FILLED)
}

fn get_slot(slots: &[Slot], slot_index: usize) -> ClipEngineResult<&Slot> {
    slots.get(slot_index).ok_or(SLOT_DOESNT_EXIST)
}

fn get_clip_mut_insert_slot(
    slots: &mut Vec<Slot>,
    slot_index: usize,
) -> ClipEngineResult<&mut Clip> {
    get_slot_mut_insert(slots, slot_index)
        .clip_mut()
        .ok_or(SLOT_NOT_FILLED)
}

fn get_slot_mut_insert(slots: &mut Vec<Slot>, slot_index: usize) -> &mut Slot {
    if slot_index >= slots.len() {
        slots.resize_with(slot_index + 1, Default::default);
    }
    slots.get_mut(slot_index).unwrap()
}

const SLOT_DOESNT_EXIST: &str = "slot doesn't exist";
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

fn create_clip_record_task(
    slot_index: usize,
    containing_track: Option<&Track>,
    column_settings: &ColumnSettings,
    project: Option<Project>,
    preview_register: Option<&PlayingPreviewRegister>,
    column_source: &SharedColumn,
) -> ClipEngineResult<ClipRecordTask> {
    let task = ClipRecordTask {
        input: {
            use RecordOrigin::*;
            match &column_settings.clip_record_settings.origin {
                TrackInput => {
                    let track =
                        resolve_recording_track(column_settings, project, preview_register)?;
                    let track_input = track
                        .recording_input()
                        .ok_or("track doesn't have any recording input")?;
                    let hw_input = translate_track_input_to_hw_input(track_input)?;
                    ClipRecordInput::HardwareInput(hw_input)
                }
                TrackAudioOutput => {
                    let track =
                        resolve_recording_track(column_settings, project, preview_register)?;
                    let containing_track = containing_track.ok_or("can't recording track audio output if Playtime runs in monitoring FX chain")?;
                    track.add_send_to(containing_track);
                    let channel_range = ChannelRange {
                        first_channel_index: 0,
                        channel_count: track.channel_count(),
                    };
                    ClipRecordInput::FxInput(ClipRecordFxInput::Audio(
                        VirtualClipRecordAudioInput::Specific(channel_range),
                    ))
                }
                FxAudioInput(range) => ClipRecordInput::FxInput(ClipRecordFxInput::Audio(
                    VirtualClipRecordAudioInput::Specific(*range),
                )),
                FxMidiInput => ClipRecordInput::FxInput(ClipRecordFxInput::Midi),
            }
        },
        destination: ClipRecordDestination {
            column_source: column_source.downgrade(),
            slot_index,
        },
    };
    Ok(task)
}

fn resolve_recording_track(
    column_settings: &ColumnSettings,
    project: Option<Project>,
    preview_register: Option<&PlayingPreviewRegister>,
) -> ClipEngineResult<Track> {
    if let Some(track_id) = &column_settings.clip_record_settings.track {
        let track_guid = Guid::from_string_without_braces(track_id.get())?;
        let track = project.or_current_project().track_by_guid(&track_guid);
        if track.is_available() {
            Ok(track)
        } else {
            Err("track not available")
        }
    } else {
        let register = preview_register.ok_or("column inactive")?;
        register.track.clone().ok_or("no playback track set")
    }
}
