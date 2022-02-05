use reaper_medium::{BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, PositionInSeconds};
use std::cmp;

mod reaper_source;
pub use reaper_source::*;

mod cache;
use crate::buffer::AudioBufMut;
pub use cache::*;

mod looper;
pub use looper::*;

mod flexible_source;
pub use flexible_source::*;

pub mod time_stretcher;
pub use time_stretcher::*;

pub mod resampler;
pub use resampler::*;

use crate::Timeline;

mod chain;
pub use chain::*;

mod suspender;
use crate::clip_timeline;
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
    /// Total length of the supplied audio material in frames, in relation to the supplier's
    /// native sample rate.
    fn frame_count(&self) -> usize;
}

pub trait WithTempo {
    /// Native tempo if applicable.
    fn tempo(&self) -> Option<Bpm>;
}

pub trait WithFrameRate {
    /// Native (preferred) sample rate of the material.
    fn frame_rate(&self) -> Hz;
}

pub trait ExactDuration {
    fn duration(&self) -> DurationInSeconds;
}

pub trait SupplyRequest {
    fn start_frame(&self) -> isize;
    fn info(&self) -> &SupplyRequestInfo;
    fn general_info(&self) -> &SupplyRequestGeneralInfo;
    fn parent_request(&self) -> Option<&Self>;
}

#[derive(Clone, Debug)]
pub struct SupplyAudioRequest<'a> {
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
    /// Just for analysis and debugging purposes.
    pub info: SupplyRequestInfo,
    pub parent_request: Option<&'a SupplyAudioRequest<'a>>,
    pub general_info: &'a SupplyRequestGeneralInfo,
}

impl<'a> SupplyRequest for SupplyAudioRequest<'a> {
    fn start_frame(&self) -> isize {
        self.start_frame
    }

    fn info(&self) -> &SupplyRequestInfo {
        &self.info
    }

    fn general_info(&self) -> &SupplyRequestGeneralInfo {
        self.general_info
    }

    fn parent_request(&self) -> Option<&Self> {
        self.parent_request
    }
}

#[derive(Clone, Debug, Default)]
pub struct SupplyRequestGeneralInfo {
    /// Timeline cursor position of the start of the currently requested audio block.
    pub audio_block_timeline_cursor_pos: PositionInSeconds,
    /// Audio block length in frames.
    pub audio_block_length: usize,
    /// The device frame rate.
    pub output_frame_rate: Hz,
    /// Current tempo on the timeline.
    pub timeline_tempo: Bpm,
    /// Current tempo factor for the clip.
    pub clip_tempo_factor: f64,
}

/// Only for analysis and debugging, shouldn't influence behavior.
#[derive(Clone, Debug, Default)]
pub struct SupplyRequestInfo {
    /// Frame offset within the currently requested audio block
    ///
    /// At the top of the chain, there's one request per audio block, so this number is usually 0.
    ///
    /// Some suppliers divide this top request into smaller ones (e.g. the looper when
    /// it reaches the start or end of the source within one block). In that case, this number will
    /// be greater than 0 for the second request. The number should be accumulated if multiple
    /// nested suppliers divide requests.
    pub audio_block_frame_offset: usize,
    /// A little label identifying which supplier and which sub request in the chain
    /// produced/modified this request.
    pub requester: &'static str,
    /// An optional note.
    pub note: &'static str,
}

#[derive(Clone, Debug)]
pub struct SupplyMidiRequest<'a> {
    /// Position within the most inner material that marks the start of the desired portion.
    ///
    /// A MIDI frame, that is 1/1024000 of a second.
    pub start_frame: isize,
    /// Number of requested frames.
    pub dest_frame_count: usize,
    /// Device sample rate.
    pub dest_sample_rate: Hz,
    /// Just for analysis and debugging purposes.
    pub info: SupplyRequestInfo,
    pub parent_request: Option<&'a SupplyMidiRequest<'a>>,
    pub general_info: &'a SupplyRequestGeneralInfo,
}

impl<'a> SupplyRequest for SupplyMidiRequest<'a> {
    fn start_frame(&self) -> isize {
        self.start_frame
    }

    fn info(&self) -> &SupplyRequestInfo {
        &self.info
    }

    fn general_info(&self) -> &SupplyRequestGeneralInfo {
        self.general_info
    }

    fn parent_request(&self) -> Option<&Self> {
        self.parent_request
    }
}

pub struct SupplyResponse {
    /// The number of frames that were actually written to the destination block.
    ///
    /// Can be less than requested if the end of the source has been reached.
    pub num_frames_written: usize,
    /// The number of frames that were actually consumed from the source.
    ///
    /// Can be less than requested if the end of the source has been reached.
    /// If the start of the source has not been reached yet, still fill it with the ideal
    /// amount of consumed frames.
    pub num_frames_consumed: usize,
    /// The next inner frame to be requested in order to ensure smooth, consecutive playback at
    /// all times.
    ///
    /// If `None`, the end has been reached.
    ///
    /// In many cases, this is just the requested start frame + `num_frames_consumed`. But
    /// suppliers have the freedom to return other values, e.g. start over from the beginning.
    pub next_inner_frame: Option<isize>,
}

pub fn convert_duration_in_seconds_to_frames(seconds: DurationInSeconds, sample_rate: Hz) -> usize {
    (seconds.get() * sample_rate.get()).round() as usize
}

pub fn convert_position_in_seconds_to_frames(seconds: PositionInSeconds, sample_rate: Hz) -> isize {
    (seconds.get() * sample_rate.get()).round() as isize
}

pub fn adjust_proportionally_positive(frame_count: f64, factor: f64) -> usize {
    adjust_proportionally(frame_count, factor) as usize
}

pub fn adjust_anti_proportionally_positive(frame_count: f64, factor: f64) -> usize {
    adjust_anti_proportionally(frame_count, factor) as usize
}

pub fn adjust_proportionally(frame_count: f64, factor: f64) -> isize {
    (frame_count as f64 * factor).round() as isize
}

pub fn adjust_anti_proportionally(frame_count: f64, factor: f64) -> isize {
    (frame_count as f64 / factor).round() as isize
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

pub fn convert_duration_in_frames_to_other_frame_rate(
    frame_count: usize,
    in_sample_rate: Hz,
    out_sample_rate: Hz,
) -> usize {
    let ratio = out_sample_rate.get() / in_sample_rate.get();
    (ratio * frame_count as f64).round() as usize
}

/// MIDI data is tempo-less. But pretending that all MIDI clips have a fixed tempo allows us to
/// treat MIDI similar to audio. E.g. if we want it to play faster, we just lower the output sample
/// rate. Plus, we can use the same time stretching supplier. Fewer special cases, nice!
pub const MIDI_BASE_BPM: f64 = 120.0;

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
        adjust_proportionally_positive(dest_buffer.frame_count() as f64, tempo_factor);
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
            num_frames_consumed: ideal_num_consumed_frames,
            // And advance the count-in phase.
            next_inner_frame: Some(ideal_end_frame),
        }
    } else {
        // Requested portion overlaps with playable material.
        if request.start_frame < 0 {
            // println!(
            //     "overlap: start_frame = {}, ideal_end_frame = {}",
            //     request.start_frame, ideal_end_frame
            // );
            // Left part of the portion is located before and right part after start of material.
            let num_skipped_frames_in_source = -request.start_frame as usize;
            let proportion_skipped =
                num_skipped_frames_in_source as f64 / ideal_num_consumed_frames as f64;
            let num_skipped_frames_in_dest = adjust_proportionally_positive(
                dest_buffer.frame_count() as f64,
                proportion_skipped,
            );
            print_distance_from_beat_start_at(
                request,
                num_skipped_frames_in_dest,
                "audio, start_frame < 0",
            );
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
                num_frames_consumed: num_skipped_frames_in_source + res.num_frames_consumed,
                next_inner_frame: res.next_inner_frame,
            }
        } else {
            // Requested portion is located on or after start of the actual source material.
            if request.start_frame == 0 {
                print_distance_from_beat_start_at(request, 0, "audio, start_frame == 0");
            }
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

/// This deals with timeline units only.
fn print_distance_from_beat_start_at(
    request: &impl SupplyRequest,
    additional_block_offset: usize,
    comment: &str,
) {
    let effective_block_offset = request.info().audio_block_frame_offset + additional_block_offset;
    let offset_in_timeline_secs = convert_duration_in_frames_to_seconds(
        effective_block_offset,
        request.general_info().output_frame_rate,
    );
    let ref_pos = request.general_info().audio_block_timeline_cursor_pos + offset_in_timeline_secs;
    let timeline = clip_timeline(None, false);
    let next_bar = timeline.next_bar_at(ref_pos);
    struct BarInfo {
        bar: i32,
        pos: PositionInSeconds,
        rel_pos: PositionInSeconds,
    }
    let create_bar_info = |bar| {
        let bar_pos = timeline.pos_of_bar(bar);
        BarInfo {
            bar,
            pos: bar_pos,
            rel_pos: ref_pos - bar_pos,
        }
    };
    let current_bar_info = create_bar_info(next_bar - 1);
    let next_bar_info = create_bar_info(next_bar);
    let closest = cmp::min_by_key(&current_bar_info, &next_bar_info, |v| v.rel_pos.abs());
    let rel_pos_from_closest_bar_in_timeline_frames = convert_position_in_seconds_to_frames(
        closest.rel_pos,
        request.general_info().output_frame_rate,
    );
    let block_duration = convert_duration_in_frames_to_seconds(
        request.general_info().audio_block_length,
        request.general_info().output_frame_rate,
    );
    let block_index = (request.general_info().audio_block_timeline_cursor_pos.get()
        / block_duration.get()) as isize;
    println!(
        "\n\
        # New loop cycle\n\
        Block index: {}\n\
        Block start position: {:.3}s\n\
        Closest bar: {}\n\
        Closest bar timeline position: {:.3}s\n\
        Relative position from closest bar: {:.3}ms (= {} timeline frames)\n\
        Effective block offset: {},\n\
        Requester: {}\n\
        Note: {}\n\
        Comment: {}\n\
        Clip tempo factor: {}\n\
        Timeline tempo: {}\n\
        Parent requester: {:?}\n\
        Parent note: {:?}\n\
        ",
        block_index,
        request.general_info().audio_block_timeline_cursor_pos,
        closest.bar,
        closest.pos.get(),
        closest.rel_pos.get() * 1000.0,
        rel_pos_from_closest_bar_in_timeline_frames,
        effective_block_offset,
        request.info().requester,
        request.info().note,
        comment,
        request.general_info().clip_tempo_factor,
        request.general_info().timeline_tempo,
        request.parent_request().map(|r| r.info().requester),
        request.parent_request().map(|r| r.info().note)
    );
}
