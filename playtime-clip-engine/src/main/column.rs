use crate::main::{Clip, ClipContent, ClipData, ClipRecordTask, Slot};
use crate::rt::supplier::RecorderEquipment;
use crate::rt::{
    ClipChangedEvent, ClipInfo, ClipPlayState, ColumnFillSlotArgs, ColumnPlayClipArgs,
    ColumnSetClipRepeatedArgs, ColumnSource, ColumnSourceCommandSender, ColumnSourceEvent,
    RecordBehavior, SharedColumnSource, SlotProcessTransportChangeArgs, WeakColumnSource,
};
use crate::{rt, ClipEngineResult};
use crossbeam_channel::Receiver;
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ColumnClipPlayAudioSettings, ColumnClipPlaySettings, ColumnClipRecordSettings,
    TrackRecordOrigin,
};
use reaper_high::{Guid, OrCurrentProject, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, Bpm, CustomPcmSource, FlexibleOwnedPcmSource, MeasureAlignment,
    OwnedPreviewRegister, PositionInSeconds, ReaperMutex, ReaperVolumeValue,
};
use std::ptr::NonNull;
use std::sync::Arc;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    rt_settings: rt::ColumnSettings,
    rt_command_sender: ColumnSourceCommandSender,
    column_source: SharedColumnSource,
    preview_register: Option<PlayingPreviewRegister>,
    slots: Vec<Slot>,
    event_receiver: Receiver<ColumnSourceEvent>,
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
    track: Option<Track>,
}

impl Column {
    pub fn new(permanent_project: Option<Project>) -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(500);
        let (event_sender, event_receiver) = crossbeam_channel::bounded(500);
        let source = ColumnSource::new(permanent_project, command_receiver, event_sender);
        let shared_source = SharedColumnSource::new(source);
        Self {
            rt_settings: Default::default(),
            // preview_register: {
            //     PlayingPreviewRegister::new(shared_source.clone(), track.as_ref())
            // },
            preview_register: None,
            column_source: shared_source,
            rt_command_sender: ColumnSourceCommandSender::new(command_sender),
            slots: vec![],
            event_receiver,
        }
    }

    pub fn load(
        &mut self,
        api_column: api::Column,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
    ) -> ClipEngineResult<()> {
        self.clear_slots();
        // Track
        let track = if let Some(id) = api_column.clip_play_settings.track.as_ref() {
            let guid = Guid::from_string_without_braces(&id.0)?;
            Some(permanent_project.or_current_project().track_by_guid(&guid))
        } else {
            None
        };
        self.preview_register = Some(PlayingPreviewRegister::new(
            self.column_source.clone(),
            track,
        ));
        // Settings
        self.rt_settings.clip_play_start_timing = api_column.clip_play_settings.start_timing;
        self.rt_command_sender
            .update_settings(self.rt_settings.clone());
        // Slots
        for api_slot in api_column.slots.unwrap_or_default() {
            if let Some(api_clip) = api_slot.clip {
                let clip = Clip::load(api_clip);
                self.fill_slot_internal(api_slot.row, clip, permanent_project, recorder_equipment)?;
            }
        }
        Ok(())
    }

    pub fn clear_slots(&mut self) {
        self.slots.clear();
        self.rt_command_sender.clear_slots();
    }

    pub fn save(&self) -> api::Column {
        let track_id = self.preview_register.as_ref().and_then(|reg| {
            reg.track
                .as_ref()
                .map(|t| t.guid().to_string_without_braces())
                .map(api::TrackId)
        });
        api::Column {
            clip_play_settings: ColumnClipPlaySettings {
                track: track_id,
                start_timing: None,
                stop_timing: None,
                audio_settings: ColumnClipPlayAudioSettings {
                    time_stretch_mode: None,
                },
            },
            clip_record_settings: ColumnClipRecordSettings {
                track: None,
                origin: TrackRecordOrigin::TrackInput,
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

    pub fn source(&self) -> WeakColumnSource {
        self.column_source.downgrade()
    }

    fn fill_slot_internal(
        &mut self,
        row: usize,
        mut clip: Clip,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
    ) -> ClipEngineResult<()> {
        let rt_clip = clip.create_real_time_clip(permanent_project, recorder_equipment)?;
        clip.connect_to(&rt_clip);
        get_slot_mut(&mut self.slots, row).clip = Some(clip);
        let args = ColumnFillSlotArgs {
            index: row,
            clip: rt_clip,
        };
        self.rt_command_sender.fill_slot(args);
        Ok(())
    }

    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<(usize, ClipChangedEvent)> {
        // Process source events and generate clip change events
        let mut change_events = vec![];
        while let Ok(evt) = self.event_receiver.try_recv() {
            use ColumnSourceEvent::*;
            let change_event = match evt {
                ClipPlayStateChanged {
                    slot_index,
                    play_state,
                } => {
                    get_clip_mut(&mut self.slots, slot_index).update_play_state(play_state);
                    Some((slot_index, ClipChangedEvent::PlayState(play_state)))
                }
                ClipFrameCountUpdated {
                    slot_index,
                    frame_count,
                } => {
                    get_clip_mut(&mut self.slots, slot_index).update_frame_count(frame_count);
                    None
                }
            };
            if let Some(evt) = change_event {
                change_events.push(evt);
            }
        }
        // Add position updates
        let pos_change_events = self.slots.iter().enumerate().filter_map(|(row, slot)| {
            let clip = slot.clip.as_ref()?;
            if clip.play_state().is_advancing() {
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

    pub fn play_clip(&mut self, args: ColumnPlayClipArgs) {
        self.rt_command_sender.play_clip(args);
    }

    pub fn stop_clip(&mut self, args: crate::rt::ColumnStopClipArgs) {
        self.rt_command_sender.stop_clip(args);
    }

    pub fn set_clip_repeated(&mut self, args: ColumnSetClipRepeatedArgs) {
        self.rt_command_sender.set_clip_repeated(args);
    }

    pub fn pause_clip(&mut self, index: usize) {
        self.rt_command_sender.pause_clip(index);
    }

    pub fn seek_clip(&mut self, index: usize, desired_pos: UnitValue) {
        self.rt_command_sender.seek_clip(index, desired_pos);
    }

    pub fn set_clip_volume(&mut self, index: usize, volume: ReaperVolumeValue) {
        self.rt_command_sender.set_clip_volume(index, volume);
    }

    pub fn toggle_clip_repeated(&mut self, index: usize) -> ClipEngineResult<ClipChangedEvent> {
        let clip = get_slot_mut(&mut self.slots, index)
            .clip
            .as_mut()
            .ok_or("no clip")?;
        let repeated = clip.toggle_looped();
        let args = ColumnSetClipRepeatedArgs { index, repeated };
        self.set_clip_repeated(args);
        Ok(ClipChangedEvent::ClipRepeat(repeated))
    }

    pub fn clip_data(&self, index: usize) -> Option<ClipData> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        let data = ClipData {
            volume: Default::default(),
            repeat: clip.data().looped,
            content: ClipContent::load(&clip.data().source),
        };
        Some(data)
    }

    pub fn clip_info(&self, index: usize) -> Option<ClipInfo> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(clip.info())
    }

    pub fn clip_position_in_seconds(
        &self,
        index: usize,
        timeline_tempo: Bpm,
    ) -> Option<PositionInSeconds> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        clip.position_in_seconds(timeline_tempo)
    }

    pub fn clip_play_state(&self, index: usize) -> Option<ClipPlayState> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(clip.play_state())
    }

    pub fn clip_repeated(&self, index: usize) -> Option<bool> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(clip.data().looped)
    }

    pub fn clip_volume(&self, index: usize) -> Option<ReaperVolumeValue> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(Default::default())
    }

    pub fn proportional_clip_position(&self, row: usize) -> Option<UnitValue> {
        get_slot(&self.slots, row)
            .ok()?
            .clip
            .as_ref()?
            .proportional_pos()
    }

    pub fn record_clip(
        &mut self,
        index: usize,
        behavior: RecordBehavior,
        equipment: RecorderEquipment,
    ) -> ClipEngineResult<ClipRecordTask> {
        self.with_source_mut(|s| s.record_clip(index, behavior, equipment))?;
        let task = ClipRecordTask {
            column_source: self.column_source.clone(),
            slot_index: index,
        };
        Ok(task)
    }

    fn with_source_mut<R>(&mut self, f: impl FnOnce(&mut ColumnSource) -> R) -> R {
        let mut guard = self.column_source.lock();
        f(&mut guard)
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
            preview_register,
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

fn get_slot(slots: &Vec<Slot>, index: usize) -> ClipEngineResult<&Slot> {
    slots.get(index).ok_or("slot doesn't exist")
}

fn get_clip_mut(slots: &mut Vec<Slot>, index: usize) -> &mut Clip {
    get_slot_mut(slots, index)
        .clip
        .as_mut()
        .expect("slot not filled")
}

fn get_slot_mut(slots: &mut Vec<Slot>, index: usize) -> &mut Slot {
    if index >= slots.len() {
        slots.resize_with(index + 1, Default::default);
    }
    slots.get_mut(index).unwrap()
}
