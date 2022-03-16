use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_other_frame_rate,
    convert_duration_in_frames_to_seconds,
};
use crate::file_util::get_path_for_new_media_file;
use crate::rt::buffer::{AudioBuf, AudioBufMut, OwnedAudioBuffer};
use crate::rt::schedule_util::{calc_distance_from_pos, calc_distance_from_quantized_pos};
use crate::rt::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiMaterialInfo, MidiSupplier,
    PositionTranslationSkill, SectionBounds, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
    WithMaterialInfo, WithSource, MIDI_BASE_BPM, MIDI_FRAME_RATE,
};
use crate::rt::{
    BasicAudioRequestProps, ColumnSettings, OverridableMatrixSettings, QuantizedPosCalcEquipment,
};
use crate::timeline::{clip_timeline, Timeline};
use crate::{ClipEngineResult, HybridTimeline, QuantizedPosition};
use crossbeam_channel::{Receiver, Sender};
use helgoboss_midi::Channel;
use playtime_api::{
    ClipPlayStartTiming, ClipRecordStartTiming, ClipRecordStopTiming, EvenQuantization,
    MatrixClipRecordSettings, RecordLength,
};
use reaper_high::{OwnedSource, Project, Reaper};
use reaper_low::raw::{midi_realtime_write_struct_t, PCM_SOURCE_EXT_ADDMIDIEVENTS};
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInBeats, DurationInSeconds, Hz, MidiImportBehavior,
    OwnedPcmSink, OwnedPcmSource, PositionInSeconds, TimeSignature,
};
use std::cmp;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut, NonNull};

// TODO-high-prebuffer In addition we should deploy a start-buffer that always keeps the start completely in
//  memory. Because sudden restarts (e.g. retriggers) are the main reason why we could still run
//  into a cache miss. That start-buffer should take the downbeat setting into account. It must
//  cache everything up to the downbeat + the usual start-buffer samples. It should probably sit
//  on top of the pre-buffer and serve samples at the beginning by itself, leaving the pre-buffer
//  out of the equation. It should forward pre-buffer requests to the pre-buffer but modify them
//  by using the end of the start-buffer cache as the minimum pre-buffer position.

#[derive(Debug)]
pub struct Recorder {
    state: Option<State>,
    request_sender: Sender<RecorderRequest>,
    response_channel: ResponseChannel,
}

#[derive(Debug)]
struct ResponseChannel {
    sender: Sender<RecorderResponse>,
    receiver: Receiver<RecorderResponse>,
}

impl ResponseChannel {
    fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::bounded(10);
        Self { sender, receiver }
    }
}

#[derive(Debug)]
pub enum RecorderRequest {
    FinishAudioRecording(FinishAudioRecordingRequest),
    DiscardSource(OwnedPcmSource),
    DiscardAudioRecordingFinishingData {
        temporary_audio_buffer: OwnedAudioBuffer,
        file: PathBuf,
        old_source: Option<OwnedPcmSource>,
    },
}

#[derive(Debug)]
pub struct FinishAudioRecordingRequest {
    sink: OwnedPcmSink,
    file: PathBuf,
    response_sender: Sender<RecorderResponse>,
}

#[derive(Debug)]
struct AudioRecordingFinishedResponse {
    pub source: Result<OwnedPcmSource, &'static str>,
}

#[derive(Debug)]
enum RecorderResponse {
    AudioRecordingFinished(AudioRecordingFinishedResponse),
}

/// State of the recorder.
///
/// This state is not necessarily synchronous with the clip state. In particular, after a recording,
/// the clip can already be in the Ready state and play the clip while the recorder is still in
/// the Recording state. In that state, the recorder delivers playable material from an in-memory
/// buffer. Until the PCM source is ready. Then it moves to the Ready state.
#[derive(Debug)]
enum State {
    Ready(ReadyState),
    Recording(RecordingState),
}

#[derive(Debug)]
struct ReadyState {
    source: OwnedPcmSource,
    /// For sending the result of overdubbing back to the main thread, we keep a mirror of the
    /// original source to which we apply the same modifications.
    midi_overdub_mirror_source: Option<OwnedPcmSource>,
}

#[derive(Debug)]
struct RecordingState {
    kind_state: KindState,
    old_source: Option<OwnedPcmSource>,
    project: Option<Project>,
    detect_downbeat: bool,
    tempo: Bpm,
    time_signature: TimeSignature,
    start_timing: RecordInteractionTiming,
    stop_timing: RecordInteractionTiming,
    recording: Option<Recording>,
    length: RecordLength,
    committed: bool,
    scheduled_end: Option<ScheduledEnd>,
    initial_play_start_timing: ClipPlayStartTiming,
}

#[derive(Clone, Copy, Debug)]
struct Recording {
    total_frame_offset: usize,
    num_count_in_frames: usize,
    frame_rate: Hz,
    first_play_frame: Option<usize>,
}

#[derive(Debug)]
enum KindState {
    Audio(RecordingAudioState),
    Midi(RecordingMidiState),
}

#[derive(Debug)]
enum RecordingAudioState {
    Active(RecordingAudioActiveState),
    Finishing(RecordingAudioFinishingState),
}

#[derive(Debug)]
struct RecordingAudioActiveState {
    file: PathBuf,
    file_clone: PathBuf,
    file_clone_2: PathBuf,
    sink: OwnedPcmSink,
    temporary_audio_buffer: OwnedAudioBuffer,
}

#[derive(Debug)]
struct RecordingAudioFinishingState {
    temporary_audio_buffer: OwnedAudioBuffer,
    file: PathBuf,
}

#[derive(Debug)]
struct RecordingMidiState {
    new_source: OwnedPcmSource,
    mirror_source: OwnedPcmSource,
}

impl KindState {
    fn new(equipment: RecordingEquipment) -> Self {
        use RecordingEquipment::*;
        match equipment {
            Midi(equipment) => {
                let recording_midi_state = RecordingMidiState {
                    new_source: equipment.empty_midi_source,
                    mirror_source: equipment.empty_midi_source_mirror,
                };
                Self::Midi(recording_midi_state)
            }
            Audio(equipment) => {
                let active_state = RecordingAudioActiveState {
                    file: equipment.file,
                    file_clone: equipment.file_clone,
                    file_clone_2: equipment.file_clone_2,
                    sink: equipment.pcm_sink,
                    temporary_audio_buffer: equipment.temporary_audio_buffer,
                };
                let recording_audio_state = RecordingAudioState::Active(active_state);
                Self::Audio(recording_audio_state)
            }
        }
    }

    pub fn is_midi(&self) -> bool {
        matches!(self, KindState::Midi(_))
    }
}

#[derive(Copy, Clone)]
pub struct WriteMidiRequest<'a> {
    pub audio_request_props: BasicAudioRequestProps,
    pub events: &'a BorrowedMidiEventList,
    // TODO-medium Filtering to one channel not supported at the moment.
    pub channel_filter: Option<Channel>,
}

#[derive(Copy, Clone)]
pub struct WriteAudioRequest<'a> {
    pub audio_request_props: BasicAudioRequestProps,
    pub left_buffer: AudioBuf<'a>,
    pub right_buffer: AudioBuf<'a>,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        debug!("Dropping recorder...");
    }
}

impl Recorder {
    /// Okay to call in real-time thread.
    pub fn ready(source: OwnedPcmSource, request_sender: Sender<RecorderRequest>) -> Self {
        let ready_state = ReadyState {
            source,
            midi_overdub_mirror_source: None,
        };
        Self::new(State::Ready(ready_state), request_sender)
    }

    pub fn recording(args: RecordingArgs, request_sender: Sender<RecorderRequest>) -> Self {
        let kind_state = KindState::new(args.equipment);
        let recording_state = RecordingState {
            kind_state,
            old_source: None,
            project: args.project,
            detect_downbeat: args.detect_downbeat,
            tempo: args.tempo,
            time_signature: args.time_signature,
            start_timing: args.start_timing,
            stop_timing: args.stop_timing,
            recording: None,
            length: args.length,
            committed: false,
            scheduled_end: None,
            initial_play_start_timing: args.initial_play_start_timing,
        };
        Self::new(State::Recording(recording_state), request_sender)
    }

    fn new(state: State, request_sender: Sender<RecorderRequest>) -> Self {
        Self {
            state: Some(state),
            request_sender,
            response_channel: ResponseChannel::new(),
        }
    }

    pub fn record_state(&self) -> Option<RecordState> {
        match self.state.as_ref().unwrap() {
            State::Ready(_) => None,
            State::Recording(s) => {
                use RecordState::*;
                let state = match s.recording {
                    None => ScheduledForStart,
                    Some(r) => {
                        if r.total_frame_offset < r.num_count_in_frames {
                            ScheduledForStart
                        } else if let Some(end) = s.scheduled_end {
                            if end.is_predefined {
                                Recording
                            } else {
                                ScheduledForStop
                            }
                        } else {
                            Recording
                        }
                    }
                };
                Some(state)
            }
        }
    }

    pub fn register_midi_overdub_mirror_source(
        &mut self,
        mirror_source: OwnedPcmSource,
    ) -> ClipEngineResult<()> {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => {
                if s.midi_overdub_mirror_source.is_some() {
                    return Err("recorder already has MIDI overdub mirror source");
                }
                s.midi_overdub_mirror_source = Some(mirror_source);
                Ok(())
            }
            State::Recording(_) => {
                Err("recorder can't take MIDI overdub mirror source because it's recording")
            }
        }
    }

    pub fn take_midi_overdub_mirror_source(&mut self) -> Option<OwnedPcmSource> {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => s.midi_overdub_mirror_source.take(),
            State::Recording(_) => None,
        }
    }

    /// Can be called in a real-time thread (doesn't allocate).
    pub fn prepare_recording(&mut self, args: RecordingArgs) -> ClipEngineResult<()> {
        use State::*;
        let (res, next_state) = match self.state.take().unwrap() {
            Ready(s) => {
                let recording_state = RecordingState {
                    kind_state: KindState::new(args.equipment),
                    old_source: Some(s.source),
                    project: args.project,
                    detect_downbeat: args.detect_downbeat,
                    tempo: args.tempo,
                    time_signature: args.time_signature,
                    start_timing: args.start_timing,
                    stop_timing: args.stop_timing,
                    recording: None,
                    length: args.length,
                    committed: false,
                    scheduled_end: None,
                    initial_play_start_timing: args.initial_play_start_timing,
                };
                (Ok(()), Recording(recording_state))
            }
            Recording(s) => (Err("already recording"), Recording(s)),
        };
        self.state = Some(next_state);
        res
    }

    pub fn stop_recording(
        &mut self,
        timeline: &HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
        audio_request_props: BasicAudioRequestProps,
    ) -> ClipEngineResult<StopRecordingOutcome> {
        let (res, next_state) = match self.state.take().unwrap() {
            State::Ready(s) => (Err("was not recording"), State::Ready(s)),
            State::Recording(s) => s.stop_recording(
                timeline,
                timeline_cursor_pos,
                audio_request_props,
                &self.request_sender,
                &self.response_channel.sender,
            ),
        };
        self.state = Some(next_state);
        res
    }

    /// Should be called once per block while in recording mode, before writing any material.
    ///
    /// Takes care of:
    ///
    /// - Creating the initial recording data and position.
    /// - Advancing the recording position for the next material.
    /// - Committing the recording as soon as the scheduled end is reached.
    pub fn poll_recording(
        &mut self,
        audio_request_props: BasicAudioRequestProps,
    ) -> PollRecordingOutcome {
        use State::*;
        let (outcome, next_state) = match self.state.take().unwrap() {
            Ready(s) => (PollRecordingOutcome::PleaseStopPolling, Ready(s)),
            Recording(s) => s.poll_recording(
                audio_request_props,
                &self.request_sender,
                &self.response_channel.sender,
            ),
        };
        self.state = Some(next_state);
        outcome
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) -> ClipEngineResult<()> {
        match self.state.as_mut().unwrap() {
            State::Ready(_) => Err("not recording"),
            State::Recording(s) => {
                if s.committed {
                    return Err("already committed");
                }
                match &mut s.kind_state {
                    KindState::Midi(_) => Err("recording MIDI, not audio"),
                    KindState::Audio(audio_state) => {
                        match audio_state {
                            RecordingAudioState::Active(active_state) => {
                                let recording = s
                                    .recording
                                    .ok_or("recording not started yet ... not polling?")?;
                                // Write into sink
                                let sink = active_state.sink.as_ref().as_ref();
                                const NCH: usize = 2;
                                let mut channels: [*mut f64; NCH] = [
                                    request.left_buffer.data_as_slice().as_ptr() as _,
                                    request.right_buffer.data_as_slice().as_ptr() as _,
                                ];
                                // TODO-high Write only part of the block until scheduled end
                                unsafe {
                                    sink.WriteDoubles(
                                        &mut channels as *mut _,
                                        request.audio_request_props.block_length as _,
                                        NCH as _,
                                        0,
                                        1,
                                    );
                                }
                                // Write into temporary buffer
                                let start_frame = recording.total_frame_offset;
                                let mut out_buf = active_state.temporary_audio_buffer.to_buf_mut();
                                let out_channel_count = out_buf.channel_count();
                                let ideal_end_frame =
                                    start_frame + request.audio_request_props.block_length;
                                let end_frame = cmp::min(ideal_end_frame, out_buf.frame_count());
                                let num_frames_written = end_frame - start_frame;
                                let out_buf_slice = out_buf.data_as_mut_slice();
                                let left_buf_slice = request.left_buffer.data_as_slice();
                                let right_buf_slice = request.right_buffer.data_as_slice();
                                for i in 0..num_frames_written {
                                    out_buf_slice
                                        [start_frame * out_channel_count + i * out_channel_count] =
                                        left_buf_slice[i];
                                    out_buf_slice[start_frame * out_channel_count
                                        + i * out_channel_count
                                        + 1] = right_buf_slice[i];
                                }
                                Ok(())
                            }
                            RecordingAudioState::Finishing(_) => {
                                unreachable!("audio can only be finishing if already committed")
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn write_midi(
        &mut self,
        request: WriteMidiRequest,
        overdub_frame: Option<usize>,
    ) -> ClipEngineResult<()> {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => match s.midi_overdub_mirror_source.as_mut() {
                None => Err("neither recording nor overdubbing"),
                Some(mirror_source) => {
                    write_midi(
                        request,
                        &mut s.source,
                        mirror_source,
                        overdub_frame.expect("no MIDI overdub frame given"),
                    );
                    Ok(())
                }
            },
            State::Recording(s) => {
                assert_eq!(s.committed, false, "MIDI doesn't use the committed state");
                match &mut s.kind_state {
                    KindState::Audio(_) => Err("recording audio, not MIDI"),
                    KindState::Midi(midi_state) => {
                        let recording = s
                            .recording
                            .ok_or("recording not started yet ... not polling?")?;
                        write_midi(
                            request,
                            &mut midi_state.new_source,
                            &mut midi_state.mirror_source,
                            recording.total_frame_offset,
                        );
                        Ok(())
                    }
                }
            }
        }
    }

    fn process_worker_response(&mut self) {
        let response = match self.response_channel.receiver.try_recv() {
            Ok(r) => r,
            Err(_) => return,
        };
        dbg!(&self.state);
        match response {
            RecorderResponse::AudioRecordingFinished(r) => {
                use State::*;
                let next_state = match self.state.take().unwrap() {
                    Recording(RecordingState {
                        kind_state: KindState::Audio(RecordingAudioState::Finishing(s)),
                        old_source,
                        ..
                    }) => match r.source {
                        Ok(source) => {
                            self.request_sender.discard_audio_recording_finishing_data(
                                s.temporary_audio_buffer,
                                s.file,
                                old_source,
                            );
                            let ready_state = ReadyState {
                                source,
                                midi_overdub_mirror_source: None,
                            };
                            Ready(ready_state)
                        }
                        Err(msg) => {
                            // TODO-high-record We should handle this more gracefully, not just let it
                            //  stuck in Finishing state. First by trying to roll back to the old
                            //  clip. If there's no old clip, either by making it possible to return
                            //  an instruction to clear the slot or by letting the worker not just
                            //  return an error message but an alternative empty source.
                            panic!("recording didn't finish successfully: {}", msg)
                        }
                    },
                    s => {
                        if let Ok(source) = r.source {
                            self.request_sender.discard_source(source);
                        }
                        s
                    }
                };
                self.state = Some(next_state);
            }
        }
    }
}
impl RecordingState {
    pub fn stop_recording(
        mut self,
        timeline: &HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
        audio_request_props: BasicAudioRequestProps,
        request_sender: &Sender<RecorderRequest>,
        response_sender: &Sender<RecorderResponse>,
    ) -> (ClipEngineResult<StopRecordingOutcome>, State) {
        match self.stop_timing {
            RecordInteractionTiming::Immediately => {
                // Commit immediately
                let (commit_result, next_state) =
                    self.commit_recording(request_sender, response_sender);
                (
                    commit_result.map(StopRecordingOutcome::Committed),
                    next_state,
                )
            }
            RecordInteractionTiming::Quantized(quantization) => {
                if self.scheduled_end.is_some() {
                    return (Err("end scheduled already"), State::Recording(self));
                }
                let rollback = match self.recording {
                    None => true,
                    Some(r) => r.total_frame_offset < r.num_count_in_frames,
                };
                if rollback {
                    // Zero point of recording hasn't even been reached yet. Cancel.
                    if let Some(old_source) = self.old_source {
                        // There's an old source to roll back to.
                        let ready_state = ReadyState {
                            source: old_source,
                            midi_overdub_mirror_source: None,
                        };
                        (
                            Ok(StopRecordingOutcome::Canceled),
                            State::Ready(ready_state),
                        )
                    } else {
                        // Nothing to roll back to. This whole chain will be removed in one moment.
                        (Ok(StopRecordingOutcome::Canceled), State::Recording(self))
                    }
                } else {
                    // Schedule end
                    self.schedule_end(
                        timeline,
                        timeline_cursor_pos,
                        audio_request_props,
                        quantization,
                    );
                    (
                        Ok(StopRecordingOutcome::EndScheduled),
                        State::Recording(self),
                    )
                }
            }
        }
    }

    pub fn schedule_end(
        &mut self,
        timeline: &HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
        audio_request_props: BasicAudioRequestProps,
        quantization: EvenQuantization,
    ) {
        let total_frame_offset = match self.recording {
            None => 0,
            Some(r) => r.total_frame_offset,
        };
        let scheduled_end = calculate_scheduled_end(
            timeline,
            timeline_cursor_pos,
            audio_request_props,
            quantization,
            total_frame_offset,
            self.kind_state.is_midi(),
            false,
        );
        self.scheduled_end = Some(scheduled_end);
    }

    pub fn poll_recording(
        mut self,
        audio_request_props: BasicAudioRequestProps,
        request_sender: &Sender<RecorderRequest>,
        response_sender: &Sender<RecorderResponse>,
    ) -> (PollRecordingOutcome, State) {
        if self.committed {
            return (
                PollRecordingOutcome::PleaseStopPolling,
                State::Recording(self),
            );
        }
        if let Some(recording) = self.recording.as_mut() {
            // Recording started already. Advancing position.
            // Advance recording position (for MIDI mainly)
            let num_source_frames = if self.kind_state.is_midi() {
                let num_midi_frames = convert_duration_in_frames_to_other_frame_rate(
                    audio_request_props.block_length,
                    audio_request_props.frame_rate,
                    MIDI_FRAME_RATE,
                );
                let timeline = clip_timeline(self.project, false);
                let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
                let tempo_factor = timeline_tempo.get() / MIDI_BASE_BPM.get();
                adjust_proportionally_positive(num_midi_frames as f64, tempo_factor)
            } else {
                audio_request_props.block_length
            };
            let next_frame_offset = recording.total_frame_offset + num_source_frames;
            recording.total_frame_offset = next_frame_offset;
            // Commit recording if end exceeded
            if let Some(scheduled_end) = self.scheduled_end {
                if next_frame_offset > scheduled_end.complete_length {
                    // Exceeded scheduled end.
                    let recording = *recording;
                    let (recording_outcome, next_state) =
                        self.commit_recording_internal(request_sender, response_sender, recording);
                    return (
                        PollRecordingOutcome::CommittedRecording(recording_outcome),
                        next_state,
                    );
                }
            }
            (
                PollRecordingOutcome::PleaseContinuePolling,
                State::Recording(self),
            )
        } else {
            // Recording not started yet. Do it now.
            let timeline = clip_timeline(self.project, false);
            let timeline_cursor_pos = timeline.cursor_pos();
            let timeline_tempo = timeline.tempo_at(timeline_cursor_pos);
            let (start_pos, frames_to_start_pos) = match self.start_timing {
                RecordInteractionTiming::Immediately => (timeline_cursor_pos, 0),
                RecordInteractionTiming::Quantized(quantization) => {
                    let equipment = QuantizedPosCalcEquipment::new_with_unmodified_tempo(
                        &timeline,
                        timeline_cursor_pos,
                        timeline_tempo,
                        audio_request_props,
                        self.kind_state.is_midi(),
                    );
                    let quantized_start_pos =
                        timeline.next_quantized_pos_at(timeline_cursor_pos, quantization);
                    debug!("Calculated quantized start pos {:?}", quantized_start_pos);
                    let start_pos = timeline.pos_of_quantized_pos(quantized_start_pos);
                    let frames_from_start_pos = calc_distance_from_pos(start_pos, equipment);
                    assert!(frames_from_start_pos < 0);
                    let frames_to_start_pos = (-frames_from_start_pos) as usize;
                    (start_pos, frames_to_start_pos)
                }
            };
            let recording = Recording {
                total_frame_offset: 0,
                num_count_in_frames: frames_to_start_pos,
                frame_rate: if self.kind_state.is_midi() {
                    MIDI_FRAME_RATE
                } else {
                    audio_request_props.frame_rate
                },
                first_play_frame: None,
            };
            self.recording = Some(recording);
            self.scheduled_end = self.calculate_predefined_scheduled_end(
                &timeline,
                audio_request_props,
                start_pos,
                frames_to_start_pos,
            );
            (
                PollRecordingOutcome::PleaseContinuePolling,
                State::Recording(self),
            )
        }
    }

    // May be called in real-time thread.
    pub fn commit_recording(
        self,
        request_sender: &Sender<RecorderRequest>,
        response_sender: &Sender<RecorderResponse>,
    ) -> (ClipEngineResult<RecordingOutcome>, State) {
        if self.committed {
            return (Err("already committed"), State::Recording(self));
        }
        let recording = match self.recording {
            None => return (Err("no input arrived yet"), State::Recording(self)),
            Some(r) => r,
        };
        let (recording_outcome, new_state) =
            self.commit_recording_internal(request_sender, response_sender, recording);
        (Ok(recording_outcome), new_state)
    }

    fn commit_recording_internal(
        self,
        request_sender: &Sender<RecorderRequest>,
        response_sender: &Sender<RecorderResponse>,
        recording: Recording,
    ) -> (RecordingOutcome, State) {
        let is_midi = self.kind_state.is_midi();
        let (kind_specific_outcome, new_state) = match self.kind_state {
            KindState::Audio(audio_state) => {
                let active_state = match audio_state {
                    RecordingAudioState::Active(s) => s,
                    RecordingAudioState::Finishing(_) => {
                        unreachable!(
                            "if recording not committed yet, audio state can't be finishing"
                        );
                    }
                };
                request_sender.finish_audio_recording(
                    active_state.sink,
                    active_state.file,
                    response_sender.clone(),
                );
                let outcome = KindSpecificRecordingOutcome::Audio {
                    path: active_state.file_clone,
                    channel_count: active_state.temporary_audio_buffer.to_buf().channel_count(),
                };
                let recording_state = RecordingState {
                    kind_state: {
                        let finishing_state = RecordingAudioFinishingState {
                            temporary_audio_buffer: active_state.temporary_audio_buffer,
                            file: active_state.file_clone_2,
                        };
                        KindState::Audio(RecordingAudioState::Finishing(finishing_state))
                    },
                    committed: true,
                    ..self
                };
                (outcome, State::Recording(recording_state))
            }
            KindState::Midi(midi_state) => {
                let outcome = KindSpecificRecordingOutcome::Midi {
                    mirror_source: midi_state.mirror_source,
                };
                let ready_state = ReadyState {
                    source: midi_state.new_source,
                    midi_overdub_mirror_source: None,
                };
                (outcome, State::Ready(ready_state))
            }
        };
        let recording_outcome = RecordingOutcome {
            data: CompleteRecordingData {
                frame_rate: recording.frame_rate,
                total_frame_count: recording.total_frame_offset,
                tempo: self.tempo,
                time_signature: self.time_signature,
                is_midi,
                section_bounds: SectionBounds::new(
                    recording.num_count_in_frames,
                    self.scheduled_end.map(|end| {
                        assert!(recording.num_count_in_frames < end.complete_length);
                        end.complete_length - recording.num_count_in_frames
                    }),
                ),
                quantized_end_pos: self.scheduled_end.map(|end| end.quantized_end_pos),
                normalized_downbeat_frame: 0,
                initial_play_start_timing: self.initial_play_start_timing,
            },
            kind_specific: kind_specific_outcome,
        };
        (recording_outcome, new_state)
    }

    fn calculate_predefined_scheduled_end(
        &self,
        timeline: &HybridTimeline,
        audio_request_props: BasicAudioRequestProps,
        start_pos: PositionInSeconds,
        frames_to_start_pos: usize,
    ) -> Option<ScheduledEnd> {
        match self.length {
            RecordLength::OpenEnd => None,
            RecordLength::Quantized(q) => {
                let end = calculate_scheduled_end(
                    timeline,
                    start_pos,
                    audio_request_props,
                    q,
                    frames_to_start_pos,
                    self.kind_state.is_midi(),
                    true,
                );
                Some(end)
            }
        }
    }
}

#[derive(Debug)]
pub enum RecordingEquipment {
    Midi(MidiRecordingEquipment),
    Audio(AudioRecordingEquipment),
}

impl RecordingEquipment {
    pub fn is_midi(&self) -> bool {
        matches!(self, Self::Midi(_))
    }
}

#[derive(Clone, Debug)]
pub struct MidiRecordingEquipment {
    empty_midi_source: OwnedPcmSource,
    empty_midi_source_mirror: OwnedPcmSource,
}

impl MidiRecordingEquipment {
    pub fn new() -> Self {
        Self {
            empty_midi_source: create_empty_midi_source(),
            empty_midi_source_mirror: create_empty_midi_source(),
        }
    }
}

#[derive(Debug)]
pub struct AudioRecordingEquipment {
    pcm_sink: OwnedPcmSink,
    temporary_audio_buffer: OwnedAudioBuffer,
    file: PathBuf,
    file_clone: PathBuf,
    file_clone_2: PathBuf,
}

impl AudioRecordingEquipment {
    pub fn new(project: Option<Project>, channel_count: usize) -> Self {
        let sink_outcome = create_audio_sink(project);
        Self {
            pcm_sink: sink_outcome.sink,
            // TODO-high Choose size wisely and explain
            temporary_audio_buffer: OwnedAudioBuffer::new(channel_count, 48000 * 10),
            file: sink_outcome.file.clone(),
            file_clone: sink_outcome.file.clone(),
            file_clone_2: sink_outcome.file,
        }
    }
}

/// Returns an empty MIDI source prepared for recording.
fn create_empty_midi_source() -> OwnedPcmSource {
    let mut source = OwnedSource::from_type("MIDI").unwrap();
    // The following seems to be the absolute minimum to create the shortest possible MIDI clip
    // (which still is longer than zero).
    let chunk = "\
        HASDATA 1 960 QN\n\
        E 1 b0 7b 00\n\
    >\n\
    ";
    source
        .set_state_chunk("<SOURCE MIDI\n", String::from(chunk))
        .unwrap();
    source.into_raw()
}

/// Project is necessary to create the sink.
fn create_audio_sink(project: Option<Project>) -> AudioSinkOutcome {
    let proj_ptr = project.map(|p| p.raw().as_ptr()).unwrap_or(null_mut());
    let file_name = get_path_for_new_media_file("clip-audio", "wav", project);
    let file_name_str = file_name.to_str().unwrap();
    let file_name_c_string = CString::new(file_name_str).unwrap();
    let sink = unsafe {
        let sink = Reaper::get().medium_reaper().low().PCM_Sink_CreateEx(
            proj_ptr,
            file_name_c_string.as_ptr(),
            null(),
            0,
            2,
            48000,
            false,
        );
        let sink = NonNull::new(sink).expect("PCM_Sink_CreateEx returned null");
        OwnedPcmSink::from_raw(sink)
    };
    AudioSinkOutcome {
        sink,
        file: file_name,
    }
}

struct AudioSinkOutcome {
    sink: OwnedPcmSink,
    file: PathBuf,
}

impl AudioSupplier for Recorder {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        self.process_worker_response();
        match self.state.as_mut().unwrap() {
            State::Ready(s) => s.source.supply_audio(request, dest_buffer),
            State::Recording(s) => {
                match &s.kind_state {
                    KindState::Audio(RecordingAudioState::Finishing(finishing_state)) => {
                        // The source is not ready yet but we have a temporary audio buffer that
                        // gives us the material we need.
                        // We know that the frame rates should be equal because this is audio and we
                        // do resampling in upper layers.
                        debug!("Using temporary buffer");
                        let recording = s
                            .recording
                            .expect("recording data must be set when audio recording finishing");
                        supply_audio_material(
                            request,
                            dest_buffer,
                            recording.frame_rate,
                            |input| {
                                transfer_samples_from_buffer(
                                    finishing_state.temporary_audio_buffer.to_buf(),
                                    input,
                                )
                            },
                        );
                        // Under the assumption that the frame rates are equal (which we asserted),
                        // the number of consumed frames is the number of written frames.
                        SupplyResponse::please_continue(dest_buffer.frame_count())
                    }
                    _ => {
                        if let Some(s) = &mut s.old_source {
                            s.supply_audio(request, dest_buffer)
                        } else {
                            panic!("attempt to play back audio material while recording with no previous source")
                        }
                    }
                }
            }
        }
    }
}

impl MidiSupplier for Recorder {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let source = match self.state.as_mut().unwrap() {
            State::Ready(s) => &mut s.source,
            State::Recording(s) => s
                .old_source
                .as_mut()
                .expect("attempt to play back MIDI without source"),
        };
        source.supply_midi(request, event_list)
    }
}

impl WithMaterialInfo for Recorder {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        match self.state.as_ref().unwrap() {
            State::Ready(s) => s.source.material_info(),
            State::Recording(s) => {
                let recording = match s.recording.as_ref() {
                    None => return Err(
                        "attempt to query material info although recording hasn't even started yet",
                    ),
                    Some(r) => r,
                };
                match &s.kind_state {
                    KindState::Audio(RecordingAudioState::Finishing(finishing_state)) => {
                        let info = AudioMaterialInfo {
                            channel_count: finishing_state
                                .temporary_audio_buffer
                                .to_buf()
                                .channel_count(),
                            frame_count: recording.total_frame_offset,
                            frame_rate: recording.frame_rate,
                        };
                        Ok(MaterialInfo::Audio(info))
                    }
                    _ => Err("attempt to query material info while recording"),
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct ScheduledEnd {
    quantized_end_pos: QuantizedPosition,
    /// This is the length from start of material, not from the scheduled start point.
    complete_length: usize,
    is_predefined: bool,
}

#[derive(Clone, Debug)]
pub struct RecordingOutcome {
    pub data: CompleteRecordingData,
    pub kind_specific: KindSpecificRecordingOutcome,
}

impl RecordingOutcome {
    pub fn material_info(&self) -> MaterialInfo {
        use KindSpecificRecordingOutcome::*;
        match &self.kind_specific {
            Midi { .. } => MaterialInfo::Midi(MidiMaterialInfo {
                frame_count: self.data.total_frame_count,
            }),
            Audio { channel_count, .. } => MaterialInfo::Audio(AudioMaterialInfo {
                channel_count: *channel_count,
                frame_count: self.data.effective_frame_count(),
                frame_rate: self.data.frame_rate,
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub enum KindSpecificRecordingOutcome {
    Midi { mirror_source: OwnedPcmSource },
    Audio { path: PathBuf, channel_count: usize },
}

#[derive(Clone, Debug)]
pub struct CompleteRecordingData {
    pub frame_rate: Hz,
    /// Doesn't take section bounds into account.
    pub total_frame_count: usize,
    pub tempo: Bpm,
    pub time_signature: TimeSignature,
    pub is_midi: bool,
    pub section_bounds: SectionBounds,
    pub normalized_downbeat_frame: usize,
    pub quantized_end_pos: Option<QuantizedPosition>,
    pub initial_play_start_timing: ClipPlayStartTiming,
}

impl CompleteRecordingData {
    pub fn effective_frame_count(&self) -> usize {
        self.section_bounds
            .calculate_frame_count(self.total_frame_count)
    }

    pub fn section_start_pos_in_seconds(&self) -> DurationInSeconds {
        convert_duration_in_frames_to_seconds(self.section_bounds.start_frame(), self.frame_rate)
    }

    pub fn section_length_in_seconds(&self) -> Option<DurationInSeconds> {
        let section_frame_count = self.section_bounds.length()?;
        Some(convert_duration_in_frames_to_seconds(
            section_frame_count,
            self.frame_rate,
        ))
    }

    pub fn downbeat_in_beats(&self) -> DurationInBeats {
        let downbeat_in_secs =
            convert_duration_in_frames_to_seconds(self.normalized_downbeat_frame, self.frame_rate);
        let bps = self.tempo.get() / 60.0;
        DurationInBeats::new(downbeat_in_secs.get() * bps)
    }
}

impl WithSource for Recorder {
    fn source(&self) -> Option<&OwnedPcmSource> {
        match self.state.as_ref().unwrap() {
            State::Ready(s) => Some(&s.source),
            State::Recording(_) => None,
        }
    }
}

pub struct RecordingArgs {
    pub equipment: RecordingEquipment,
    pub project: Option<Project>,
    pub timeline_cursor_pos: PositionInSeconds,
    pub tempo: Bpm,
    pub time_signature: TimeSignature,
    pub detect_downbeat: bool,
    pub start_timing: RecordInteractionTiming,
    pub stop_timing: RecordInteractionTiming,
    pub length: RecordLength,
    pub initial_play_start_timing: ClipPlayStartTiming,
}

impl RecordingArgs {
    pub fn from_stuff(
        project: Option<Project>,
        column_settings: &ColumnSettings,
        overridable_matrix_settings: &OverridableMatrixSettings,
        matrix_record_settings: &MatrixClipRecordSettings,
        recording_equipment: RecordingEquipment,
    ) -> Self {
        let timeline = clip_timeline(project, false);
        let timeline_cursor_pos = timeline.cursor_pos();
        let tempo = timeline.tempo_at(timeline_cursor_pos);
        let initial_play_start_timing = column_settings
            .clip_play_start_timing
            .unwrap_or(overridable_matrix_settings.clip_play_start_timing);
        let is_midi = recording_equipment.is_midi();
        RecordingArgs {
            equipment: recording_equipment,
            project,
            timeline_cursor_pos,
            tempo,
            time_signature: timeline.time_signature_at(timeline_cursor_pos),
            detect_downbeat: matrix_record_settings.downbeat_detection_enabled(is_midi),
            start_timing: RecordInteractionTiming::from_record_start_timing(
                matrix_record_settings.start_timing,
                initial_play_start_timing,
            ),
            stop_timing: RecordInteractionTiming::from_record_stop_timing(
                matrix_record_settings.stop_timing,
                matrix_record_settings.start_timing,
                initial_play_start_timing,
            ),
            length: matrix_record_settings.duration,
            initial_play_start_timing,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RecordInteractionTiming {
    Immediately,
    Quantized(EvenQuantization),
}

impl RecordInteractionTiming {
    pub fn from_record_start_timing(
        timing: ClipRecordStartTiming,
        play_start_timing: ClipPlayStartTiming,
    ) -> Self {
        use ClipRecordStartTiming::*;
        match timing {
            LikeClipPlayStartTiming => match play_start_timing {
                ClipPlayStartTiming::Immediately => Self::Immediately,
                ClipPlayStartTiming::Quantized(q) => Self::Quantized(q),
            },
            Immediately => Self::Immediately,
            Quantized(q) => Self::Quantized(q),
        }
    }

    pub fn from_record_stop_timing(
        timing: ClipRecordStopTiming,
        record_start_timing: ClipRecordStartTiming,
        play_start_timing: ClipPlayStartTiming,
    ) -> Self {
        use ClipRecordStopTiming::*;
        match timing {
            LikeClipRecordStartTiming => {
                Self::from_record_start_timing(record_start_timing, play_start_timing)
            }
            Immediately => Self::Immediately,
            Quantized(q) => Self::Quantized(q),
        }
    }
}

trait RecorderRequestSender {
    fn finish_audio_recording(
        &self,
        sink: OwnedPcmSink,
        file: PathBuf,
        response_sender: Sender<RecorderResponse>,
    );

    fn discard_source(&self, source: OwnedPcmSource);

    fn discard_audio_recording_finishing_data(
        &self,
        temporary_audio_buffer: OwnedAudioBuffer,
        file: PathBuf,
        old_source: Option<OwnedPcmSource>,
    );

    fn send_request(&self, request: RecorderRequest);
}

impl RecorderRequestSender for Sender<RecorderRequest> {
    fn finish_audio_recording(
        &self,
        sink: OwnedPcmSink,
        file: PathBuf,
        response_sender: Sender<RecorderResponse>,
    ) {
        let request = RecorderRequest::FinishAudioRecording(FinishAudioRecordingRequest {
            sink,
            file,
            response_sender,
        });
        self.send_request(request);
    }

    fn discard_source(&self, source: OwnedPcmSource) {
        let request = RecorderRequest::DiscardSource(source);
        self.send_request(request);
    }

    fn discard_audio_recording_finishing_data(
        &self,
        temporary_audio_buffer: OwnedAudioBuffer,
        file: PathBuf,
        old_source: Option<OwnedPcmSource>,
    ) {
        let request = RecorderRequest::DiscardAudioRecordingFinishingData {
            temporary_audio_buffer,
            file,
            old_source,
        };
        let _ = self.try_send(request);
    }

    fn send_request(&self, request: RecorderRequest) {
        self.try_send(request).unwrap();
    }
}

pub fn keep_processing_recorder_requests(receiver: Receiver<RecorderRequest>) {
    while let Ok(request) = receiver.recv() {
        use RecorderRequest::*;
        match request {
            FinishAudioRecording(r) => {
                let response = finish_audio_recording(r.sink, &r.file);
                // If the clip is not interested in the recording anymore, so what.
                let _ = r
                    .response_sender
                    .try_send(RecorderResponse::AudioRecordingFinished(response));
            }
            DiscardSource(_) => {}
            DiscardAudioRecordingFinishingData { .. } => {}
        }
    }
}

fn finish_audio_recording(sink: OwnedPcmSink, file: &Path) -> AudioRecordingFinishedResponse {
    std::mem::drop(sink);
    let source = OwnedSource::from_file(file, MidiImportBehavior::ForceNoMidiImport);
    AudioRecordingFinishedResponse {
        source: source.map(|s| s.into_raw()),
    }
}

fn write_midi(
    request: WriteMidiRequest,
    source: &mut OwnedPcmSource,
    mirror_source: &mut OwnedPcmSource,
    block_pos_frame: usize,
) {
    let global_time = convert_duration_in_frames_to_seconds(block_pos_frame, MIDI_FRAME_RATE);
    let mut write_struct = midi_realtime_write_struct_t {
        // Time within the source.
        global_time: global_time.get(),
        srate: request.audio_request_props.frame_rate.get(),
        item_playrate: 1.0,
        // This is the item position minus project start offset (project time of the start of
        // the MIDI source). The overdub mechanism would look at it in order to determine the
        // tempo. However, we want to work independently from REAPER's main timeline:
        // At source creation time, we set the source preview tempo to a constant value because
        // we control the tempo by modifying the frame rate (which allows us to do it while
        // playing). This in turn makes the overdub ignore project time, so the project tempo
        // and thus global_item_time doesn't matter anymore.
        global_item_time: 0.0,
        length: request.audio_request_props.block_length as _,
        // Overdub
        overwritemode: 0,
        events: unsafe { request.events.as_ptr().as_mut() },
        latency: 0.0,
        // Not used
        overwrite_actives: null_mut(),
    };
    debug!(
        "Write MIDI: Pos = {}s (= {} frames)",
        global_time.get(),
        block_pos_frame
    );
    unsafe {
        source.extended(
            PCM_SOURCE_EXT_ADDMIDIEVENTS as _,
            &mut write_struct as *mut _ as _,
            null_mut(),
            null_mut(),
        );
        mirror_source.extended(
            PCM_SOURCE_EXT_ADDMIDIEVENTS as _,
            &mut write_struct as *mut _ as _,
            null_mut(),
            null_mut(),
        );
    }
}

pub enum StopRecordingOutcome {
    Committed(RecordingOutcome),
    Canceled,
    EndScheduled,
}

pub enum PollRecordingOutcome {
    PleaseStopPolling,
    CommittedRecording(RecordingOutcome),
    PleaseContinuePolling,
}

impl PositionTranslationSkill for Recorder {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        play_pos
    }
}

fn calculate_scheduled_end(
    timeline: &HybridTimeline,
    timeline_cursor_pos: PositionInSeconds,
    audio_request_props: BasicAudioRequestProps,
    quantization: EvenQuantization,
    total_frame_offset: usize,
    is_midi: bool,
    is_predefined: bool,
) -> ScheduledEnd {
    let quantized_end_pos = timeline.next_quantized_pos_at(timeline_cursor_pos, quantization);
    debug!("Calculated quantized end pos {:?}", quantized_end_pos);
    let equipment = QuantizedPosCalcEquipment::new_with_unmodified_tempo(
        timeline,
        timeline_cursor_pos,
        timeline.tempo_at(timeline_cursor_pos),
        audio_request_props,
        is_midi,
    );
    let distance_from_end = calc_distance_from_quantized_pos(quantized_end_pos, equipment);
    assert!(distance_from_end < 0, "scheduled end before now");
    let distance_to_end = (-distance_from_end) as usize;
    let complete_length = total_frame_offset + distance_to_end;
    ScheduledEnd {
        quantized_end_pos,
        complete_length,
        is_predefined,
    }
}

pub enum RecordState {
    ScheduledForStart,
    Recording,
    ScheduledForStop,
}
