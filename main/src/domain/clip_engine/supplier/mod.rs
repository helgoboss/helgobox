use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz, PositionInSeconds};

mod source;
pub use source::*;

mod cache;
use crate::domain::clip_engine::buffer::AudioBufMut;
pub use cache::*;

mod looper;
pub use looper::*;

pub mod stretcher;
pub use stretcher::*;

mod chain;
pub use chain::*;

mod suspender;
pub use suspender::*;

mod midi_util;

pub trait AudioSupplier {
    /// Writes a portion of audio material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse;

    /// How many channels the supplied audio material consists of.
    fn channel_count(&self) -> usize;

    /// Native (preferred) sample rate of the material.
    fn sample_rate(&self) -> Hz;
}

pub trait MidiSupplier {
    /// Writes a portion of MIDI material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_midi(
        &self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse;
}

pub trait ExactFrameCount {
    /// Total length of the supplied audio material in frames, in relation to the audio supplier's
    /// native sample rate.
    fn frame_count(&self) -> usize;
}

#[derive(Copy, Clone, Debug)]
pub struct SupplyAudioRequest {
    /// Position within the most inner material that marks the start of the desired portion.
    ///
    /// It's important to know that we are talking about the position within the most inner audio
    /// supplier (usually the source) because this one provides the continuity that we rely on for
    /// smooth tempo changes etc.
    ///
    /// The frame always relates to the preferred sample rate of the audio supplier, not to
    /// `dest_sample_rate`.
    pub start_frame: isize,
    /// Desired sample rate of the requested material.
    ///
    /// The supplier might employ resampling to fulfill this sample rate demand.
    pub dest_sample_rate: Hz,
}

#[derive(Copy, Clone, Debug)]
pub struct SupplyMidiRequest {
    /// Position within the most inner material that marks the start of the desired portion.
    ///
    /// A MIDI frame, that is 1/1024000 of a second.
    pub start_frame: isize,
    /// Number of requested frames.
    pub dest_frame_count: usize,
    /// Device sample rate.
    pub dest_sample_rate: Hz,
}

pub struct SupplyResponse {
    /// The number of frames that were actually written to the destination block.
    ///
    /// Can be less than requested to indicate that the end of the source has been reached.
    pub num_frames_written: usize,
    /// The next inner frame to be requested in order to ensure smooth, consecutive playback at
    /// all times.
    ///
    /// If `None`, the end has been reached.
    pub next_inner_frame: Option<isize>,
}

pub fn convert_duration_in_seconds_to_frames(seconds: DurationInSeconds, sample_rate: Hz) -> usize {
    (seconds.get() * sample_rate.get()).round() as usize
}

pub fn convert_duration_in_seconds_to_midi_frames(seconds: DurationInSeconds) -> usize {
    (seconds.get() * MIDI_FRAME_RATE).round() as usize
}

pub fn convert_position_in_seconds_to_frames(seconds: PositionInSeconds, sample_rate: Hz) -> isize {
    (seconds.get() * sample_rate.get()).round() as isize
}

pub fn convert_duration_in_frames_to_seconds(
    frame_count: usize,
    sample_rate: Hz,
) -> DurationInSeconds {
    DurationInSeconds::new(frame_count as f64 / sample_rate.get())
}

pub fn convert_position_in_frames_to_seconds(
    frame_count: isize,
    sample_rate: Hz,
) -> PositionInSeconds {
    PositionInSeconds::new(frame_count as f64 / sample_rate.get())
}

pub fn convert_position_in_midi_frames_to_seconds(frame_count: isize) -> PositionInSeconds {
    PositionInSeconds::new(frame_count as f64 / MIDI_FRAME_RATE)
}

/// We could use just any unit to represent a position within a MIDI source, but we choose frames
/// with regard to the following frame rate. Choosing frames allows us to treat MIDI similar to
/// audio, which results in fewer special cases. The frame rate of 1,024,000 is also the unit which
/// is used in REAPER's MIDI events, so this corresponds nicely to the audio world where one sample
/// frame is the smallest possible unit.
pub const MIDI_FRAME_RATE: f64 = 1_024_000.0;

/// MIDI data is tempo-less. But pretending that all MIDI clips have a fixed tempo allows us to
/// treat MIDI similar to audio. E.g. if we want it to play faster, we just lower the output sample
/// rate. Plus, we can use the same time stretching supplier. Fewer special cases, nice!
// TODO-high Change to 120 later when we detect audio tempos correctly.
pub const MIDI_BASE_BPM: f64 = 96.0;

/// Helper function for suppliers that read from sources and don't want to deal with
/// negative start frames themselves.
fn supply_source_material(
    request: &SupplyAudioRequest,
    dest_buffer: &mut AudioBufMut,
    source_sample_rate: Hz,
    supply_inner: impl FnOnce(SourceMaterialRequest) -> SupplyResponse,
) -> SupplyResponse {
    // The lower the destination sample rate in relation to the source sample rate, the
    // higher the tempo.
    let tempo_factor = source_sample_rate.get() / request.dest_sample_rate.get();
    // The higher the tempo, the more inner source material we should grab.
    let ideal_num_consumed_frames =
        (dest_buffer.frame_count() as f64 * tempo_factor).round() as usize;
    let ideal_end_frame = request.start_frame + ideal_num_consumed_frames as isize;
    if ideal_end_frame <= 0 {
        // Requested portion is located entirely before the actual source material.
        // println!(
        //     "ideal end frame {} ({})",
        //     ideal_end_frame, ideal_num_consumed_frames
        // );
        SupplyResponse {
            // We haven't reached the end of the source, so still tell the caller that we
            // wrote all frames.
            num_frames_written: dest_buffer.frame_count(),
            // And advance the count-in phase.
            next_inner_frame: Some(ideal_end_frame),
        }
    } else {
        // Requested portion overlaps with playable material.
        if request.start_frame < 0 {
            println!(
                "overlap: start_frame = {}, ideal_end_frame = {}",
                request.start_frame, ideal_end_frame
            );
            // Left part of the portion is located before and right part after start of material.
            let num_skipped_frames_in_source = -request.start_frame as usize;
            let proportion_skipped =
                num_skipped_frames_in_source as f64 / ideal_num_consumed_frames as f64;
            let num_skipped_frames_in_dest =
                (proportion_skipped * dest_buffer.frame_count() as f64).round() as usize;
            let mut shifted_dest_buffer = dest_buffer.slice_mut(num_skipped_frames_in_dest..);
            let req = SourceMaterialRequest {
                start_frame: 0,
                dest_buffer: &mut shifted_dest_buffer,
                source_sample_rate,
                dest_sample_rate: request.dest_sample_rate,
            };
            // println!(
            //     "Before source: start = {}, source sr = {}, dest sr = {}",
            //     req.start_frame, req.source_sample_rate, req.dest_sample_rate
            // );
            let res = supply_inner(req);
            SupplyResponse {
                num_frames_written: num_skipped_frames_in_dest + res.num_frames_written,
                next_inner_frame: res.next_inner_frame,
            }
        } else {
            // Requested portion is located on or after start of the actual source material.
            let req = SourceMaterialRequest {
                start_frame: request.start_frame as usize,
                dest_buffer,
                source_sample_rate,
                dest_sample_rate: request.dest_sample_rate,
            };
            // println!(
            //     "In source: start = {}, source sr = {}, dest sr = {}",
            //     req.start_frame, req.source_sample_rate, req.dest_sample_rate
            // );
            supply_inner(req)
        }
    }
}

struct SourceMaterialRequest<'a, 'b> {
    start_frame: usize,
    dest_buffer: &'a mut AudioBufMut<'b>,
    source_sample_rate: Hz,
    dest_sample_rate: Hz,
}
