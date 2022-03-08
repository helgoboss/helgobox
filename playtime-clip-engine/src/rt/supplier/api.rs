use crate::conversion_util::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
};
use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{MIDI_BASE_BPM, MIDI_FRAME_RATE};
use crate::ClipEngineResult;
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInBeats, DurationInSeconds, Hz, OwnedPcmSource,
    PositionInSeconds,
};
use std::fmt::Debug;

pub trait AudioSupplier: Debug + WithMaterialInfo {
    /// Writes a portion of audio material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse;
}

pub trait PreBufferSourceSkill: Debug {
    /// Does its best to make sure that the next source block is pre-buffered with the given
    /// criteria.
    ///
    /// It must be asynchronous and cheap enough to call from a real-time thread.
    fn pre_buffer(&mut self, request: PreBufferFillRequest);
}

pub trait MidiSupplier: Debug {
    /// Writes a portion of MIDI material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse;
}

pub trait WithSource {
    fn source(&self) -> &OwnedPcmSource;

    fn source_mut(&mut self) -> &mut OwnedPcmSource;
}

pub trait SupplyRequest {
    fn start_frame(&self) -> isize;
    fn info(&self) -> &SupplyRequestInfo;
    fn general_info(&self) -> &SupplyRequestGeneralInfo;
    fn parent_request(&self) -> Option<&Self>;
}

pub trait WithMaterialInfo {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo>;
}

#[derive(Debug)]
pub enum MaterialInfo {
    Audio(AudioMaterialInfo),
    Midi(MidiMaterialInfo),
}

impl MaterialInfo {
    pub fn channel_count(&self) -> usize {
        match self {
            MaterialInfo::Audio(i) => i.channel_count,
            MaterialInfo::Midi(_) => 0,
        }
    }

    pub fn frame_rate(&self) -> Hz {
        match self {
            MaterialInfo::Audio(i) => i.sample_rate,
            MaterialInfo::Midi(i) => i.frame_rate(),
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            MaterialInfo::Audio(i) => i.frame_count(),
            MaterialInfo::Midi(i) => i.frame_count(),
        }
    }

    pub fn duration(&self) -> DurationInSeconds {
        match self {
            MaterialInfo::Audio(i) => i.duration(),
            MaterialInfo::Midi(i) => i.duration(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioMaterialInfo {
    pub channel_count: usize,
    // pub length: DurationInSeconds,
    pub length: usize,
    pub sample_rate: Hz,
}

impl AudioMaterialInfo {
    pub fn frame_count(&self) -> usize {
        // convert_duration_in_seconds_to_frames(self.length, self.sample_rate)
        self.length
    }

    pub fn duration(&self) -> DurationInSeconds {
        convert_duration_in_frames_to_seconds(self.frame_count(), self.sample_rate)
    }
}

#[derive(Debug)]
pub struct MidiMaterialInfo {
    // pub length: DurationInBeats,
    pub length: usize,
}

impl MidiMaterialInfo {
    // pub fn length_in_seconds(&self) -> DurationInSeconds {
    //     // For MIDI, get_length() takes the current project tempo in account ... which is not
    //     // what we want because we want to do all the tempo calculations ourselves and treat
    //     // MIDI/audio the same wherever possible.
    //     let beats_per_minute = MIDI_BASE_BPM;
    //     let beats_per_second = beats_per_minute / 60.0;
    //     DurationInSeconds::new(self.length.get() / beats_per_second)
    // }

    pub fn frame_count(&self) -> usize {
        // convert_duration_in_seconds_to_frames(self.length_in_seconds(), Hz::new(MIDI_FRAME_RATE))
        self.length
    }

    pub fn duration(&self) -> DurationInSeconds {
        convert_duration_in_frames_to_seconds(self.frame_count(), self.frame_rate())
    }

    // TODO-high Check that this is not used in situations where we know it's MIDI already. Because
    //  then we can just use MIDI_FRAME_RATE directly. Also introduce a constant Hz (unchecked)!
    pub fn frame_rate(&self) -> Hz {
        Hz::new(MIDI_FRAME_RATE)
    }
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
    /// Desired sample rate of the requested material (uses material's native sample rate if not
    /// set).
    ///
    /// The supplier might employ resampling to fulfill this sample rate demand.
    pub dest_sample_rate: Option<Hz>,
    /// Just for analysis and debugging purposes.
    pub info: SupplyRequestInfo,
    pub parent_request: Option<&'a SupplyAudioRequest<'a>>,
    pub general_info: &'a SupplyRequestGeneralInfo,
}

impl<'a> SupplyAudioRequest<'a> {
    /// Can be used by code that's built upon the assumption that in/out frame rates equal and
    /// therefore number of consumed frames == number of written frames.
    ///
    /// In our supplier chain, this assumption is for most suppliers true because we don't let the
    /// PCM source or our buffers do the resampling itself. The higher-level resample supplier
    /// takes care of that.
    pub fn assert_wants_source_frame_rate(&self, source_frame_rate: Hz) {
        if let Some(dest_sample_rate) = self.dest_sample_rate {
            assert_eq!(dest_sample_rate, source_frame_rate);
        }
    }
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
    pub is_realtime: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct PreBufferFillRequest {
    pub start_frame: isize,
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

#[derive(Copy, Clone, Debug, Default)]
pub struct SupplyResponse {
    /// The number of frames that were actually consumed from the source.
    ///
    /// Can be less than requested if the end of the source has been reached.
    /// If the start of the source has not been reached yet, still fill it with the ideal
    /// amount of consumed frames.
    pub num_frames_consumed: usize,
    pub status: SupplyResponseStatus,
}

#[derive(Copy, Clone, Debug)]
pub enum SupplyResponseStatus {
    PleaseContinue,
    ReachedEnd {
        /// The number of frames that were actually written to the destination block.
        num_frames_written: usize,
    },
}

impl Default for SupplyResponseStatus {
    fn default() -> Self {
        Self::ReachedEnd {
            num_frames_written: 0,
        }
    }
}

impl SupplyResponseStatus {
    pub fn reached_end(&self) -> bool {
        matches!(self, SupplyResponseStatus::ReachedEnd { .. })
    }
}

impl SupplyResponse {
    pub fn reached_end(num_frames_consumed: usize, num_frames_written: usize) -> Self {
        Self {
            num_frames_consumed,
            status: SupplyResponseStatus::ReachedEnd { num_frames_written },
        }
    }

    pub fn exceeded_end() -> Self {
        Self::reached_end(0, 0)
    }

    pub fn please_continue(num_frames_consumed: usize) -> Self {
        Self {
            num_frames_consumed,
            status: SupplyResponseStatus::PleaseContinue,
        }
    }

    pub fn limited_by_total_frame_count(
        num_frames_consumed: usize,
        num_frames_written: usize,
        start_frame: isize,
        total_frame_count: usize,
    ) -> Self {
        let next_frame = start_frame + num_frames_consumed as isize;
        Self {
            num_frames_consumed,
            status: if next_frame < total_frame_count as isize {
                SupplyResponseStatus::PleaseContinue
            } else {
                SupplyResponseStatus::ReachedEnd { num_frames_written }
            },
        }
    }
}
