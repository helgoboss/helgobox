use crate::domain::clip_engine::audio::stretcher::resampling::Resampler;
use crate::domain::clip_engine::audio::stretcher::time_stretching::SeriousTimeStretcher;
use crate::domain::clip_engine::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip_engine::buffer::{AudioBufMut, OwnedAudioBuffer};
use core::cmp;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer};

mod resampling;
pub use resampling::*;
pub mod time_stretching;
pub use time_stretching::*;

pub struct AudioStretcher<S: AudioSupplier> {
    enabled: bool,
    supplier: S,
    tempo_factor: f64,
    mode: StretchMode,
}

#[derive(Debug)]
pub enum StretchMode {
    /// Changes time but also pitch.
    Resampling(Resampler),
    /// Uses serious time stretching, without influencing pitch.
    Serious(SeriousTimeStretcher),
}

impl<S: AudioSupplier> AudioStretcher<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            enabled: false,
            supplier,
            tempo_factor: 1.0,
            mode: StretchMode::Resampling(Resampler),
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.tempo_factor = tempo_factor;
    }

    pub fn set_mode(&mut self, mode: StretchMode) {
        self.mode = mode;
    }

    fn ctx<'a, T>(&'a self, mode: &'a T) -> Ctx<'a, T, S> {
        Ctx {
            supplier: &self.supplier,
            mode,
            tempo_factor: self.tempo_factor,
        }
    }
}

impl<S: AudioSupplier> AudioSupplier for AudioStretcher<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        if !self.enabled {
            return self.supplier.supply_audio(&request, dest_buffer);
        }
        use StretchMode::*;
        match &self.mode {
            Resampling(m) => self.ctx(m).supply_audio(request, dest_buffer),
            Serious(m) => self.ctx(m).supply_audio(request, dest_buffer),
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }

    fn sample_rate(&self) -> Hz {
        use StretchMode::*;
        match &self.mode {
            Resampling(m) => self.ctx(m).sample_rate(),
            Serious(m) => self.ctx(m).sample_rate(),
        }
    }
}

pub struct Ctx<'a, M, S: AudioSupplier> {
    supplier: &'a S,
    mode: &'a M,
    tempo_factor: f64,
}
