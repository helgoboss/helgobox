use crate::buffer::AudioBufMut;
use crate::file_util::get_path_for_new_media_file;
use crate::supplier::{supply_source_material, transfer_samples_from_buffer, SupplyResponse};
use crate::ClipPlayState::Recording;
use crate::{
    adjust_proportionally, adjust_proportionally_positive, clip_timeline,
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioBuf,
    AudioSupplier, ClipContent, ClipInfo, ClipRecordInput, CreateClipContentMode, ExactDuration,
    ExactFrameCount, MidiSupplier, OwnedAudioBuffer, RecordTiming, SourceData, SupplyAudioRequest,
    SupplyMidiRequest, Timeline, WithFrameRate, WithTempo, MIDI_BASE_BPM, MIDI_FRAME_RATE,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use helgoboss_midi::ShortMessage;
use reaper_high::{OwnedSource, Project, Reaper, ReaperSource};
use reaper_low::raw::{
    midi_realtime_write_struct_t, PCM_SINK_EXT_CREATESOURCE, PCM_SOURCE_EXT_ADDMIDIEVENTS,
};
use reaper_low::{raw, PCM_source};
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, MidiImportBehavior, OwnedPcmSink,
    OwnedPcmSource, PcmSource, PositionInSeconds, ReaperString,
};
use std::convert::TryInto;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut, NonNull};
use std::{cmp, mem};

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

impl Default for ResponseChannel {
    fn default() -> Self {
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
            DiscardSource(r) => {
                let _ = r;
            }
            DiscardAudioRecordingFinishingData {
                temporary_audio_buffer,
                file,
            } => {
                let _ = temporary_audio_buffer;
                let _ = file;
            }
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
}

#[derive(Debug)]
struct RecordingState {
    kind_state: KindSpecificRecordingState,
    old_source: Option<OwnedPcmSource>,
    project: Option<Project>,
    detect_downbeat: bool,
    phase: Option<RecordingPhase>,
}

#[derive(Debug)]
enum KindSpecificRecordingState {
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
    sink: OwnedPcmSink,
    temporary_audio_buffer: OwnedAudioBuffer,
    next_record_start_frame: usize,
}

#[derive(Debug)]
struct RecordingAudioFinishingState {
    temporary_audio_buffer: OwnedAudioBuffer,
    frame_rate: Hz,
    source_duration: DurationInSeconds,
    file: PathBuf,
}

#[derive(Debug)]
struct RecordingMidiState {
    new_source: OwnedPcmSource,
    source_start_timeline_pos: PositionInSeconds,
}

impl KindSpecificRecordingState {
    fn new(
        input: ClipRecordInput,
        project: Option<Project>,
        trigger_timeline_pos: PositionInSeconds,
    ) -> Self {
        use ClipRecordInput::*;
        match input {
            Midi => {
                let recording_midi_state = RecordingMidiState {
                    new_source: create_empty_midi_source(),
                    source_start_timeline_pos: trigger_timeline_pos,
                };
                Self::Midi(recording_midi_state)
            }
            Audio => {
                let outcome = create_audio_sink(project);
                let active_state = RecordingAudioActiveState {
                    file: outcome.file.clone(),
                    file_clone: outcome.file,
                    sink: outcome.sink,
                    temporary_audio_buffer: OwnedAudioBuffer::new(2, 48000 * 10),
                    next_record_start_frame: 0,
                };
                let recording_audio_state = RecordingAudioState::Active(active_state);
                Self::Audio(recording_audio_state)
            }
        }
    }
}

#[derive(Copy, Clone)]
pub struct WriteMidiRequest<'a> {
    pub input_sample_rate: Hz,
    pub block_length: usize,
    pub events: &'a BorrowedMidiEventList,
}

#[derive(Copy, Clone)]
pub struct WriteAudioRequest<'a> {
    pub input_sample_rate: Hz,
    pub block_length: usize,
    pub left_buffer: AudioBuf<'a>,
    pub right_buffer: AudioBuf<'a>,
}

impl Recorder {
    pub fn ready(source: OwnedPcmSource, request_sender: Sender<RecorderRequest>) -> Self {
        let ready_state = ReadyState { source };
        Self::new(State::Ready(ready_state), request_sender)
    }

    pub fn recording(
        input: ClipRecordInput,
        project: Option<Project>,
        trigger_timeline_pos: PositionInSeconds,
        tempo: Bpm,
        request_sender: Sender<RecorderRequest>,
        detect_downbeat: bool,
        timing: RecordTiming,
    ) -> Self {
        let kind_state = KindSpecificRecordingState::new(input, project, trigger_timeline_pos);
        let initial_phase =
            RecordingPhase::initial_phase(timing, tempo, input, trigger_timeline_pos);
        let recording_state = RecordingState {
            kind_state,
            old_source: None,
            project,
            detect_downbeat,
            phase: Some(initial_phase),
        };
        Self::new(State::Recording(recording_state), request_sender)
    }

    fn new(state: State, request_sender: Sender<RecorderRequest>) -> Self {
        Self {
            state: Some(state),
            request_sender,
            response_channel: Default::default(),
        }
    }

    pub fn downbeat_pos_during_recording(&self, timeline: &dyn Timeline) -> DurationInSeconds {
        match self.state.as_ref().unwrap() {
            State::Ready(_) => Default::default(),
            State::Recording(s) => {
                use RecordingPhase::*;
                let (frame, frame_rate) = match s.phase.as_ref().unwrap() {
                    Empty(_) => {
                        panic!("attempt to query downbeat position in empty recording phase")
                    }
                    // Should usually not be called here. But we can provide a current snapshot.
                    OpenEnd(p) => (p.snapshot(timeline).downbeat_frame, p.frame_rate),
                    EndScheduled(p) => (p.downbeat_frame, p.prev_phase.frame_rate),
                    Committed => panic!(
                        "attempt to query downbeat position when recording already committed"
                    ),
                };
                convert_duration_in_frames_to_seconds(frame, frame_rate)
            }
        }
    }

    pub fn schedule_end(&mut self, end_bar: i32, timeline: &dyn Timeline) {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => panic!("attempt to schedule recording end while recorder ready"),
            State::Recording(s) => {
                use RecordingPhase::*;
                let next_phase = match s.phase.take().unwrap() {
                    RecordingPhase::Empty(_) => {
                        panic!("attempt to schedule recording end although no audio material arrived yet")
                    }
                    RecordingPhase::OpenEnd(p) => EndScheduled(p.schedule_end(end_bar, timeline)),
                    // Idempotence
                    p => p,
                };
                s.phase = Some(next_phase);
            }
        }
    }

    pub fn clip_info(&self) -> Option<ClipInfo> {
        let info = match self.state.as_ref().unwrap() {
            State::Ready(s) => {
                ClipInfo {
                    r#type: s.source.get_type(|t| t.to_string()),
                    file_name: s.source.get_file_name(|p| Some(p?.to_owned())),
                    length: {
                        // TODO-low Doesn't need to be optional
                        Some(s.source.duration())
                    },
                }
            }
            State::Recording(s) => return None,
        };
        Some(info)
    }

    pub fn clip_content(&self, project: Option<Project>) -> Option<ClipContent> {
        let source = match self.state.as_ref().unwrap() {
            State::Ready(s) => &s.source,
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                    return Some(ClipContent::from_file(project, &s.file))
                }
                _ => s.old_source.as_ref()?,
            },
        };
        let source = ReaperSource::new(source.as_ptr());
        let content = ClipContent::from_reaper_source(
            &source,
            CreateClipContentMode::AllowEmbeddedData,
            project,
        );
        Some(content.unwrap())
    }

    /// This must not be done in a real-time thread!
    pub fn prepare_recording(
        &mut self,
        input: ClipRecordInput,
        project: Option<Project>,
        trigger_timeline_pos: PositionInSeconds,
        tempo: Bpm,
        detect_downbeat: bool,
        timing: RecordTiming,
    ) {
        use State::*;
        let old_source = match self.state.take().unwrap() {
            Ready(s) => Some(s.source),
            Recording(s) => s.old_source,
        };
        let initial_phase =
            RecordingPhase::initial_phase(timing, tempo, input, trigger_timeline_pos);
        let recording_state = RecordingState {
            kind_state: KindSpecificRecordingState::new(input, project, trigger_timeline_pos),
            old_source,
            project,
            detect_downbeat,
            phase: Some(initial_phase),
        };
        self.state = Some(Recording(recording_state));
    }

    /// Can be called in a real-time thread (doesn't allocate).
    pub fn commit_recording(
        &mut self,
        start_and_end_bar: Option<(i32, i32)>,
        timeline: &dyn Timeline,
    ) -> Result<RecordingOutcome, &'static str> {
        use State::*;
        let (res, next_state) = match self.state.take().unwrap() {
            Ready(s) => (Err("not recording"), Ready(s)),
            Recording(s) => {
                use KindSpecificRecordingState::*;
                let (next_state, source_duration) = match s.kind_state {
                    Audio(ss) => {
                        let sss = match ss {
                            RecordingAudioState::Active(s) => s,
                            // TODO-high This destroys the state.
                            RecordingAudioState::Finishing(_) => return Err("already committed"),
                        };
                        // TODO-medium We should probably record a bit longer to have some
                        //  crossfade material for the future.
                        let source_duration =
                            DurationInSeconds::new(sss.sink.as_ref().as_ref().GetLength());
                        let request =
                            RecorderRequest::FinishAudioRecording(FinishAudioRecordingRequest {
                                sink: sss.sink,
                                file: sss.file,
                                response_sender: self.response_channel.sender.clone(),
                            });
                        self.request_sender
                            .try_send(request)
                            .map_err(|_| "couldn't send request to finish audio recording")?;
                        let finishing_state = RecordingAudioFinishingState {
                            temporary_audio_buffer: sss.temporary_audio_buffer,
                            frame_rate: s
                                .phase
                                .as_ref()
                                .unwrap()
                                .frame_rate()
                                .expect("frame rate not available yet"),
                            file: sss.file_clone,
                            source_duration,
                        };
                        let recording_state = RecordingState {
                            kind_state: Audio(RecordingAudioState::Finishing(finishing_state)),
                            phase: Some(RecordingPhase::Committed),
                            ..s
                        };
                        (Recording(recording_state), source_duration)
                    }
                    Midi(ss) => {
                        let source_duration = ss.new_source.get_length().unwrap();
                        (
                            Ready(ReadyState {
                                source: ss.new_source,
                            }),
                            source_duration,
                        )
                    }
                };
                let outcome = match s.phase.unwrap() {
                    RecordingPhase::Empty(_) => {
                        Err("attempt to commit recording although no material arrived yet")
                    }
                    RecordingPhase::OpenEnd(p) => Ok(p.commit(source_duration, timeline)),
                    RecordingPhase::EndScheduled(p) => Ok(p.commit(source_duration)),
                    RecordingPhase::Committed => Err("already committed"),
                };
                (outcome, next_state)
            }
        };
        self.state = Some(next_state);
        res
    }

    pub fn rollback_recording(&mut self) -> Result<(), &'static str> {
        use State::*;
        let (res, next_state) = match self.state.take().unwrap() {
            Ready(s) => (Ok(()), Ready(s)),
            Recording(s) => {
                if let Some(old_source) = s.old_source {
                    let ready_state = ReadyState { source: old_source };
                    (Ok(()), Ready(ready_state))
                } else {
                    (Err("nothing to roll back to"), Recording(s))
                }
            }
        };
        self.state = Some(next_state);
        res
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) {
        let (state, project, phase) = match self.state.as_mut().unwrap() {
            State::Recording(RecordingState {
                kind_state: KindSpecificRecordingState::Audio(RecordingAudioState::Active(s)),
                project,
                phase,
                ..
            }) => (s, project, phase),
            _ => return,
        };
        // Advance phase
        use RecordingPhase::*;
        let next_phase = match phase.take().unwrap() {
            Empty(p) => {
                let timeline = clip_timeline(*project, false);
                p.advance(timeline.cursor_pos(), request.input_sample_rate)
            }
            p => p,
        };
        *phase = Some(next_phase);
        // Write into sink
        let sink = state.sink.as_ref().as_ref();
        const NCH: usize = 2;
        let mut channels: [*mut f64; NCH] = [
            request.left_buffer.data_as_slice().as_ptr() as _,
            request.right_buffer.data_as_slice().as_ptr() as _,
        ];
        sink.WriteDoubles(
            &mut channels as *mut _,
            request.block_length as _,
            NCH as _,
            0,
            1,
        );
        // Write into temporary buffer
        let start_frame = state.next_record_start_frame;
        let mut out_buf = state.temporary_audio_buffer.to_buf_mut();
        let out_channel_count = out_buf.channel_count();
        let ideal_end_frame = start_frame + request.block_length;
        let end_frame = cmp::min(ideal_end_frame, out_buf.frame_count());
        let num_frames_written = end_frame - start_frame;
        let mut out_buf_slice = out_buf.data_as_mut_slice();
        let left_buf_slice = request.left_buffer.data_as_slice();
        let right_buf_slice = request.right_buffer.data_as_slice();
        for i in 0..num_frames_written {
            out_buf_slice[start_frame * out_channel_count + i * out_channel_count + 0] =
                left_buf_slice[i];
            out_buf_slice[start_frame * out_channel_count + i * out_channel_count + 1] =
                right_buf_slice[i];
        }
        state.next_record_start_frame += num_frames_written;
    }

    pub fn write_midi(&mut self, request: WriteMidiRequest, pos: DurationInSeconds) {
        let source = match self.state.as_mut().unwrap() {
            State::Recording(RecordingState {
                kind_state:
                    KindSpecificRecordingState::Midi(RecordingMidiState {
                        new_source: source, ..
                    }),
                detect_downbeat,
                phase,
                ..
            }) => {
                use RecordingPhase::*;
                match phase.as_mut().unwrap() {
                    Empty(_) => unreachable!("MIDI shouldn't start in empty phase"),
                    OpenEnd(p) => {
                        if *detect_downbeat && p.first_play_frame.is_none() {
                            if let Some(evt) = request
                                .events
                                .into_iter()
                                .find(|e| e.message().is_note_on())
                            {
                                let block_frame = convert_duration_in_seconds_to_frames(
                                    pos,
                                    Hz::new(MIDI_FRAME_RATE),
                                );
                                let event_frame = block_frame + evt.frame_offset().get() as usize;
                                p.first_play_frame = Some(event_frame);
                            }
                        }
                    }
                    _ => {}
                };
                source
            }
            // Overdubbing existing clip
            State::Ready(ReadyState { source }) => source,
            _ => return,
        };
        let mut write_struct = midi_realtime_write_struct_t {
            global_time: pos.get(),
            srate: request.input_sample_rate.get(),
            item_playrate: 1.0,
            global_item_time: 0.0,
            length: request.block_length as _,
            // Overdub
            overwritemode: 0,
            events: unsafe { request.events.as_ptr().as_mut() },
            latency: 0.0,
            // Not used
            overwrite_actives: null_mut(),
        };
        unsafe {
            source.extended(
                PCM_SOURCE_EXT_ADDMIDIEVENTS as _,
                &mut write_struct as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn current_or_old_source(&self) -> Option<&OwnedPcmSource> {
        match self.state.as_ref().unwrap() {
            State::Ready(s) => Some(&s.source),
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(_)) => None,
                _ => s.old_source.as_ref(),
            },
        }
    }

    fn current_or_old_source_mut(&mut self) -> Option<&mut OwnedPcmSource> {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => Some(&mut s.source),
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(_)) => None,
                _ => s.old_source.as_mut(),
            },
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
                        kind_state:
                            KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)),
                        ..
                    }) => match r.source {
                        Ok(source) => {
                            let request = RecorderRequest::DiscardAudioRecordingFinishingData {
                                temporary_audio_buffer: s.temporary_audio_buffer,
                                file: s.file,
                            };
                            let _ = self.request_sender.try_send(request);
                            let ready_state = ReadyState { source };
                            Ready(ready_state)
                        }
                        Err(msg) => {
                            // TODO-high We should handle this more gracefully, not just let it
                            //  stuck in Finishing state. First by trying to roll back to the old
                            //  clip. If there's no old clip, either by making it possible to return
                            //  an instruction to clear the slot or by letting the worker not just
                            //  return an error message but an alternative empty source.
                            panic!("recording didn't finish successfully: {}", msg)
                        }
                    },
                    s => {
                        if let Ok(source) = r.source {
                            let _ = self
                                .request_sender
                                .try_send(RecorderRequest::DiscardSource(source));
                        }
                        s
                    }
                };
                self.state = Some(next_state);
            }
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
        let source = match self.state.as_mut().unwrap() {
            State::Ready(s) => &mut s.source,
            State::Recording(s) => {
                match &s.kind_state {
                    KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                        // The source is not ready yet but we have a temporary audio buffer that
                        // gives us the material we need.
                        // We know that the frame rates should be equal because this is audio and we
                        // do resampling in upper layers.
                        println!("Using temporary buffer");
                        supply_source_material(request, dest_buffer, s.frame_rate, |input| {
                            transfer_samples_from_buffer(s.temporary_audio_buffer.to_buf(), input)
                        });
                        // Under the assumption that the frame rates are equal (which we asserted),
                        // the number of consumed frames is the number of written frames.
                        return SupplyResponse::please_continue(dest_buffer.frame_count());
                    }
                    _ => {
                        if let Some(s) = &mut s.old_source {
                            return s.supply_audio(request, dest_buffer);
                        } else {
                            panic!("attempt to play back audio material while recording with no previous source")
                        }
                    }
                }
            }
        };
        source.supply_audio(request, dest_buffer)
    }

    fn channel_count(&self) -> usize {
        let source = match self.state.as_ref().unwrap() {
            State::Ready(s) => &s.source,
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                    return s.temporary_audio_buffer.to_buf().channel_count();
                }
                _ => s
                    .old_source
                    .as_ref()
                    .expect("attempt to get channel count while recording with no previous source"),
            },
        };
        source.channel_count()
    }
}

impl MidiSupplier for Recorder {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let source = self
            .current_or_old_source_mut()
            .expect("attempt to play back MIDI without source");
        source.supply_midi(request, event_list)
    }
}

impl ExactFrameCount for Recorder {
    fn frame_count(&self) -> usize {
        let source = match self.state.as_ref().unwrap() {
            State::Ready(s) => &s.source,
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                    return convert_duration_in_seconds_to_frames(s.source_duration, s.frame_rate);
                }
                _ => s
                    .old_source
                    .as_ref()
                    .expect("attempt to query frame count without source"),
            },
        };
        source.frame_count()
    }
}

impl ExactDuration for Recorder {
    fn duration(&self) -> DurationInSeconds {
        let source = match self.state.as_ref().unwrap() {
            State::Ready(s) => &s.source,
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                    return s.source_duration
                }
                _ => s
                    .old_source
                    .as_ref()
                    .expect("attempt to query duration without source"),
            },
        };
        source.duration()
    }
}

impl WithFrameRate for Recorder {
    fn frame_rate(&self) -> Option<Hz> {
        let source = match self.state.as_ref().unwrap() {
            State::Ready(s) => &s.source,
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                    return Some(s.frame_rate)
                }
                _ => s
                    .old_source
                    .as_ref()
                    .expect("attempt to query frame rate without source"),
            },
        };
        source.frame_rate()
    }
}

#[derive(Debug)]
enum RecordingPhase {
    /// We have a provisional section start position already but some important material info is
    /// not available yet (audio only).
    Empty(EmptyPhase),
    /// Frame rate and source start position are clear.
    ///
    /// This phase might be enriched with a non-zero downbeat position if downbeat detection is
    /// enabled. In this case, the provisional section start position will change.
    OpenEnd(OpenEndPhase),
    /// Section frame count is clear.
    EndScheduled(EndScheduledPhase),
    /// Source duration is clear.
    Committed,
}

impl RecordingPhase {
    fn initial_phase(
        timing: RecordTiming,
        tempo: Bpm,
        input: ClipRecordInput,
        trigger_timeline_pos: PositionInSeconds,
    ) -> Self {
        let empty = EmptyPhase {
            tempo,
            timing,
            is_midi: input.is_midi(),
        };
        match input {
            ClipRecordInput::Midi => {
                // MIDI starts in phase two because source start position and frame rate are clear
                // right from he start.
                empty.advance(trigger_timeline_pos, Hz::new(MIDI_FRAME_RATE))
            }
            ClipRecordInput::Audio => RecordingPhase::Empty(empty),
        }
    }

    fn frame_rate(&self) -> Option<Hz> {
        use RecordingPhase::*;
        match self {
            Empty(_) => None,
            OpenEnd(s) => Some(s.frame_rate),
            EndScheduled(s) => Some(s.prev_phase.frame_rate),
            Committed => None,
        }
    }
}

#[derive(Debug)]
struct EmptyPhase {
    tempo: Bpm,
    timing: RecordTiming,
    is_midi: bool,
}

impl EmptyPhase {
    fn advance(
        self,
        source_start_timeline_pos: PositionInSeconds,
        frame_rate: Hz,
    ) -> RecordingPhase {
        let end_bar = match &self.timing {
            RecordTiming::Unsynced => None,
            RecordTiming::Synced { end_bar, .. } => *end_bar,
        };
        let open_end_phase = OpenEndPhase {
            prev_phase: self,
            source_start_timeline_pos,
            frame_rate,
            first_play_frame: None,
        };
        if let Some(end_bar) = end_bar {
            RecordingPhase::EndScheduled(open_end_phase.schedule_end(end_bar, todo!()))
        } else {
            RecordingPhase::OpenEnd(open_end_phase)
        }
    }

    /// MIDI has a constant normalized tempo.
    ///
    /// This tempo factor must be used to adjust positions and durations that are measured using the
    /// actual recording tempo in order to conform to the normalized tempo.
    fn midi_tempo_factor(&self) -> f64 {
        self.tempo.get() / MIDI_BASE_BPM
    }
}

#[derive(Debug)]
struct OpenEndPhase {
    prev_phase: EmptyPhase,
    source_start_timeline_pos: PositionInSeconds,
    frame_rate: Hz,
    first_play_frame: Option<usize>,
}

struct PhaseTwoSnapshot {
    downbeat_frame: usize,
    section_start_frame: usize,
}

impl OpenEndPhase {
    pub fn commit(
        self,
        source_duration: DurationInSeconds,
        timeline: &dyn Timeline,
    ) -> RecordingOutcome {
        let snapshot = self.snapshot(timeline);
        RecordingOutcome {
            frame_rate: self.frame_rate,
            tempo: self.prev_phase.tempo,
            is_midi: self.prev_phase.is_midi,
            source_duration,
            section_start_frame: snapshot.section_start_frame,
            downbeat_frame: snapshot.downbeat_frame,
            section_frame_count: None,
            effective_duration: todo!(),
        }
    }

    pub fn schedule_end(self, end_bar: i32, timeline: &dyn Timeline) -> EndScheduledPhase {
        let start_bar = match self.prev_phase.timing {
            RecordTiming::Unsynced => {
                unimplemented!("scheduled end without scheduled start not supported")
            }
            RecordTiming::Synced { start_bar, .. } => start_bar,
        };
        let quantized_record_start_timeline_pos = timeline.pos_of_bar(start_bar);
        let quantized_record_end_timeline_pos = timeline.pos_of_bar(end_bar);
        let duration = quantized_record_end_timeline_pos - quantized_record_start_timeline_pos;
        let duration: DurationInSeconds = duration.try_into().expect("end bar pos < start bar pos");
        // Determine section data
        let effective_duration = if self.prev_phase.is_midi {
            // MIDI has a constant normalized tempo.
            let tempo_factor = self.prev_phase.midi_tempo_factor();
            DurationInSeconds::new(duration.get() * tempo_factor)
        } else {
            duration
        };
        let effective_frame_count =
            convert_duration_in_seconds_to_frames(effective_duration, self.frame_rate);
        let snapshot = self.snapshot(timeline);
        EndScheduledPhase {
            prev_phase: self,
            section_frame_count: Some(effective_frame_count),
            downbeat_frame: snapshot.downbeat_frame,
            section_start_frame: snapshot.section_start_frame,
            effective_duration,
        }
    }

    pub fn snapshot(&self, timeline: &dyn Timeline) -> PhaseTwoSnapshot {
        match self.prev_phase.timing {
            RecordTiming::Unsynced => PhaseTwoSnapshot {
                // When recording not scheduled, downbeat detection doesn't make sense.
                downbeat_frame: 0,
                section_start_frame: 0,
            },
            RecordTiming::Synced { start_bar, end_bar } => {
                let quantized_record_start_timeline_pos = timeline.pos_of_bar(start_bar);
                // TODO-high Depending on source_start_timeline_pos doesn't work when tempo changed
                //  during recording. It would be better to advance frames just like we do it
                //  when counting in (e.g. provide advance() function that's called in process()).
                let start_pos =
                    quantized_record_start_timeline_pos - self.source_start_timeline_pos;
                let start_pos: DurationInSeconds = start_pos
                    .try_into()
                    // Recorder started recording material after quantized. This can only happen
                    // if the preparation of the PCM sink was not fast enough. In future we should
                    // probably set the position of the section on the canvas by exactly the
                    // abs() of that negative start position, to keep at least the timing perfect.
                    .unwrap_or(DurationInSeconds::ZERO);
                let (effective_start_pos, effective_first_play_frame) = if self.prev_phase.is_midi {
                    let tempo_factor = self.prev_phase.midi_tempo_factor();
                    let adjusted_start_pos = DurationInSeconds::new(start_pos.get() * tempo_factor);
                    let adjusted_first_play_frame = self
                        .first_play_frame
                        .map(|frame| adjust_proportionally_positive(frame as f64, tempo_factor));
                    (adjusted_start_pos, adjusted_first_play_frame)
                } else {
                    (start_pos, self.first_play_frame)
                };
                let effective_start_frame =
                    convert_duration_in_seconds_to_frames(effective_start_pos, self.frame_rate);
                match effective_first_play_frame {
                    Some(f) if f < effective_start_frame => {
                        // We detected material that should play at count-in phase
                        // (also called pick-up beat or anacrusis). So the position of the downbeat in
                        // the material is greater than zero.
                        let downbeat_frame = effective_start_frame - f;
                        PhaseTwoSnapshot {
                            downbeat_frame,
                            section_start_frame: f,
                        }
                    }
                    _ => {
                        // Either no play material arrived or too late, right of the scheduled start
                        // position. This is not a pick-up beat. Ignore it.
                        PhaseTwoSnapshot {
                            downbeat_frame: 0,
                            section_start_frame: effective_start_frame,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct EndScheduledPhase {
    prev_phase: OpenEndPhase,
    section_frame_count: Option<usize>,
    downbeat_frame: usize,
    section_start_frame: usize,
    effective_duration: DurationInSeconds,
}

impl EndScheduledPhase {
    pub fn commit(self, source_duration: DurationInSeconds) -> RecordingOutcome {
        RecordingOutcome {
            frame_rate: self.prev_phase.frame_rate,
            tempo: self.prev_phase.prev_phase.tempo,
            is_midi: self.prev_phase.prev_phase.is_midi,
            source_duration,
            section_start_frame: self.section_start_frame,
            downbeat_frame: self.downbeat_frame,
            section_frame_count: self.section_frame_count,
            effective_duration: self.effective_duration,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordingOutcome {
    pub frame_rate: Hz,
    pub tempo: Bpm,
    pub is_midi: bool,
    /// Duration of the source material.
    pub source_duration: DurationInSeconds,
    pub section_start_frame: usize,
    pub section_frame_count: Option<usize>,
    pub downbeat_frame: usize,
    /// If we have a section, then this corresponds to the duration of the section. If not,
    /// this corresponds to the duration of the source material.
    pub effective_duration: DurationInSeconds,
}
