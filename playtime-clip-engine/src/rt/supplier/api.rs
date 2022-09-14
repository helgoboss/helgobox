use crate::conversion_util::convert_duration_in_frames_to_seconds;
use crate::mutex_util::non_blocking_lock;
use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{get_cycle_at_frame, ClipSource, MIDI_BASE_BPM, MIDI_FRAME_RATE};
use crate::rt::tempo_util::calc_tempo_factor;
use crate::ClipEngineResult;
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, MidiFrameOffset, PositionInSeconds,
};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

// TODO-medium We can remove the WithMaterialInfo because we don't box anymore.
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

pub trait PositionTranslationSkill: Debug {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize;
}

pub trait MidiSupplier: Debug {
    /// Writes a portion of MIDI material into the given destination buffer so that it completely
    /// fills that buffer.
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse;

    /// Releases all currently playing notes.
    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    );
}

pub trait WithSource {
    fn source(&self) -> Option<&ClipSource>;
}

pub trait SupplyRequest {
    fn start_frame(&self) -> isize;
    fn info(&self) -> &SupplyRequestInfo;
    fn general_info(&self) -> &SupplyRequestGeneralInfo;
    fn parent_request(&self) -> Option<&Self>;
}

pub trait WithMaterialInfo {
    /// Returns an error if no material available.
    fn material_info(&self) -> ClipEngineResult<MaterialInfo>;
}

/// Contains information about the material.
///
/// "Material" here usually means the inner-most material (the source). However, there's
/// one exception: If a section is defined, that section will change the frame count.
#[derive(Clone, Debug)]
pub enum MaterialInfo {
    Audio(AudioMaterialInfo),
    Midi(MidiMaterialInfo),
}

impl MaterialInfo {
    pub fn is_midi(&self) -> bool {
        matches!(self, MaterialInfo::Midi(_))
    }

    pub fn channel_count(&self) -> usize {
        match self {
            MaterialInfo::Audio(i) => i.channel_count,
            MaterialInfo::Midi(_) => 0,
        }
    }

    pub fn frame_rate(&self) -> Hz {
        match self {
            MaterialInfo::Audio(i) => i.frame_rate,
            MaterialInfo::Midi(_) => MIDI_FRAME_RATE,
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            MaterialInfo::Audio(i) => i.frame_count,
            MaterialInfo::Midi(i) => i.frame_count,
        }
    }

    /// Returns the duration assuming native source tempo.
    pub fn duration(&self) -> DurationInSeconds {
        match self {
            MaterialInfo::Audio(i) => i.duration(),
            MaterialInfo::Midi(i) => i.duration(),
        }
    }

    pub fn get_cycle_at_frame(&self, frame: isize) -> usize {
        get_cycle_at_frame(frame, self.frame_count())
    }

    pub fn tempo_factor_during_recording(&self, timeline_tempo: Bpm) -> f64 {
        if self.is_midi() {
            calc_tempo_factor(MIDI_BASE_BPM, timeline_tempo)
        } else {
            // When recording audio, we have tempo factor 1.0 (original recording tempo).
            1.0
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioMaterialInfo {
    pub channel_count: usize,
    pub frame_count: usize,
    pub frame_rate: Hz,
}

impl AudioMaterialInfo {
    /// Returns the duration assuming native source tempo.
    pub fn duration(&self) -> DurationInSeconds {
        convert_duration_in_frames_to_seconds(self.frame_count, self.frame_rate)
    }
}

#[derive(Clone, Debug)]
pub struct MidiMaterialInfo {
    pub frame_count: usize,
}

impl MidiMaterialInfo {
    /// Returns the duration assuming native source tempo.
    pub fn duration(&self) -> DurationInSeconds {
        convert_duration_in_frames_to_seconds(self.frame_count, MIDI_FRAME_RATE)
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

#[derive(Clone, Eq, PartialEq, Debug)]
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

impl<T: WithMaterialInfo> WithMaterialInfo for Arc<Mutex<T>> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        non_blocking_lock(self, "material info").material_info()
    }
}

impl<T: AudioSupplier> AudioSupplier for Arc<Mutex<T>> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        non_blocking_lock(&*self, "supply audio").supply_audio(request, dest_buffer)
    }
}

impl<T: MidiSupplier> MidiSupplier for Arc<Mutex<T>> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        non_blocking_lock(&*self, "supply MIDI").supply_midi(request, event_list)
    }

    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    ) {
        non_blocking_lock(&*self, "release notes").release_notes(frame_offset, event_list);
    }
}

impl<T: PreBufferSourceSkill> PreBufferSourceSkill for Arc<Mutex<T>> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        non_blocking_lock(&*self, "pre-buffer").pre_buffer(request);
    }
}

impl<T: PositionTranslationSkill> PositionTranslationSkill for Arc<Mutex<T>> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        non_blocking_lock(self, "position translation").translate_play_pos_to_source_pos(play_pos)
    }
}
