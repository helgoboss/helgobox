use crate::buffer::AudioBufMut;
use crate::file_util::get_path_for_new_media_file;
use crate::ClipPlayState::Recording;
use crate::{
    clip_timeline, convert_duration_in_seconds_to_frames, AudioBuf, AudioSupplier, ClipContent,
    ClipInfo, ClipRecordInput, CreateClipContentMode, ExactDuration, ExactFrameCount, MidiSupplier,
    OwnedAudioBuffer, SourceData, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, Timeline,
    WithFrameRate, WithTempo, MIDI_BASE_BPM,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
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
    sub_state: RecordingSubState,
    old_source: Option<OwnedPcmSource>,
    project: Option<Project>,
    initial_tempo: Bpm,
}

#[derive(Debug)]
enum RecordingSubState {
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
    /// If `None`, no material has been written yet
    lazy_material_data: Option<AudioMaterialData>,
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
struct AudioMaterialData {
    source_start_timeline_pos: PositionInSeconds,
    frame_rate: Hz,
}

#[derive(Debug)]
struct RecordingMidiState {
    new_source: OwnedPcmSource,
    source_start_timeline_pos: PositionInSeconds,
}

impl RecordingSubState {
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
                    lazy_material_data: None,
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
        current_tempo: Bpm,
        request_sender: Sender<RecorderRequest>,
    ) -> Self {
        let recording_state = RecordingState {
            sub_state: RecordingSubState::new(input, project, trigger_timeline_pos),
            old_source: None,
            project,
            initial_tempo: current_tempo,
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
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(s)) => {
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
        current_tempo: Bpm,
    ) {
        use State::*;
        let old_source = match self.state.take().unwrap() {
            Ready(s) => Some(s.source),
            Recording(s) => s.old_source,
        };
        let recording_state = RecordingState {
            sub_state: RecordingSubState::new(input, project, trigger_timeline_pos),
            old_source,
            project,
            initial_tempo: current_tempo,
        };
        self.state = Some(Recording(recording_state));
    }

    /// Can be called in a real-time thread (doesn't allocate).
    pub fn commit_recording(
        &mut self,
        start_and_end_bar: Option<(i32, i32)>,
        timeline: &dyn Timeline,
        timeline_cursor_pos: PositionInSeconds,
    ) -> Result<RecordingOutcome, &'static str> {
        use State::*;
        let (res, next_state) = match self.state.take().unwrap() {
            Ready(s) => (Err("not recording"), Ready(s)),
            Recording(s) => {
                use RecordingSubState::*;
                match s.sub_state {
                    Audio(ss) => {
                        let sss = match ss {
                            RecordingAudioState::Active(s) => s,
                            RecordingAudioState::Finishing(_) => return Err("already committed"),
                        };
                        // TODO-medium We should probably record a bit longer to have some
                        //  crossfade material for the future.
                        let request =
                            RecorderRequest::FinishAudioRecording(FinishAudioRecordingRequest {
                                sink: sss.sink,
                                file: sss.file,
                                response_sender: self.response_channel.sender.clone(),
                            });
                        self.request_sender
                            .try_send(request)
                            .map_err(|_| "couldn't send request to finish audio recording")?;
                        let material_data =
                            sss.lazy_material_data.ok_or("no material data available")?;
                        let outcome = RecordingOutcome::new(
                            material_data.source_start_timeline_pos,
                            material_data.frame_rate,
                            s.initial_tempo,
                            false,
                            start_and_end_bar,
                            timeline,
                            timeline_cursor_pos,
                        );
                        let finishing_state = RecordingAudioFinishingState {
                            temporary_audio_buffer: sss.temporary_audio_buffer,
                            frame_rate: material_data.frame_rate,
                            file: sss.file_clone,
                            source_duration: outcome.source_duration,
                        };
                        let recording_state = RecordingState {
                            sub_state: Audio(RecordingAudioState::Finishing(finishing_state)),
                            ..s
                        };
                        (Ok(outcome), Recording(recording_state))
                    }
                    Midi(ss) => {
                        let outcome = RecordingOutcome::new(
                            ss.source_start_timeline_pos,
                            ss.new_source.frame_rate().unwrap(),
                            s.initial_tempo,
                            true,
                            start_and_end_bar,
                            timeline,
                            timeline_cursor_pos,
                        );
                        (
                            Ok(outcome),
                            Ready(ReadyState {
                                source: ss.new_source,
                            }),
                        )
                    }
                }
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
        let (state, project) = match self.state.as_mut().unwrap() {
            State::Recording(RecordingState {
                sub_state: RecordingSubState::Audio(RecordingAudioState::Active(s)),
                project,
                ..
            }) => (s, project),
            _ => return,
        };
        // Write into sink
        if state.lazy_material_data.is_none() {
            let timeline = clip_timeline(*project, false);
            let material_data = AudioMaterialData {
                source_start_timeline_pos: timeline.cursor_pos(),
                frame_rate: request.input_sample_rate,
            };
            state.lazy_material_data = Some(material_data);
        }
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

    pub fn write_midi(&mut self, request: WriteMidiRequest, pos: PositionInSeconds) {
        let source = match self.state.as_mut().unwrap() {
            State::Recording(RecordingState {
                sub_state:
                    RecordingSubState::Midi(RecordingMidiState {
                        new_source: source, ..
                    }),
                ..
            })
            | State::Ready(ReadyState { source }) => source,
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
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(_)) => None,
                _ => s.old_source.as_ref(),
            },
        }
    }

    fn current_or_old_source_mut(&mut self) -> Option<&mut OwnedPcmSource> {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => Some(&mut s.source),
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(_)) => None,
                _ => s.old_source.as_mut(),
            },
        }
    }

    fn process_response(&mut self) {
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
                        sub_state: RecordingSubState::Audio(RecordingAudioState::Finishing(s)),
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
        self.process_response();
        let source = match self.state.as_mut().unwrap() {
            State::Ready(s) => &mut s.source,
            State::Recording(s) => {
                match &s.sub_state {
                    RecordingSubState::Audio(RecordingAudioState::Finishing(s)) => {
                        // The source is not ready yet but we have a temporary audio buffer that
                        // gives us the material we need.
                        // We know that the frame rates should be equal because this is audio and we
                        // do resampling in upper layers.
                        assert_eq!(
                            s.frame_rate, request.dest_sample_rate,
                            "frame rate of recorded material != requested frame rate"
                        );
                        if request.start_frame < 0 {
                            // TODO-high This skips the first few frames.
                            let num_frames_written = dest_buffer.frame_count();
                            return SupplyResponse::limited(
                                num_frames_written,
                                num_frames_written,
                                request.start_frame,
                                false,
                            );
                        }
                        println!("Using temporary buffer");
                        let start_frame = request.start_frame as usize;
                        let temp_buf = s.temporary_audio_buffer.to_buf();
                        let ideal_end_frame = start_frame + dest_buffer.frame_count();
                        let end_frame = cmp::min(ideal_end_frame, temp_buf.frame_count());
                        let num_frames_to_write = end_frame - start_frame;
                        temp_buf
                            .slice(start_frame..end_frame)
                            .copy_to(&mut dest_buffer.slice_mut(..num_frames_to_write))
                            .unwrap();
                        let num_frames_written = dest_buffer.frame_count();
                        // Under the assumption that the frame rates are equal (which they should),
                        // the number of consumed frames is the number of written frames.
                        return SupplyResponse::limited_by_total_frame_count(
                            num_frames_written,
                            num_frames_written,
                            request.start_frame,
                            None,
                        );
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
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(s)) => {
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
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(s)) => {
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
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(s)) => {
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
            State::Recording(s) => match &s.sub_state {
                RecordingSubState::Audio(RecordingAudioState::Finishing(s)) => {
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

pub struct RecordingOutcome {
    pub frame_rate: Hz,
    pub tempo: Bpm,
    pub is_midi: bool,
    /// Duration of the source material.
    pub source_duration: DurationInSeconds,
    pub section_start_frame: usize,
    pub section_frame_count: Option<usize>,
    /// If we have a section, then this corresponds to the duration of the section. If not,
    /// this corresponds to the duration of the source material.
    pub effective_duration: DurationInSeconds,
}

impl RecordingOutcome {
    fn new(
        source_start_timeline_pos: PositionInSeconds,
        frame_rate: Hz,
        tempo: Bpm,
        is_midi: bool,
        start_and_end_bar: Option<(i32, i32)>,
        timeline: &dyn Timeline,
        timeline_cursor_pos: PositionInSeconds,
    ) -> Self {
        struct R {
            section_start_frame: usize,
            section_frame_count: Option<usize>,
            effective_duration: DurationInSeconds,
        }
        let source_duration =
            DurationInSeconds::new(timeline_cursor_pos.get() - source_start_timeline_pos.get());
        let r = match start_and_end_bar {
            None => R {
                section_start_frame: 0,
                section_frame_count: None,
                effective_duration: source_duration,
            },
            Some((start_bar, end_bar)) => {
                // Determine start pos
                let quantized_record_start_timeline_pos = timeline.pos_of_bar(start_bar);
                let start_pos = quantized_record_start_timeline_pos - source_start_timeline_pos;
                let positive_start_pos = if start_pos.get() >= 0.0 {
                    DurationInSeconds::new(start_pos.get())
                } else {
                    // Recorder started recording material after quantized. This can only happen
                    // if the preparation of the PCM sink was not fast enough. In future we should
                    // probably set the position of the section on the canvas by exactly the
                    // abs() of that negative start position, to keep at least the timing perfect.
                    DurationInSeconds::ZERO
                };
                // Determine length
                let quantized_record_end_timeline_pos = timeline.pos_of_bar(end_bar);
                let length =
                    quantized_record_end_timeline_pos - quantized_record_start_timeline_pos;
                let positive_length: DurationInSeconds =
                    length.try_into().expect("end bar pos < start bar pos");
                // Determine source data based on section, not on recorded source
                let (section_start_pos, effective_duration) = if is_midi {
                    // MIDI has a constant normalized tempo.
                    let tempo_factor = tempo.get() / MIDI_BASE_BPM;
                    let adjusted_start_pos =
                        DurationInSeconds::new(positive_start_pos.get() * tempo_factor);
                    let adjusted_positive_length =
                        DurationInSeconds::new(positive_length.get() * tempo_factor);
                    (adjusted_start_pos, adjusted_positive_length)
                } else {
                    (positive_start_pos, positive_length)
                };
                let section_start_frame =
                    convert_duration_in_seconds_to_frames(section_start_pos, frame_rate);
                let section_frame_count =
                    convert_duration_in_seconds_to_frames(effective_duration, frame_rate);
                R {
                    section_start_frame,
                    section_frame_count: Some(section_frame_count),
                    effective_duration,
                }
            }
        };
        Self {
            frame_rate,
            tempo,
            is_midi,
            source_duration,
            section_start_frame: r.section_start_frame,
            section_frame_count: r.section_frame_count,
            effective_duration: r.effective_duration,
        }
    }
}
