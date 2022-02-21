use crate::{
    CacheRequest, ClipChangedEvent, ClipData, ClipInfo, ClipPlayState, ClipRecordTask,
    ColumnFillSlotArgs, ColumnPauseClipArgs, ColumnPlayClipArgs, ColumnSeekClipArgs,
    ColumnSetClipRepeatedArgs, ColumnSetClipVolumeArgs, ColumnSource, ColumnSourceCommand,
    ColumnSourceCommandSender, ColumnSourceEvent, ColumnStopClipArgs, RecordBehavior, RecordKind,
    RecorderEquipment, RecorderRequest, SharedColumnSource, SharedPos, Slot,
    SlotProcessTransportChangeArgs, Timeline, TimelineMoment, TransportChange,
};
use crossbeam_channel::{Receiver, Sender};
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use reaper_high::{BorrowedSource, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, BorrowedPcmSource, Bpm, CustomPcmSource,
    FlexibleOwnedPcmSource, MeasureAlignment, OwnedPreviewRegister, PositionInSeconds, ReaperMutex,
    ReaperMutexGuard, ReaperVolumeValue,
};
use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    track: Option<Track>,
    column_source: SharedColumnSource,
    preview_register: PlayingPreviewRegister,
    command_sender: ColumnSourceCommandSender,
    slots: Vec<SlotDesc>,
    event_receiver: Receiver<ColumnSourceEvent>,
}

#[derive(Clone, Debug, Default)]
struct SlotDesc {
    clip: Option<ClipDesc>,
}

#[derive(Clone, Debug)]
struct ClipDesc {
    persistent_data: ClipData,
    runtime_data: ClipRuntimeData,
    derived_data: ClipDerivedData,
}

#[derive(Clone, Debug)]
struct ClipRuntimeData {
    play_state: ClipPlayState,
    pos: SharedPos,
}

#[derive(Clone, Debug)]
struct ClipDerivedData {
    frame_count: usize,
}

impl ClipRuntimeData {
    fn pos(&self) -> isize {
        self.pos.get()
    }
}

impl ClipDesc {
    fn proportional_pos(&self) -> Option<UnitValue> {
        let pos = self.runtime_data.pos.get();
        if pos < 0 {
            return None;
        }
        let frame_count = self.derived_data.frame_count;
        if frame_count == 0 {
            return None;
        }
        let mod_pos = pos as usize % self.derived_data.frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Some(proportional)
    }

    fn position_in_seconds(&self, timeline_tempo: Bpm) -> Option<PositionInSeconds> {
        // TODO-high At the moment we don't use this anyway. But we should implement it as soon
        //  as we do. Relies on having the current section length, source frame rate, source tempo.
        todo!()
    }

    fn info(&self) -> ClipInfo {
        // TODO-high This should be implemented as soon as we hold more derived info here.
        //  - type (MIDI, WAVE, ...)
        //  - original length
        todo!()
    }
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
}

impl Column {
    pub fn new(track: Option<Track>) -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(500);
        let (event_sender, event_receiver) = crossbeam_channel::bounded(500);
        let source = ColumnSource::new(
            track.as_ref().map(|t| t.project()),
            command_receiver,
            event_sender,
        );
        let shared_source = SharedColumnSource::new(source);
        Self {
            preview_register: {
                PlayingPreviewRegister::new(shared_source.clone(), track.as_ref())
            },
            track,
            column_source: shared_source,
            command_sender: ColumnSourceCommandSender::new(command_sender),
            slots: vec![],
            event_receiver,
        }
    }

    pub fn source(&self) -> SharedColumnSource {
        self.column_source.clone()
    }

    pub fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        // TODO-high Implement correctly
        let clip_desc = ClipDesc {
            persistent_data: args.clip.persistent_data().unwrap().clone(),
            runtime_data: ClipRuntimeData {
                play_state: Default::default(),
                pos: args.clip.shared_pos(),
            },
            derived_data: ClipDerivedData {
                // TODO-high We need to update things like the frame count or in future derive
                //  it from section data!
                frame_count: args.clip.effective_frame_count(),
            },
        };
        get_slot_mut(&mut self.slots, 0).clip = Some(clip_desc);
        self.command_sender.fill_slot(args);
    }

    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<(usize, ClipChangedEvent)> {
        // Process source events and generate clip change events
        let mut change_events = vec![];
        while let Ok(evt) = self.event_receiver.try_recv() {
            use ColumnSourceEvent::*;
            let change_event = match evt {
                ClipPlayStateChanged { index, play_state } => {
                    get_slot_mut(&mut self.slots, index)
                        .clip
                        .as_mut()
                        .expect("slot not filled")
                        .runtime_data
                        .play_state = play_state;
                    (index, ClipChangedEvent::PlayState(play_state))
                }
            };
            change_events.push(change_event);
        }
        // Add position updates
        let pos_change_events = self.slots.iter().enumerate().filter_map(|(row, slot)| {
            let clip = slot.clip.as_ref()?;
            if clip.runtime_data.play_state.is_advancing() {
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
        self.command_sender.play_clip(args);
    }

    pub fn stop_clip(&mut self, args: ColumnStopClipArgs) {
        self.command_sender.stop_clip(args);
    }

    pub fn set_clip_repeated(&mut self, args: ColumnSetClipRepeatedArgs) {
        self.command_sender.set_clip_repeated(args);
    }

    pub fn pause_clip(&mut self, index: usize) {
        self.command_sender.pause_clip(index);
    }

    pub fn seek_clip(&mut self, index: usize, desired_pos: UnitValue) {
        self.command_sender.seek_clip(index, desired_pos);
    }

    pub fn set_clip_volume(&mut self, index: usize, volume: ReaperVolumeValue) {
        self.command_sender.set_clip_volume(index, volume);
    }

    /// This method should be called whenever REAPER's play state changes. It will make the clip
    /// start/stop synchronized with REAPER's transport.
    pub fn process_transport_change(&mut self, args: SlotProcessTransportChangeArgs) {
        self.command_sender.process_transport_change(args);
    }

    pub fn toggle_clip_repeated(&mut self, index: usize) -> Result<ClipChangedEvent, &'static str> {
        let clip = get_slot_mut(&mut self.slots, index)
            .clip
            .as_mut()
            .ok_or("no clip")?;
        let new_repeated = !clip.persistent_data.repeat;
        clip.persistent_data.repeat = new_repeated;
        let args = ColumnSetClipRepeatedArgs {
            index,
            repeated: new_repeated,
        };
        self.set_clip_repeated(args);
        Ok(ClipChangedEvent::ClipRepeat(new_repeated))
    }

    pub fn clip_data(&self, index: usize) -> Option<ClipData> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(clip.persistent_data.clone())
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
        Some(clip.runtime_data.play_state)
    }

    pub fn clip_repeated(&self, index: usize) -> Option<bool> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(clip.persistent_data.repeat)
    }

    pub fn clip_volume(&self, index: usize) -> Option<ReaperVolumeValue> {
        let clip = get_slot(&self.slots, index).ok()?.clip.as_ref()?;
        Some(clip.persistent_data.volume)
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
    ) -> Result<ClipRecordTask, &'static str> {
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

impl Drop for Column {
    fn drop(&mut self) {
        self.preview_register
            .stop_playing_preview(self.track.as_ref());
    }
}
impl PlayingPreviewRegister {
    pub fn new(source: impl CustomPcmSource + 'static, track: Option<&Track>) -> Self {
        let mut register = OwnedPreviewRegister::default();
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        let (out_chan, preview_track) = if let Some(t) = track {
            (-1, Some(t.raw()))
        } else {
            (0, None)
        };
        register.set_out_chan(out_chan);
        register.set_preview_track(preview_track);
        let source = create_custom_owned_pcm_source(source);
        register.set_src(Some(FlexibleOwnedPcmSource::Custom(source)));
        let preview_register = Arc::new(ReaperMutex::new(register));
        let play_handle = start_playing_preview(&preview_register, track);
        Self {
            preview_register,
            play_handle,
        }
    }

    fn stop_playing_preview(&mut self, track: Option<&Track>) {
        if let Some(track) = track {
            // Check prevents error message on project close.
            let project = track.project();
            if project.is_available() {
                // If not successful this probably means it was stopped already, so okay.
                let _ = Reaper::get()
                    .medium_session()
                    .stop_track_preview_2(project.context(), self.play_handle);
            }
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

fn get_slot(slots: &Vec<SlotDesc>, index: usize) -> Result<&SlotDesc, &'static str> {
    slots.get(index).ok_or("slot doesn't exist")
}

fn get_slot_mut(slots: &mut Vec<SlotDesc>, index: usize) -> &mut SlotDesc {
    if index >= slots.len() {
        slots.resize_with(index + 1, Default::default);
    }
    slots.get_mut(index).unwrap()
}
