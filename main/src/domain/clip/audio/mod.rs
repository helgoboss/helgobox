use reaper_medium::{DurationInSeconds, Hz};

mod source;
pub use source::*;

mod cache;
use crate::domain::clip::buffer::AudioBufMut;
pub use cache::*;

pub trait AudioSupplier {
    /// Writes a portion of audio material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_audio(&self, request: &SupplyRequest, dest_buffer: AudioBufMut) -> SupplyResponse;

    /// How many channels the supplied audio material consists of.
    fn channel_count(&self) -> usize;

    /// Total length of the supplied audio material in frames, in relation to the audio supplier's
    /// native sample rate.
    fn frame_count(&self) -> usize;

    /// Native (preferred) sample rate of the material.
    fn sample_rate(&self) -> Hz;
}

pub struct SupplyRequest {
    /// Position within the requested material that marks the start of the desired portion.
    ///
    /// The frame always relates to the preferred sample rate of the audio supplier, not to
    /// `dest_sample_rate`.
    pub start_frame: usize,
    /// Desired sample rate of the requested material.
    ///
    /// The supplier might employ resampling to fulfill this sample rate demand.
    pub dest_sample_rate: Hz,
}

pub struct SupplyResponse {
    /// The number of frames that were actually written.
    pub num_frames_written: usize,
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
