use crate::buffer::{AudioBuf, AudioBufMut, OwnedAudioBuffer};
use crate::source_util::pcm_source_is_midi;
use crate::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::supplier::{
    AudioSupplier, ExactFrameCount, MidiSupplier, SupplyAudioRequest, SupplyMidiRequest,
    SupplyResponse, WithFrameRate,
};
use crate::SupplyResponseStatus::PleaseContinue;
use crate::{
    get_source_frame_rate, ExactDuration, PreBufferFillRequest, PreBufferSourceSkill,
    SupplyRequestInfo, SupplyResponseStatus, WithSource,
};
use core::cmp;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use derive_more::Display;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, OwnedPcmSource,
    PcmSourceTransfer,
};
use rtrb::{Consumer, PeekError, Producer, RingBuffer};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::BuildHasherDefault;
use std::ops::Range;
use std::sync::atomic;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use std::{iter, thread};
use twox_hash::XxHash64;

#[derive(Debug)]
pub struct PreBuffer<S> {
    id: PreBufferInstanceId,
    enabled: bool,
    request_sender: Sender<PreBufferRequest>,
    supplier: S,
    consumer: Consumer<PreBufferedBlock>,
    /// If we know the underlying supplier doesn't deliver count-in material, we should set this
    /// to `true`. An important optimization that saves supplier queries.
    skip_count_in_phase_material: bool,
}

trait PreBufferSender {
    fn recycle_block(&self, block: PreBufferedBlock);
}

impl PreBufferSender for Sender<PreBufferRequest> {
    fn recycle_block(&self, block: PreBufferedBlock) {
        let request = PreBufferRequest::Recycle(block);
        self.try_send(request).unwrap();
    }
}

#[derive(Debug)]
pub struct PreBufferedBlock {
    start_frame: isize,
    frame_rate: Hz,
    buffer: OwnedAudioBuffer,
    response: SupplyResponse,
}

struct MatchCriteria {
    start_frame: isize,
    channel_count: usize,
    frame_rate: Hz,
    /// This is just a wish. We are also satisfied if the block offers less frames.
    desired_frame_count: usize,
}

impl PreBufferedBlock {
    fn new(channel_count: usize) -> Self {
        Self {
            start_frame: 0,
            frame_rate: Default::default(),
            buffer: OwnedAudioBuffer::new(channel_count, PRE_BUFFERED_BLOCK_LENGTH),
            response: SupplyResponse::default(),
        }
    }

    fn try_apply_to(
        &self,
        remaining_dest_buffer: &mut AudioBufMut,
        criteria: &MatchCriteria,
    ) -> Result<ApplyOutcome, MatchError> {
        let range = self.matches(&criteria)?;
        // Pre-buffered block available and matches.
        Ok(self.copy_range_to(remaining_dest_buffer, range))
    }

    /// Checks whether this block contains at least the given start position and hopefully more.
    ///
    /// It returns the range of this block which can be used to fill the
    fn matches(&self, criteria: &MatchCriteria) -> Result<Range<usize>, MatchError> {
        let self_buffer = self.buffer.to_buf();
        if self_buffer.channel_count() != criteria.channel_count {
            return Err(MatchError::WrongChannelCount);
        }
        if self.frame_rate != criteria.frame_rate {
            return Err(MatchError::WrongFrameRate);
        }
        // At this point we know the channel count is correct.
        let offset = criteria.start_frame - self.start_frame;
        if offset < 0 {
            // Start frame is in the future.
            let block_end = offset + criteria.desired_frame_count as isize;
            let err = if block_end < self_buffer.frame_count() as isize {
                MatchError::BlockContainsFutureMaterial
            } else {
                MatchError::BlockContainsOnlyRelevantMaterialButStartFrameIsInFuture
            };
            return Err(err);
        }
        let offset = offset as usize;
        // At this point we know the start frame is in the past or spot-on.
        let num_available_frames = self_buffer.frame_count() as isize - offset as isize;
        if num_available_frames <= 0 {
            return Err(MatchError::BlockContainsOnlyPastMaterial);
        }
        let num_available_frames = num_available_frames as usize;
        // At this point we know we have usable material.
        let length = cmp::min(criteria.desired_frame_count, num_available_frames);
        Ok(offset..(offset + length))
    }

    fn copy_range_to(
        &self,
        remaining_dest_buffer: &mut AudioBufMut,
        range: Range<usize>,
    ) -> ApplyOutcome {
        let pre_buf = self.buffer.to_buf();
        debug_assert!(range.end <= pre_buf.frame_count());
        // Check if we reached end.
        use SupplyResponseStatus::*;
        let (clamped_range_end, reached_end) = match self.response.status {
            PleaseContinue => {
                // Pre-buffered block doesn't contain end.
                (range.end, false)
            }
            ReachedEnd { num_frames_written } => {
                // Pre-buffered block contains end.
                if range.end < num_frames_written {
                    // But requested block is not there yet.
                    (range.end, false)
                } else {
                    // Requested block reached end.
                    (num_frames_written, true)
                }
            }
        };
        // Copy material from pre-buffered block to destination buffer
        let range = range.start..clamped_range_end;
        let sliced_src_buffer = pre_buf.slice(range.clone());
        let mut sliced_dest_buffer = remaining_dest_buffer.slice_mut(0..range.len());
        sliced_src_buffer.copy_to(&mut sliced_dest_buffer);
        // Express outcome
        ApplyOutcome {
            partial_response: SupplyResponse {
                num_frames_consumed: range.len(),
                status: if reached_end {
                    ReachedEnd {
                        num_frames_written: range.len(),
                    }
                } else {
                    PleaseContinue
                },
            },
            block_exhausted: reached_end || range.end == pre_buf.frame_count(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum MatchError {
    /// Channel count of the pre-buffered block doesn't match the requested channel count.
    WrongChannelCount,
    /// Frame rate of the pre-buffered block doesn't match the requested frame rate.
    WrongFrameRate,
    /// Start frame of the pre-buffered block is in the future but all of its material would
    /// belong into the requested block.
    BlockContainsOnlyRelevantMaterialButStartFrameIsInFuture,
    /// Start frame of the pre-buffered block is in the future and it contains material that
    /// doesn't belong into the requested block.
    BlockContainsFutureMaterial,
    /// All material in the pre-buffered block is in the past.
    BlockContainsOnlyPastMaterial,
}

impl MatchError {
    fn should_consume_block(&self) -> bool {
        use MatchError::*;
        match self {
            WrongChannelCount | WrongFrameRate => true,
            BlockContainsOnlyRelevantMaterialButStartFrameIsInFuture => true,
            BlockContainsFutureMaterial => false,
            BlockContainsOnlyPastMaterial => true,
        }
    }
}

#[derive(Debug)]
pub enum PreBufferRequest {
    RegisterInstance {
        id: PreBufferInstanceId,
        producer: Producer<PreBufferedBlock>,
        supplier: Box<dyn AudioSupplier + Send + 'static>,
    },
    Recycle(PreBufferedBlock),
    KeepFillingFrom {
        id: PreBufferInstanceId,
        args: PreBufferFillRequest,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display)]
pub struct PreBufferInstanceId(usize);

impl PreBufferInstanceId {
    pub fn next() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        Self(COUNTER.fetch_add(1, atomic::Ordering::SeqCst))
    }
}

impl<S: AudioSupplier + Clone + Send + 'static> PreBuffer<S> {
    /// Don't call in real-time thread.
    pub fn new(supplier: S, request_sender: Sender<PreBufferRequest>) -> Self {
        let (producer, consumer) = RingBuffer::new(RING_BUFFER_BLOCK_COUNT);
        let id = PreBufferInstanceId::next();
        let request = PreBufferRequest::RegisterInstance {
            id,
            producer,
            supplier: Box::new(supplier.clone()),
        };
        request_sender.try_send(request).unwrap();
        Self {
            id,
            enabled: false,
            request_sender,
            supplier,
            consumer,
            // We know we sit right above the source and this one can't deliver material in the
            // count-in phase. This is good for performance, especially when crossing the
            // zero boundary.
            skip_count_in_phase_material: true,
        }
    }

    fn pre_buffer_internal(&mut self, args: PreBufferFillRequest) {
        // Not sufficiently thought about what to do if consumer wants to pre-buffer from a negative
        // start frame. Probably normalization to 0 because we know we sit on the source. Let's see.
        debug_assert!(args.start_frame >= 0);
        let request = PreBufferRequest::KeepFillingFrom { id: self.id, args };
        self.request_sender.try_send(request).unwrap();
        self.recycle_next_n_blocks(self.consumer.slots());
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn recycle_next_n_blocks(&mut self, count: usize) {
        for block in self.consumer.read_chunk(count).unwrap().into_iter() {
            self.request_sender.recycle_block(block);
        }
    }

    /// This is an optimization we *can* (and should) apply only because we know we sit right
    /// above the source, which by definition doesn't have any material in the count-in phase.
    fn skip_count_in_phase(
        &self,
        start_frame: isize,
        dest_buffer: &mut AudioBufMut,
    ) -> SkipCountInPhaseOutcome {
        if start_frame >= 0 {
            // Not in count-in phase.
            return SkipCountInPhaseOutcome::StartWithFrameOffset(0);
        }
        let num_frames_until_zero = -start_frame as usize;
        let num_frames_to_be_silenced = cmp::min(num_frames_until_zero, dest_buffer.frame_count());
        dest_buffer.slice_mut(0..num_frames_to_be_silenced).clear();
        if num_frames_to_be_silenced == dest_buffer.frame_count() {
            // Pure count-in.
            let response = SupplyResponse::please_continue(num_frames_to_be_silenced);
            SkipCountInPhaseOutcome::PureCountIn(response)
        } else {
            SkipCountInPhaseOutcome::StartWithFrameOffset(num_frames_to_be_silenced)
        }
    }

    fn query_supplier_for_remaining_portion(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
        frame_offset: usize,
    ) -> SupplyResponse {
        let inner_request = SupplyAudioRequest {
            start_frame: request.start_frame + frame_offset as isize,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset + frame_offset,
                requester: "pre-buffer-fallback",
                note: "",
                is_realtime: false,
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let mut remaining_dest_buffer = dest_buffer.slice_mut(frame_offset..);
        let inner_response = self
            .supplier
            .supply_audio(&inner_request, &mut remaining_dest_buffer);
        use SupplyResponseStatus::*;
        let num_frames_consumed = frame_offset + inner_response.num_frames_consumed;
        // rt_debug!("pre-buffer: fallback");
        SupplyResponse {
            num_frames_consumed,
            status: match inner_response.status {
                PleaseContinue => PleaseContinue,
                ReachedEnd { .. } => ReachedEnd {
                    num_frames_written: num_frames_consumed,
                },
            },
        }
    }

    /// A successful result means that the complete request could be satisfied using pre-buffered
    /// blocks. An error means that some frames are left to be filled. It contains the frame offset.
    fn use_pre_buffers_as_far_as_possible(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
        initial_frame_offset: usize,
    ) -> Result<SupplyResponse, StepFailure> {
        let mut frame_offset = initial_frame_offset;
        loop {
            let outcome = self.step(request, dest_buffer, frame_offset)?;
            use StepSuccess::*;
            match outcome {
                Finished(response) => {
                    return Ok(response);
                }
                ContinueWithFrameOffset(new_offset) => {
                    frame_offset = new_offset;
                }
            }
        }
    }

    /// A successful result means that a matching pre-buffered block was found and fulfilled at
    /// least a part of the request.
    fn step(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
        frame_offset: usize,
    ) -> Result<StepSuccess, StepFailure> {
        let requested_channel_count = dest_buffer.channel_count();
        let mut remaining_dest_buffer = dest_buffer.slice_mut(frame_offset..);
        let criteria = MatchCriteria {
            start_frame: request.start_frame + frame_offset as isize,
            channel_count: requested_channel_count,
            frame_rate: request.dest_sample_rate,
            desired_frame_count: remaining_dest_buffer.frame_count(),
        };
        // Try to fill at least the beginning of the remaining portion of the requested material
        // with the next available pre-buffered block.
        let block = self.consumer.peek().map_err(|_| StepFailure {
            frame_offset,
            non_matching_block_count: 0,
        })?;
        let apply_result = block.try_apply_to(&mut remaining_dest_buffer, &criteria);
        // Evaluate peek result
        match apply_result {
            Ok(apply_outcome) => {
                // Consume block if exhausted.
                if apply_outcome.block_exhausted {
                    let block = self.consumer.pop().unwrap();
                    self.request_sender.recycle_block(block);
                }
                let success = process_pre_buffered_response(
                    dest_buffer,
                    apply_outcome.partial_response,
                    frame_offset,
                );
                Ok(success)
            }
            Err(_) => {
                // We just left the super happy path.
                // Let's check not just the next available block but all available blocks.
                // Don't consume immediately! We might run into the situation that no block matches
                // and consuming immediately would make the producer produce further probably
                // unnecessary blocks. We defer consumption until we know what's going on.
                let slots = self.consumer.slots();
                let read_chunk = self.consumer.read_chunk(slots).unwrap();
                let (slice_one, slice_two) = read_chunk.as_slices();
                let outcome = slice_one
                    .iter()
                    .chain(slice_two.iter())
                    .enumerate()
                    // We already checked the first block.
                    // Important to have the skip after the enumerate because then i starts at 1.
                    .skip(1)
                    .find_map(|(i, b)| {
                        let outcome = b.try_apply_to(&mut remaining_dest_buffer, &criteria).ok()?;
                        Some((i, outcome))
                    });
                match outcome {
                    None => {
                        // No block matched. Sad path.
                        debug!("No block matched.");
                        let failure = StepFailure {
                            frame_offset,
                            non_matching_block_count: slots,
                        };
                        return Err(failure);
                    }
                    Some((i, outcome)) => {
                        // Found a matching block and applied it!
                        debug!(
                            "Found matching block after searching {} of {} additional slot(s).",
                            i,
                            slots - 1
                        );
                        // At first recycle blocks.
                        let num_blocks_to_be_consumed = if outcome.block_exhausted {
                            // Including the matched one
                            i + 1
                        } else {
                            // Not including the matched one
                            i
                        };
                        self.recycle_next_n_blocks(num_blocks_to_be_consumed);
                        let success = process_pre_buffered_response(
                            dest_buffer,
                            outcome.partial_response,
                            frame_offset,
                        );
                        Ok(success)
                    }
                }
            }
        }
    }
}

enum StepSuccess {
    Finished(SupplyResponse),
    ContinueWithFrameOffset(usize),
}

struct StepFailure {
    /// Position in the block until which material was filled so far.
    frame_offset: usize,
    /// If this > 0, this means that blocks were available but none of them matched.
    non_matching_block_count: usize,
}

enum SkipCountInPhaseOutcome {
    PureCountIn(SupplyResponse),
    /// Partial count-in or no count-in (in latter case the frame offset is 0).
    StartWithFrameOffset(usize),
}

struct ApplyOutcome {
    partial_response: SupplyResponse,
    /// If not exhausted, the pre-buffered block still holds future material.
    block_exhausted: bool,
}

impl<S: AudioSupplier + Clone + Send + 'static> PreBufferSourceSkill for PreBuffer<S> {
    fn pre_buffer(&mut self, args: PreBufferFillRequest) {
        self.pre_buffer_internal(args);
    }
}

impl<S: AudioSupplier + WithFrameRate + Clone + Send + 'static> AudioSupplier for PreBuffer<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        // Below logic is built upon assumption that in/out frame rates equal and
        // therefore number of consumed frames == number of written frames.
        debug_assert_eq!(
            request.dest_sample_rate,
            self.supplier.frame_rate().unwrap()
        );
        if !self.enabled {
            return self.supplier.supply_audio(request, dest_buffer);
        }
        // Return silence until frame 0 reached, if allowed.
        let initial_frame_offset = if self.skip_count_in_phase_material {
            use SkipCountInPhaseOutcome::*;
            match self.skip_count_in_phase(request.start_frame, dest_buffer) {
                PureCountIn(response) => return response,
                StartWithFrameOffset(initial_frame_offset) => initial_frame_offset,
            }
        } else {
            0
        };
        // Get the material.
        // - Happy path: The next pre-buffered blocks match and we consume them if fully exhausted.
        // - Almost happy path: There are some non-matching pre-buffered blocks in the way and we
        //   need to get rid of them until we finally reach matching blocks again that satisfy our
        //   request.
        // - Sad path: Not enough matching pre-buffered blocks are there to satisfy our request.
        //   When we realize it, we fill up the remaining material by querying the supplier.
        //   In addition, we send a new pre-buffer request to hopefully can go the happy path next
        //   time. Also, we consume all blocks because they are useless.
        match self.use_pre_buffers_as_far_as_possible(request, dest_buffer, initial_frame_offset) {
            Ok(response) => response,
            Err(step_failure) => {
                let response = self.query_supplier_for_remaining_portion(
                    request,
                    dest_buffer,
                    step_failure.frame_offset,
                );
                if step_failure.non_matching_block_count > 0 {
                    // We found non-matching blocks.
                    // First, we can assume that the pre-buffer worker somehow is somehow on the
                    // wrong track. "Recalibrate" it.
                    let fill_request = PreBufferFillRequest {
                        start_frame: calculate_next_reasonable_frame(
                            request.start_frame,
                            &response,
                        ),
                        frame_rate: request.dest_sample_rate,
                        channel_count: dest_buffer.channel_count(),
                    };
                    // TODO-high
                    // self.pre_buffer_internal(fill_request);
                    // Second, let's drain all non-matching blocks. Not useful!
                    self.recycle_next_n_blocks(step_failure.non_matching_block_count);
                }
                response
            }
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for PreBuffer<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier> MidiSupplier for PreBuffer<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        // MIDI doesn't need pre-buffering.
        self.supplier.supply_midi(request, event_list)
    }
}

impl<S: ExactFrameCount> ExactFrameCount for PreBuffer<S> {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}
impl<S: ExactDuration> ExactDuration for PreBuffer<S> {
    fn duration(&self) -> DurationInSeconds {
        self.supplier.duration()
    }
}

impl<S: WithSource> WithSource for PreBuffer<S> {
    fn source(&self) -> &OwnedPcmSource {
        self.supplier.source()
    }

    fn source_mut(&mut self) -> &mut OwnedPcmSource {
        self.supplier.source_mut()
    }
}

#[derive(Default)]
struct PreBufferWorker {
    instances: HashMap<PreBufferInstanceId, Instance, BuildHasherDefault<XxHash64>>,
    spare_buffers: Vec<OwnedAudioBuffer>,
}

impl PreBufferWorker {
    pub fn process_request(&mut self, request: PreBufferRequest) {
        use PreBufferRequest::*;
        match request {
            RegisterInstance {
                id,
                producer,
                supplier,
            } => {
                let instance = Instance {
                    producer,
                    supplier,
                    state: InstanceState::Initialized,
                };
                self.register_instance(id, instance);
            }
            Recycle(block) => {
                self.recycle(block);
            }
            KeepFillingFrom { id, args } => {
                let _ = self.keep_filling_from(id, args);
            }
        }
    }

    pub fn fill_all(&mut self) {
        let spare_buffers = &mut self.spare_buffers;
        let mut get_spare_buffer = |channel_count: usize| {
            iter::repeat_with(|| spare_buffers.pop())
                .take_while(|buffer| buffer.is_some())
                .flatten()
                .find(|buffer| buffer.to_buf().channel_count() == channel_count)
                .unwrap_or_else(|| OwnedAudioBuffer::new(channel_count, PRE_BUFFERED_BLOCK_LENGTH))
        };
        self.instances.retain(|_, instance| {
            let outcome = instance.fill(&mut get_spare_buffer);
            // Unregister instance if consumer gone.
            !matches!(outcome, Err(FillError::ConsumerGone))
        });
    }

    fn register_instance(&mut self, id: PreBufferInstanceId, instance: Instance) {
        self.instances.insert(id, instance);
    }

    fn recycle(&mut self, block: PreBufferedBlock) {
        self.spare_buffers.push(block.buffer);
    }

    fn keep_filling_from(
        &mut self,
        id: PreBufferInstanceId,
        args: PreBufferFillRequest,
    ) -> Result<(), &'static str> {
        debug!("Pre-buffer request for instance {}: {:?}", id, &args);
        let instance = self
            .instances
            .get_mut(&id)
            .ok_or("instance doesn't exist")?;
        let filling_state = FillingState {
            next_start_frame: args.start_frame,
            required_frame_rate: args.frame_rate,
            required_channel_count: args.channel_count,
        };
        instance.state = InstanceState::Filling(filling_state);
        Ok(())
    }
}

struct Instance {
    producer: Producer<PreBufferedBlock>,
    supplier: Box<dyn AudioSupplier + Send + 'static>,
    state: InstanceState,
}

impl Instance {
    pub fn fill(
        &mut self,
        mut get_spare_buffer: impl FnMut(usize) -> OwnedAudioBuffer,
    ) -> Result<(), FillError> {
        if self.producer.is_abandoned() {
            return Err(FillError::ConsumerGone);
        }
        if self.producer.is_full() {
            return Err(FillError::Full);
        }
        use InstanceState::*;
        let state = match &mut self.state {
            Initialized => return Err(FillError::NotFilling),
            Filling(s) => s,
        };
        let mut buffer = get_spare_buffer(state.required_channel_count);
        let request = SupplyAudioRequest {
            start_frame: state.next_start_frame,
            dest_sample_rate: state.required_frame_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "pre-buffer",
                note: "",
                is_realtime: false,
            },
            parent_request: None,
            general_info: &Default::default(),
        };
        let response = self
            .supplier
            .supply_audio(&request, &mut buffer.to_buf_mut());
        let block = PreBufferedBlock {
            start_frame: state.next_start_frame,
            frame_rate: state.required_frame_rate,
            buffer,
            response,
        };
        state.next_start_frame = calculate_next_reasonable_frame(state.next_start_frame, &response);
        // dbg!(&block);
        self.producer
            .push(block)
            .expect("ring buffer should not be full");
        Ok(())
    }
}

fn calculate_next_reasonable_frame(current_frame: isize, response: &SupplyResponse) -> isize {
    use SupplyResponseStatus::*;
    match response.status {
        PleaseContinue => current_frame + response.num_frames_consumed as isize,
        // Starting over pre-buffering the start is a good default because we do looping mostly.
        ReachedEnd { .. } => 0,
    }
}

enum FillError {
    NotFilling,
    ConsumerGone,
    Full,
}

enum InstanceState {
    Initialized,
    Filling(FillingState),
}

struct FillingState {
    next_start_frame: isize,
    required_frame_rate: Hz,
    required_channel_count: usize,
}

pub fn keep_processing_pre_buffer_requests(receiver: Receiver<PreBufferRequest>) {
    let mut worker = PreBufferWorker::default();
    loop {
        // At first take every incoming request serious so we can fill based on up-to-date demands.
        loop {
            match receiver.try_recv() {
                Ok(request) => {
                    worker.process_request(request);
                }
                Err(e) => {
                    use TryRecvError::*;
                    match e {
                        Empty => break,
                        Disconnected => return,
                    }
                }
            }
        }
        // Then write more audio into ring buffers.
        worker.fill_all();
        // Don't spin like crazy
        thread::sleep(Duration::from_millis(1));
    }
}

fn process_pre_buffered_response(
    dest_buffer: &mut AudioBufMut,
    step_response: SupplyResponse,
    frame_offset: usize,
) -> StepSuccess {
    use SupplyResponseStatus::*;
    let next_frame_offset = frame_offset + step_response.num_frames_consumed;
    // Finish if end of source material reached.
    if step_response.status.reached_end() {
        debug!("pre-buffer: COMPLETE end!");
        let finished_response = SupplyResponse {
            num_frames_consumed: next_frame_offset,
            status: ReachedEnd {
                num_frames_written: next_frame_offset,
            },
        };
        return StepSuccess::Finished(finished_response);
    }
    // Finish if block filled to satisfaction.
    debug_assert!(next_frame_offset <= dest_buffer.frame_count());
    if next_frame_offset == dest_buffer.frame_count() {
        // println!(
        //     "Finished with frame offset = frame count = num_frames_consumed {}",
        //     frame_offset
        // );
        // Satisfied complete block and supplier still has material.
        let finished_response = SupplyResponse {
            num_frames_consumed: dest_buffer.frame_count(),
            status: PleaseContinue,
        };
        return StepSuccess::Finished(finished_response);
    }
    StepSuccess::ContinueWithFrameOffset(next_frame_offset)
}

// Unusable. Misses start at 1x 150 bpm or so.
// const PRE_BUFFERED_BLOCK_LENGTH: usize = 128;
// const RING_BUFFER_BLOCK_COUNT: usize = 375;

// Quite stable. Misses start at 2x 960 bpm.
// Double block count doesn't help.
// const PRE_BUFFERED_BLOCK_LENGTH: usize = 2048;
// const RING_BUFFER_BLOCK_COUNT: usize = 4;

// Quite stable. No misses.
// const PRE_BUFFERED_BLOCK_LENGTH: usize = 4096;
// const RING_BUFFER_BLOCK_COUNT: usize = 4;

// Quite stable. No misses. THIS IS THE BEST!
const PRE_BUFFERED_BLOCK_LENGTH: usize = 4096;
const RING_BUFFER_BLOCK_COUNT: usize = 2;

// Quite stable. Misses start at 3x 960 bpm.
// const PRE_BUFFERED_BLOCK_LENGTH: usize = 4096;
// const RING_BUFFER_BLOCK_COUNT: usize = 1;
