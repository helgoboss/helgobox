use crate::{
    CacheRequest, ClipChangedEvent, ClipRecordTask, ColumnFillSlotArgs, ColumnPlayClipArgs,
    ColumnPollSlotArgs, ColumnSetClipRepeatedArgs, ColumnSource, ColumnStopClipArgs,
    RecordBehavior, RecordKind, RecorderRequest, SharedColumnSource, Slot,
    SlotProcessTransportChangeArgs, Timeline, TimelineMoment, TransportChange,
};
use crossbeam_channel::Sender;
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use reaper_high::{BorrowedSource, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, BorrowedPcmSource, CustomPcmSource, FlexibleOwnedPcmSource,
    MeasureAlignment, OwnedPreviewRegister, PositionInSeconds, ReaperMutex, ReaperMutexGuard,
    ReaperVolumeValue,
};
use std::ptr::NonNull;
use std::sync::Arc;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    track: Option<Track>,
    column_source: SharedColumnSource,
    preview_register: PlayingPreviewRegister,
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
}

impl Column {
    pub fn new(track: Option<Track>) -> Self {
        let source = ColumnSource::new(track.as_ref().map(|t| t.project()));
        let shared_source = SharedColumnSource::new(source);
        Self {
            preview_register: {
                PlayingPreviewRegister::new(shared_source.clone(), track.as_ref())
            },
            track,
            column_source: shared_source,
        }
    }

    pub fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        self.with_source_mut(|s| s.fill_slot(args));
    }

    pub fn poll_slot(&mut self, args: ColumnPollSlotArgs) -> Option<ClipChangedEvent> {
        self.with_source_mut(|s| s.poll_slot(args))
    }

    pub fn with_slot<R>(
        &self,
        index: usize,
        f: impl FnOnce(&Slot) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        self.with_source(|s| s.with_slot(index, f))
    }

    pub fn play_clip(&mut self, args: ColumnPlayClipArgs) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.play_clip(args))
    }

    pub fn stop_clip(&mut self, args: ColumnStopClipArgs) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.stop_clip(args))
    }

    pub fn set_clip_repeated(
        &mut self,
        args: ColumnSetClipRepeatedArgs,
    ) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.set_clip_repeated(args))
    }

    pub fn toggle_clip_repeated(&mut self, index: usize) -> Result<ClipChangedEvent, &'static str> {
        self.with_source_mut(|s| s.toggle_clip_repeated(index))
    }

    pub fn record_clip(
        &mut self,
        index: usize,
        behavior: RecordBehavior,
        recorder_request_sender: Sender<RecorderRequest>,
        cache_request_sender: Sender<CacheRequest>,
    ) -> Result<ClipRecordTask, &'static str> {
        self.with_source_mut(|s| {
            s.record_clip(
                index,
                behavior,
                recorder_request_sender,
                cache_request_sender,
            )
        })?;
        let task = ClipRecordTask {
            column_source: self.column_source.clone(),
            slot_index: index,
        };
        Ok(task)
    }

    pub fn pause_clip(&mut self, index: usize) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.pause_clip(index))
    }

    pub fn seek_clip(&mut self, index: usize, desired_pos: UnitValue) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.seek_clip(index, desired_pos))
    }

    pub fn set_clip_volume(
        &mut self,
        index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<ClipChangedEvent, &'static str> {
        self.with_source_mut(|s| s.set_clip_volume(index, volume))
    }

    /// This method should be called whenever REAPER's play state changes. It will make the clip
    /// start/stop synchronized with REAPER's transport.
    pub fn process_transport_change(&mut self, args: &SlotProcessTransportChangeArgs) {
        self.with_source_mut(|s| s.process_transport_change(args));
    }

    fn with_source<R>(&self, f: impl FnOnce(&ColumnSource) -> R) -> R {
        let guard = self.column_source.lock();
        f(&guard)
    }

    fn with_source_mut<R>(&mut self, f: impl FnOnce(&mut ColumnSource) -> R) -> R {
        let mut guard = self.column_source.lock();
        f(&mut guard)
    }
}

fn lock(reg: &SharedRegister) -> ReaperMutexGuard<OwnedPreviewRegister> {
    reg.lock().expect("couldn't acquire lock")
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
