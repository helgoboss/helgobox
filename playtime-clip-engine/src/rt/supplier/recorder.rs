use crate::conversion_util::{
    adjust_anti_proportionally_positive, adjust_proportionally_positive,
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
};
use crate::file_util::get_path_for_new_media_file;
use crate::rt::buffer::{AudioBuf, AudioBufMut, OwnedAudioBuffer};
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, Cache, CacheRequest, CacheResponseChannel, MaterialInfo,
    MidiSupplier, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithMaterialInfo,
    WithSource, MIDI_BASE_BPM, MIDI_FRAME_RATE,
};
use crate::rt::ClipRecordArgs;
use crate::timeline::{clip_timeline, Timeline};
use crate::{ClipEngineResult, HybridTimeline, QuantizedPosition};
use crossbeam_channel::{Receiver, Sender};
use playtime_api::{
    AudioCacheBehavior, ClipPlayStartTiming, ClipRecordStartTiming, EvenQuantization, RecordLength,
};
use reaper_high::{OwnedSource, Project, Reaper};
use reaper_low::raw::{midi_realtime_write_struct_t, PCM_SOURCE_EXT_ADDMIDIEVENTS};
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, MidiImportBehavior, OwnedPcmSink,
    OwnedPcmSource, PositionInSeconds,
};
use std::cmp;
use std::convert::TryInto;
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
type RecorderCache = Cache<OwnedPcmSource>;

#[derive(Debug)]
pub struct Recorder {
    state: Option<State>,
    request_sender: Sender<RecorderRequest>,
    response_channel: ResponseChannel,
    cache_request_sender: Sender<CacheRequest>,
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
        old_cache: Option<RecorderCache>,
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
    cache: RecorderCache,
}

#[derive(Debug)]
struct RecordingState {
    kind_state: KindSpecificRecordingState,
    old_cache: Option<RecorderCache>,
    project: Option<Project>,
    detect_downbeat: bool,
    phase: Option<RecordingPhase>,
}

impl RecordingState {
    pub fn commit_recording(
        self,
        timeline: &dyn Timeline,
        request_sender: &Sender<RecorderRequest>,
        response_sender: &Sender<RecorderResponse>,
        cache_request_sender: &Sender<CacheRequest>,
    ) -> (ClipEngineResult<RecordingOutcome>, State) {
        use KindSpecificRecordingState::*;
        use State::*;
        let (next_state, source_duration) = match self.kind_state {
            Audio(ss) => {
                let sss = match ss {
                    RecordingAudioState::Active(s) => s,
                    RecordingAudioState::Finishing(s) => {
                        let recording_state = RecordingState {
                            kind_state: KindSpecificRecordingState::Audio(
                                RecordingAudioState::Finishing(s),
                            ),
                            ..self
                        };
                        return (Err("already committed"), Recording(recording_state));
                    }
                };
                // TODO-medium We should probably record a bit longer to have some
                //  crossfade material for the future.
                let source_duration =
                    DurationInSeconds::new(sss.sink.as_ref().as_ref().GetLength());
                let request = RecorderRequest::FinishAudioRecording(FinishAudioRecordingRequest {
                    sink: sss.sink,
                    file: sss.file,
                    response_sender: response_sender.clone(),
                });
                request_sender.try_send(request).unwrap();
                let finishing_state = RecordingAudioFinishingState {
                    temporary_audio_buffer: sss.temporary_audio_buffer,
                    frame_rate: self
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
                    ..self
                };
                (Recording(recording_state), source_duration)
            }
            Midi(ss) => {
                let source_duration = ss.new_source.get_length().unwrap();
                let ready_state = ReadyState {
                    cache: create_recorder_cache(ss.new_source, cache_request_sender.clone()),
                };
                (Ready(ready_state), source_duration)
            }
        };
        let outcome = match self.phase.unwrap() {
            RecordingPhase::Empty(_) => {
                Err("attempt to commit recording although no material arrived yet")
            }
            RecordingPhase::OpenEnd(p) => Ok(p.commit(source_duration, timeline)),
            RecordingPhase::EndScheduled(p) => Ok(p.commit(source_duration)),
            RecordingPhase::Committed => Err("already committed"),
        };
        (outcome, next_state)
    }
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
    // TODO-high-record Use
    _source_start_timeline_pos: PositionInSeconds,
}

impl KindSpecificRecordingState {
    fn new(equipment: RecordingEquipment, trigger_timeline_pos: PositionInSeconds) -> Self {
        use RecordingEquipment::*;
        match equipment {
            Midi(equipment) => {
                let recording_midi_state = RecordingMidiState {
                    new_source: equipment.empty_midi_source,
                    _source_start_timeline_pos: trigger_timeline_pos,
                };
                Self::Midi(recording_midi_state)
            }
            Audio(equipment) => {
                let active_state = RecordingAudioActiveState {
                    file: equipment.file,
                    file_clone: equipment.file_clone,
                    sink: equipment.pcm_sink,
                    temporary_audio_buffer: equipment.temporary_audio_buffer,
                    next_record_start_frame: 0,
                };
                let recording_audio_state = RecordingAudioState::Active(active_state);
                Self::Audio(recording_audio_state)
            }
        }
    }

    pub fn is_midi(&self) -> bool {
        matches!(self, Self::Midi(_))
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

fn create_recorder_cache(
    source: OwnedPcmSource,
    cache_request_sender: Sender<CacheRequest>,
) -> RecorderCache {
    Cache::new(source, cache_request_sender, CacheResponseChannel::new())
}

#[derive(Clone, Debug)]
pub struct RecorderEquipment {
    pub recorder_request_sender: Sender<RecorderRequest>,
    pub cache_request_sender: Sender<CacheRequest>,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        debug!("Dropping recorder...");
    }
}

impl Recorder {
    /// Okay to call in real-time thread.
    pub fn ready(source: OwnedPcmSource, equipment: RecorderEquipment) -> Self {
        let ready_state = ReadyState {
            cache: create_recorder_cache(source, equipment.cache_request_sender.clone()),
        };
        Self::new(State::Ready(ready_state), equipment)
    }

    pub fn recording(
        recording_equipment: RecordingEquipment,
        project: Option<Project>,
        trigger_timeline_pos: PositionInSeconds,
        tempo: Bpm,
        recorder_equipment: RecorderEquipment,
        detect_downbeat: bool,
        timing: RecordTiming,
    ) -> Self {
        let initial_phase = RecordingPhase::initial_phase(
            timing,
            tempo,
            recording_equipment.is_midi(),
            trigger_timeline_pos,
        );
        let kind_state = KindSpecificRecordingState::new(recording_equipment, trigger_timeline_pos);
        let recording_state = RecordingState {
            kind_state,
            old_cache: None,
            project,
            detect_downbeat,
            phase: Some(initial_phase),
        };
        Self::new(State::Recording(recording_state), recorder_equipment)
    }

    fn new(state: State, equipment: RecorderEquipment) -> Self {
        Self {
            state: Some(state),
            request_sender: equipment.recorder_request_sender.clone(),
            cache_request_sender: equipment.cache_request_sender.clone(),
            response_channel: ResponseChannel::new(),
        }
    }

    pub fn set_audio_cache_behavior(
        &mut self,
        cache_behavior: AudioCacheBehavior,
    ) -> ClipEngineResult<()> {
        match self.state.as_mut().unwrap() {
            State::Ready(s) => {
                use AudioCacheBehavior::*;
                let cache_enabled = match cache_behavior {
                    DirectFromDisk => false,
                    CacheInMemory => true,
                };
                if cache_enabled {
                    s.cache.enable();
                } else {
                    s.cache.disable();
                }
                Ok(())
            }
            State::Recording(_) => Err("can't set audio cache behavior while recording"),
        }
    }

    pub fn downbeat_pos_during_recording(&self, timeline: &dyn Timeline) -> DurationInSeconds {
        match self.state.as_ref().unwrap() {
            State::Ready(_) => Default::default(),
            State::Recording(s) => {
                use RecordingPhase::*;
                match s.phase.as_ref().unwrap() {
                    Empty(_) => {
                        panic!("attempt to query downbeat position in empty recording phase")
                    }
                    // Should usually not be called here. But we can provide a current snapshot.
                    OpenEnd(p) => p.snapshot(timeline).non_normalized_downbeat_pos,
                    EndScheduled(p) => p.non_normalized_downbeat_pos,
                    Committed => panic!(
                        "attempt to query downbeat position when recording already committed"
                    ),
                }
            }
        }
    }

    pub fn schedule_end(&mut self, end: QuantizedPosition, timeline: &dyn Timeline) {
        match self.state.as_mut().unwrap() {
            State::Ready(_) => panic!("attempt to schedule recording end while recorder ready"),
            State::Recording(s) => {
                use RecordingPhase::*;
                let next_phase = match s.phase.take().unwrap() {
                    RecordingPhase::Empty(_) => {
                        panic!("attempt to schedule recording end although no audio material arrived yet")
                    }
                    RecordingPhase::OpenEnd(p) => EndScheduled(p.schedule_end(end, timeline)),
                    // Idempotence
                    p => p,
                };
                s.phase = Some(next_phase);
            }
        }
    }

    pub fn is_midi(&self) -> bool {
        match self.state.as_ref().unwrap() {
            State::Ready(s) => pcm_source_is_midi(s.cache.source()),
            State::Recording(s) => s.kind_state.is_midi(),
        }
    }

    pub fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        match self.state.as_ref().unwrap() {
            State::Ready(s) => s.cache.source().material_info(),
            State::Recording(_) => todo!(),
        }
    }

    /// This must not be done in a real-time thread!
    pub fn prepare_recording(
        &mut self,
        equipment: RecordingEquipment,
        project: Option<Project>,
        trigger_timeline_pos: PositionInSeconds,
        tempo: Bpm,
        detect_downbeat: bool,
        timing: RecordTiming,
    ) {
        use State::*;
        let old_cache = match self.state.take().unwrap() {
            Ready(s) => Some(s.cache),
            Recording(s) => s.old_cache,
        };
        let initial_phase =
            RecordingPhase::initial_phase(timing, tempo, equipment.is_midi(), trigger_timeline_pos);
        let recording_state = RecordingState {
            kind_state: KindSpecificRecordingState::new(equipment, trigger_timeline_pos),
            old_cache,
            project,
            detect_downbeat,
            phase: Some(initial_phase),
        };
        self.state = Some(Recording(recording_state));
    }

    /// Can be called in a real-time thread (doesn't allocate).
    pub fn commit_recording(
        &mut self,
        timeline: &dyn Timeline,
    ) -> ClipEngineResult<RecordingOutcome> {
        use State::*;
        let (res, next_state) = match self.state.take().unwrap() {
            Ready(s) => (Err("not recording"), Ready(s)),
            Recording(s) => s.commit_recording(
                timeline,
                &self.request_sender,
                &self.response_channel.sender,
                &self.cache_request_sender,
            ),
        };
        self.state = Some(next_state);
        res
    }

    pub fn rollback_recording(&mut self) -> ClipEngineResult<()> {
        use State::*;
        let (res, next_state) = match self.state.take().unwrap() {
            Ready(s) => (Ok(()), Ready(s)),
            Recording(s) => {
                if let Some(old_source) = s.old_cache {
                    let ready_state = ReadyState { cache: old_source };
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
        unsafe {
            sink.WriteDoubles(
                &mut channels as *mut _,
                request.block_length as _,
                NCH as _,
                0,
                1,
            );
        }
        // Write into temporary buffer
        let start_frame = state.next_record_start_frame;
        let mut out_buf = state.temporary_audio_buffer.to_buf_mut();
        let out_channel_count = out_buf.channel_count();
        let ideal_end_frame = start_frame + request.block_length;
        let end_frame = cmp::min(ideal_end_frame, out_buf.frame_count());
        let num_frames_written = end_frame - start_frame;
        let out_buf_slice = out_buf.data_as_mut_slice();
        let left_buf_slice = request.left_buffer.data_as_slice();
        let right_buf_slice = request.right_buffer.data_as_slice();
        for i in 0..num_frames_written {
            out_buf_slice[start_frame * out_channel_count + i * out_channel_count] =
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
                                .find(|e| crate::midi_util::is_play_message(e.message()))
                            {
                                let block_frame =
                                    convert_duration_in_seconds_to_frames(pos, MIDI_FRAME_RATE);
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
            State::Ready(ReadyState { cache }) => cache.source_mut(),
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
                        old_cache,
                        ..
                    }) => match r.source {
                        Ok(source) => {
                            let request = RecorderRequest::DiscardAudioRecordingFinishingData {
                                temporary_audio_buffer: s.temporary_audio_buffer,
                                file: s.file,
                                old_cache,
                            };
                            let _ = self.request_sender.try_send(request);
                            let ready_state = ReadyState {
                                cache: create_recorder_cache(
                                    source,
                                    self.cache_request_sender.clone(),
                                ),
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
}

impl MidiRecordingEquipment {
    pub fn new() -> Self {
        Self {
            empty_midi_source: create_empty_midi_source(),
        }
    }
}

#[derive(Debug)]
pub struct AudioRecordingEquipment {
    pcm_sink: OwnedPcmSink,
    temporary_audio_buffer: OwnedAudioBuffer,
    file: PathBuf,
    file_clone: PathBuf,
}

impl AudioRecordingEquipment {
    pub fn new(project: Option<Project>, channel_count: usize) -> Self {
        let sink_outcome = create_audio_sink(project);
        Self {
            pcm_sink: sink_outcome.sink,
            // TODO-high Choose size wisely and explain
            temporary_audio_buffer: OwnedAudioBuffer::new(channel_count, 48000 * 10),
            file: sink_outcome.file.clone(),
            file_clone: sink_outcome.file,
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
        let cache = match self.state.as_mut().unwrap() {
            State::Ready(s) => &mut s.cache,
            State::Recording(s) => {
                match &s.kind_state {
                    KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                        // The source is not ready yet but we have a temporary audio buffer that
                        // gives us the material we need.
                        // We know that the frame rates should be equal because this is audio and we
                        // do resampling in upper layers.
                        debug!("Using temporary buffer");
                        supply_audio_material(request, dest_buffer, s.frame_rate, |input| {
                            transfer_samples_from_buffer(s.temporary_audio_buffer.to_buf(), input)
                        });
                        // Under the assumption that the frame rates are equal (which we asserted),
                        // the number of consumed frames is the number of written frames.
                        return SupplyResponse::please_continue(dest_buffer.frame_count());
                    }
                    _ => {
                        if let Some(s) = &mut s.old_cache {
                            return s.supply_audio(request, dest_buffer);
                        } else {
                            panic!("attempt to play back audio material while recording with no previous source")
                        }
                    }
                }
            }
        };
        cache.supply_audio(request, dest_buffer)
    }
}

impl MidiSupplier for Recorder {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let cache = match self.state.as_mut().unwrap() {
            State::Ready(s) => &mut s.cache,
            State::Recording(s) => s
                .old_cache
                .as_mut()
                .expect("attempt to play back MIDI without source"),
        };
        cache.supply_midi(request, event_list)
    }
}

impl WithMaterialInfo for Recorder {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        let cache = match self.state.as_ref().unwrap() {
            State::Ready(s) => &s.cache,
            State::Recording(s) => match &s.kind_state {
                KindSpecificRecordingState::Audio(RecordingAudioState::Finishing(s)) => {
                    let info = AudioMaterialInfo {
                        channel_count: s.temporary_audio_buffer.to_buf().channel_count(),
                        frame_count: convert_duration_in_seconds_to_frames(
                            s.source_duration,
                            s.frame_rate,
                        ),
                        frame_rate: s.frame_rate,
                    };
                    return Ok(MaterialInfo::Audio(info));
                }
                _ => s
                    .old_cache
                    .as_ref()
                    .ok_or("attempt to query material info without source")?,
            },
        };
        cache.material_info()
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
        is_midi: bool,
        trigger_timeline_pos: PositionInSeconds,
    ) -> Self {
        let empty = EmptyPhase {
            tempo,
            timing,
            is_midi,
        };
        if is_midi {
            // MIDI starts in phase two because source start position and frame rate are clear
            // right from he start.
            empty.advance(trigger_timeline_pos, MIDI_FRAME_RATE)
        } else {
            RecordingPhase::Empty(empty)
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
        let end = match &self.timing {
            RecordTiming::Unsynced => None,
            RecordTiming::Synced { end, .. } => *end,
        };
        let open_end_phase = OpenEndPhase {
            prev_phase: self,
            source_start_timeline_pos,
            frame_rate,
            first_play_frame: None,
        };
        if let Some(end) = end {
            // TODO-high-record Deal with end schulding when no audio material arrived yet
            let todo_timeline = clip_timeline(None, false);
            RecordingPhase::EndScheduled(open_end_phase.schedule_end(end, &todo_timeline))
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
    normalized_downbeat_frame: usize,
    non_normalized_downbeat_pos: DurationInSeconds,
    section_start_frame: usize,
}

impl OpenEndPhase {
    pub fn commit(
        self,
        source_duration: DurationInSeconds,
        timeline: &dyn Timeline,
    ) -> RecordingOutcome {
        let snapshot = self.snapshot(timeline);
        // TODO-high-record Deal with immediate recording stop
        let fake_duration = DurationInSeconds::ZERO;
        RecordingOutcome {
            frame_rate: self.frame_rate,
            tempo: self.prev_phase.tempo,
            is_midi: self.prev_phase.is_midi,
            source_duration,
            section_start_frame: snapshot.section_start_frame,
            normalized_downbeat_frame: snapshot.normalized_downbeat_frame,
            section_frame_count: None,
            effective_duration: fake_duration,
        }
    }

    pub fn schedule_end(
        self,
        end: QuantizedPosition,
        timeline: &dyn Timeline,
    ) -> EndScheduledPhase {
        let start = match self.prev_phase.timing {
            RecordTiming::Unsynced => {
                unimplemented!("scheduled end without scheduled start not supported")
            }
            RecordTiming::Synced { start, .. } => start,
        };
        let quantized_record_start_timeline_pos = timeline.pos_of_quantized_pos(start);
        let quantized_record_end_timeline_pos = timeline.pos_of_quantized_pos(end);
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
            normalized_downbeat_frame: snapshot.normalized_downbeat_frame,
            non_normalized_downbeat_pos: snapshot.non_normalized_downbeat_pos,
            section_start_frame: snapshot.section_start_frame,
            effective_duration,
        }
    }

    pub fn snapshot(&self, timeline: &dyn Timeline) -> PhaseTwoSnapshot {
        match self.prev_phase.timing {
            RecordTiming::Unsynced => PhaseTwoSnapshot {
                // When recording not scheduled, downbeat detection doesn't make sense.
                normalized_downbeat_frame: 0,
                non_normalized_downbeat_pos: DurationInSeconds::ZERO,
                section_start_frame: 0,
            },
            RecordTiming::Synced { start, .. } => {
                let quantized_record_start_timeline_pos = timeline.pos_of_quantized_pos(start);
                // TODO-high-record Depending on source_start_timeline_pos doesn't work when tempo changed
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
                let (effective_start_pos, normalized_first_play_frame) = if self.prev_phase.is_midi
                {
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
                match normalized_first_play_frame {
                    Some(f) if f < effective_start_frame => {
                        // We detected material that should play at count-in phase
                        // (also called pick-up beat or anacrusis). So the position of the downbeat in
                        // the material is greater than zero.
                        let downbeat_frame = effective_start_frame - f;
                        PhaseTwoSnapshot {
                            normalized_downbeat_frame: downbeat_frame,
                            non_normalized_downbeat_pos: {
                                let nn_frame = if self.prev_phase.is_midi {
                                    adjust_anti_proportionally_positive(
                                        downbeat_frame as f64,
                                        self.prev_phase.midi_tempo_factor(),
                                    )
                                } else {
                                    downbeat_frame
                                };
                                convert_duration_in_frames_to_seconds(nn_frame, self.frame_rate)
                            },
                            section_start_frame: f,
                        }
                    }
                    _ => {
                        // Either no play material arrived or too late, right of the scheduled start
                        // position. This is not a pick-up beat. Ignore it.
                        PhaseTwoSnapshot {
                            normalized_downbeat_frame: 0,
                            non_normalized_downbeat_pos: DurationInSeconds::ZERO,
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
    normalized_downbeat_frame: usize,
    non_normalized_downbeat_pos: DurationInSeconds,
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
            normalized_downbeat_frame: self.normalized_downbeat_frame,
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
    pub normalized_downbeat_frame: usize,
    /// If we have a section, then this corresponds to the duration of the section. If not,
    /// this corresponds to the duration of the source material.
    pub effective_duration: DurationInSeconds,
}

#[derive(Copy, Clone, Debug)]
pub enum RecordTiming {
    Unsynced,
    Synced {
        start: QuantizedPosition,
        end: Option<QuantizedPosition>,
    },
}

impl RecordTiming {
    pub fn from_args(
        args: &ClipRecordArgs,
        timeline: &HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
    ) -> Self {
        use ClipRecordStartTiming::*;
        match args.start_timing {
            LikeClipPlayStartTiming => {
                use ClipPlayStartTiming::*;
                match args.parent_play_start_timing {
                    Immediately => RecordTiming::Unsynced,
                    Quantized(q) => {
                        RecordTiming::resolve_synced(q, args.length, timeline, timeline_cursor_pos)
                    }
                }
            }
            Immediately => RecordTiming::Unsynced,
            Quantized(q) => {
                RecordTiming::resolve_synced(q, args.length, timeline, timeline_cursor_pos)
            }
        }
    }

    pub fn resolve_synced(
        start: EvenQuantization,
        length: RecordLength,
        timeline: &HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
    ) -> Self {
        let start = timeline.next_quantized_pos_at(timeline_cursor_pos, start);
        let end = match length {
            RecordLength::OpenEnd => None,
            RecordLength::Quantized(q) => {
                let resolved_start_pos = timeline.pos_of_quantized_pos(start);
                Some(timeline.next_quantized_pos_at(resolved_start_pos, q))
            }
        };
        Self::Synced { start, end }
    }
}
