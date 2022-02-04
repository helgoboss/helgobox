use crate::{
    ClipChangedEvent, ColumnFillSlotArgs, ColumnPlaySlotArgs, ColumnSource, ColumnSourceSkills,
    ColumnStopSlotArgs, LegacyClip, SharedRegister, StretchWorkerRequest, Timeline,
};
use crossbeam_channel::Sender;
use enumflags2::BitFlags;
use reaper_high::{BorrowedSource, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, BorrowedPcmSource, CustomPcmSource, FlexibleOwnedPcmSource,
    MeasureAlignment, OwnedPreviewRegister, ReaperMutex, ReaperMutexGuard, ReaperVolumeValue,
};
use std::ptr::NonNull;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Column {
    track: Option<Track>,
    audio_preview_register: PlayingPreviewRegister,
    // midi_preview_register: PlayingPreviewRegister,
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
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

impl Column {
    pub fn new(track: Option<Track>) -> Self {
        Self {
            audio_preview_register: {
                let source = ColumnSource::new(track.as_ref().map(|t| t.project()));
                PlayingPreviewRegister::new(source, track.as_ref())
            },
            // midi_preview_register: {
            //     let source = ColumnSource::new(track.as_ref().map(|t| t.project()));
            //     PlayingPreviewRegister::new(source, track.as_ref())
            // },
            track,
        }
    }

    pub fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        self.with_source_mut(|s| s.fill_slot(args));
    }

    pub fn play_slot(&mut self, args: ColumnPlaySlotArgs) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.play_slot(args))
    }

    pub fn stop_slot(&mut self, args: ColumnStopSlotArgs) -> Result<(), &'static str> {
        self.with_source_mut(|s| s.stop_slot(args))
    }

    fn with_source_mut<R>(&mut self, f: impl FnOnce(&mut BorrowedPcmSource) -> R) -> R {
        let mut guard = lock(&self.audio_preview_register.preview_register);
        let src = guard.src_mut().expect("column source must be set");
        f(src.as_mut())
    }
}

fn lock(reg: &SharedRegister) -> ReaperMutexGuard<OwnedPreviewRegister> {
    reg.lock().expect("couldn't acquire lock")
}

impl Drop for Column {
    fn drop(&mut self) {
        // self.midi_preview_register
        //     .stop_playing_preview(self.track.as_ref());
        self.audio_preview_register
            .stop_playing_preview(self.track.as_ref());
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
