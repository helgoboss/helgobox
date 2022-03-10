use crate::main::{
    Clip, ClipMatrixHandler, ClipRecordDestination, ClipRecordFxInput, ClipRecordHardwareInput,
    ClipRecordHardwareMidiInput, ClipRecordInput, ClipRecordTask, ClipRecordTiming, MatrixSettings,
    Slot, VirtualClipRecordAudioInput, VirtualClipRecordHardwareMidiInput,
};
use crate::mutex_util::non_blocking_lock;
use crate::rt::supplier::{ChainPreBufferRequest, PreBufferRequest, RecorderEquipment};
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ClipRecordArgs, ClipRecordInputKind, ColumnCommandSender,
    ColumnEvent, ColumnFillSlotArgs, ColumnPlayClipArgs, ColumnSetClipLoopedArgs,
    ColumnStopClipArgs, RecordBehavior, SharedColumn, WeakColumn,
};
use crate::{clip_timeline, rt, ClipEngineResult};
use crossbeam_channel::{Receiver, Sender};
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, ChannelRange, ClipPlayStartTiming,
    ColumnClipPlayAudioSettings, ColumnClipPlaySettings, ColumnClipRecordSettings, Db,
    MatrixClipRecordSettings, RecordOrigin, VirtualResampleMode,
};
use reaper_high::{Guid, OrCurrentProject, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, Bpm, CustomPcmSource, FlexibleOwnedPcmSource, MeasureAlignment,
    OwnedPreviewRegister, PositionInSeconds, ReaperMutex, ReaperVolumeValue, RecordingInput,
};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    settings: ColumnSettings,
    rt_settings: rt::ColumnSettings,
    rt_command_sender: ColumnCommandSender,
    column_source: SharedColumn,
    preview_register: Option<PlayingPreviewRegister>,
    slots: Vec<Slot>,
    event_receiver: Receiver<ColumnEvent>,
    project: Option<Project>,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    pub audio_resample_mode: Option<VirtualResampleMode>,
    pub audio_time_stretch_mode: Option<AudioTimeStretchMode>,
    pub audio_cache_behavior: Option<AudioCacheBehavior>,
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
            column_source: shared_source,
            rt_command_sender: ColumnCommandSender::new(command_sender),
            slots: vec![],
            event_receiver,
            project: permanent_project,
        }
    }

    pub fn load(
        &mut self,
        api_column: api::Column,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
        pre_buffer_request_sender: &Sender<ChainPreBufferRequest>,
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
        self.preview_register = Some(PlayingPreviewRegister::new(
            self.column_source.clone(),
            track,
        ));
        // Settings
        self.settings.audio_resample_mode =
            api_column.clip_play_settings.audio_settings.resample_mode;
        self.settings.audio_time_stretch_mode = api_column
            .clip_play_settings
            .audio_settings
            .time_stretch_mode;
        self.settings.audio_cache_behavior =
            api_column.clip_play_settings.audio_settings.cache_behavior;
        self.rt_settings.play_mode = api_column.clip_play_settings.mode.unwrap_or_default();
        self.rt_settings.clip_play_start_timing = api_column.clip_play_settings.start_timing;
        self.rt_settings.clip_play_stop_timing = api_column.clip_play_settings.stop_timing;
        self.rt_command_sender
            .update_settings(self.rt_settings.clone());
        // Slots
        for api_slot in api_column.slots.unwrap_or_default() {
            if let Some(api_clip) = api_slot.clip {
                let clip = Clip::load(api_clip);
                self.fill_slot_internal(
                    api_slot.row,
                    clip,
                    permanent_project,
                    recorder_equipment,
                    pre_buffer_request_sender,
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

    pub fn slot(&self, index: usize) -> Option<&Slot> {
        self.slots.get(index)
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
                start_timing: None,
                stop_timing: None,
                audio_settings: ColumnClipPlayAudioSettings {
                    resample_mode: self.settings.audio_resample_mode.clone(),
                    time_stretch_mode: self.settings.audio_time_stretch_mode.clone(),
                    cache_behavior: self.settings.audio_cache_behavior.clone(),
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
                        if let Some(clip) = &s.clip {
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

    pub fn source(&self) -> WeakColumn {
        self.column_source.downgrade()
    }

    fn fill_slot_internal(
        &mut self,
        row: usize,
        mut clip: Clip,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
        pre_buffer_request_sender: &Sender<ChainPreBufferRequest>,
        matrix_settings: &MatrixSettings,
    ) -> ClipEngineResult<()> {
        let rt_clip = clip.create_real_time_clip(
            permanent_project,
            recorder_equipment,
            pre_buffer_request_sender,
            matrix_settings,
            &self.settings,
        )?;
        clip.connect_to(&rt_clip);
        get_slot_mut(&mut self.slots, row).clip = Some(clip);
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
                    if let Ok(clip) = get_clip_mut(&mut self.slots, slot_index) {
                        let _ = clip.update_play_state(play_state);
                    }
                    Some((slot_index, ClipChangedEvent::PlayState(play_state)))
                }
                ClipMaterialInfoChanged {
                    slot_index,
                    material_info,
                } => {
                    if let Ok(clip) = get_clip_mut(&mut self.slots, slot_index) {
                        let _ = clip.update_material_info(material_info);
                    }
                    None
                }
                Dispose(_) => None,
            };
            if let Some(evt) = change_event {
                change_events.push(evt);
            }
        }
        // Add position updates
        let pos_change_events = self.slots.iter().enumerate().filter_map(|(row, slot)| {
            let clip = slot.clip.as_ref()?;
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
        let clip = get_clip_mut(&mut self.slots, slot_index)?;
        clip.set_volume(volume);
        self.rt_command_sender.set_clip_volume(slot_index, volume);
        Ok(())
    }

    pub fn toggle_clip_looped(&mut self, slot_index: usize) -> ClipEngineResult<ClipChangedEvent> {
        let clip = get_clip_mut(&mut self.slots, slot_index)?;
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
        let clip = get_clip(&self.slots, slot_index)?;
        clip.play_state()
    }

    pub fn clip_repeated(&self, slot_index: usize) -> ClipEngineResult<bool> {
        let clip = get_clip(&self.slots, slot_index)?;
        Ok(clip.data().looped)
    }

    pub fn proportional_clip_position(&self, slot_index: usize) -> ClipEngineResult<UnitValue> {
        let clip = get_clip(&self.slots, slot_index)?;
        clip.proportional_pos()
    }

    pub fn record_clip<H: ClipMatrixHandler>(
        &self,
        slot_index: usize,
        matrix_settings: &MatrixClipRecordSettings,
        equipment: &RecorderEquipment,
        pre_buffer_request_sender: &Sender<ChainPreBufferRequest>,
        handler: &H,
        containing_track: Option<&Track>,
        parent_play_start_timing: ClipPlayStartTiming,
    ) -> ClipEngineResult<()> {
        // Prepare record task (for delivering the material to be recorded)
        let task = self.create_clip_record_task(slot_index, containing_track)?;
        let input_kind = task.input.derive_kind();
        let args = ClipRecordArgs {
            parent_play_start_timing: self
                .rt_settings
                .clip_play_start_timing
                .unwrap_or(parent_play_start_timing),
            input_kind,
            start_timing: matrix_settings.start_timing.clone(),
            midi_record_mode: matrix_settings.midi_settings.record_mode,
            length: matrix_settings.duration.clone(),
            looped: matrix_settings.looped,
            detect_downbeat: if input_kind.is_midi() {
                matrix_settings.midi_settings.detect_downbeat
            } else {
                matrix_settings.audio_settings.detect_downbeat
            },
            equipment: equipment.clone(),
            pre_buffer_request_sender: pre_buffer_request_sender.clone(),
            project: self.project,
        };
        self.rt_command_sender.record_clip(slot_index, args);
        handler.request_recording_input(task);
        Ok(())
    }

    fn create_clip_record_task(
        &self,
        slot_index: usize,
        containing_track: Option<&Track>,
    ) -> ClipEngineResult<ClipRecordTask> {
        let task = ClipRecordTask {
            input: {
                use RecordOrigin::*;
                match &self.settings.clip_record_settings.origin {
                    TrackInput => {
                        let track = self.resolve_recording_track()?;
                        let track_input = track
                            .recording_input()
                            .ok_or("track doesn't have any recording input")?;
                        let hw_input = translate_track_input_to_hw_input(track_input)?;
                        ClipRecordInput::HardwareInput(hw_input)
                    }
                    TrackAudioOutput => {
                        let track = self.resolve_recording_track()?;
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
                column_source: self.column_source.downgrade(),
                slot_index,
            },
        };
        Ok(task)
    }

    fn resolve_recording_track(&self) -> ClipEngineResult<Track> {
        if let Some(track_id) = &self.settings.clip_record_settings.track {
            let track_guid = Guid::from_string_without_braces(track_id.get())?;
            let track = self.project.or_current_project().track_by_guid(&track_guid);
            if track.is_available() {
                Ok(track)
            } else {
                Err("track not available")
            }
        } else {
            let register = self.preview_register.as_ref().ok_or("column inactive")?;
            register.track.clone().ok_or("no playback track set")
        }
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
    get_slot(slots, slot_index)?
        .clip
        .as_ref()
        .ok_or(SLOT_NOT_FILLED)
}

fn get_slot(slots: &[Slot], slot_index: usize) -> ClipEngineResult<&Slot> {
    slots.get(slot_index).ok_or(SLOT_DOESNT_EXIST)
}

fn get_clip_mut(slots: &mut Vec<Slot>, slot_index: usize) -> ClipEngineResult<&mut Clip> {
    get_slot_mut(slots, slot_index)
        .clip
        .as_mut()
        .ok_or(SLOT_NOT_FILLED)
}

fn get_slot_mut(slots: &mut Vec<Slot>, slot_index: usize) -> &mut Slot {
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
