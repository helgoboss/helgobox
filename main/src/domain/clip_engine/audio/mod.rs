use reaper_medium::{DurationInSeconds, Hz};

mod source;
pub use source::*;

mod cache;
use crate::domain::clip_engine::buffer::AudioBufMut;
pub use cache::*;

mod looper;
pub use looper::*;

pub mod stretcher;
pub use stretcher::*;

pub trait AudioSupplier {
    /// Writes a portion of audio material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse;

    /// How many channels the supplied audio material consists of.
    fn channel_count(&self) -> usize;

    /// Native (preferred) sample rate of the material.
    fn sample_rate(&self) -> Hz;
}

pub trait ExactSizeAudioSupplier: AudioSupplier {
    /// Total length of the supplied audio material in frames, in relation to the audio supplier's
    /// native sample rate.
    // TODO-high Not every source knows this. Put into separate trait!
    fn frame_count(&self) -> usize;
}

#[derive(Copy, Clone)]
pub struct SupplyAudioRequest {
    /// Position within the most inner material that marks the start of the desired portion.
    ///
    /// It's important to know that we are talking about the position within the most inner audio
    /// supplier (usually the source) because this one provides the continuity that we rely on for
    /// smooth tempo changes etc.
    ///
    /// The frame always relates to the preferred sample rate of the audio supplier, not to
    /// `dest_sample_rate`.
    // TODO-high Change to isize so that we can cope with supply a request that covers both count-in
    //  and start material.
    pub start_frame: usize,
    /// Desired sample rate of the requested material.
    ///
    /// The supplier might employ resampling to fulfill this sample rate demand.
    pub dest_sample_rate: Hz,
}

pub struct SupplyAudioResponse {
    /// The number of frames that were actually written to the destination block.
    ///
    /// Can be less than requested.
    pub num_frames_written: usize,
    /// The next inner frame to be requested in order to ensure smooth, consecutive playback at
    /// all times.
    ///
    /// If `None`, the end has been reached.
    pub next_inner_frame: Option<usize>,
}

pub fn convert_duration_in_seconds_to_frames(seconds: DurationInSeconds, sample_rate: Hz) -> usize {
    (seconds.get() * sample_rate.get()).round() as usize
}

pub fn convert_duration_in_frames_to_seconds(
    frame_count: usize,
    sample_rate: Hz,
) -> DurationInSeconds {
    DurationInSeconds::new(frame_count as f64 / sample_rate.get())
}
