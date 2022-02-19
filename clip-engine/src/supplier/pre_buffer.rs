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
    last_args: Option<PreBufferFillRequest>,
    /// If we know the underlying supplier doesn't deliver count-in material, we should set this
    /// to `true`. An important optimization that saves supplier queries.
    skip_count_in_phase_material: bool,
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
        if num_available_frames < 0 {
            return Err(MatchError::BlockContainsOnlyPastMaterial);
        }
        let num_available_frames = num_available_frames as usize;
        // At this point we know we have usable material.
        let length = cmp::min(criteria.desired_frame_count, num_available_frames);
        Ok(offset..offset + length)
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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
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
            last_args: None,
            // We know we sit right above the source and this one can't deliver material in the
            // count-in phase. This is good for performance, especially when crossing the
            // zero boundary.
            skip_count_in_phase_material: true,
        }
    }

    fn keep_filling_from(&mut self, args: PreBufferFillRequest) {
        // Not sufficiently thought about what to do if consumer wants to pre-buffer from a negative
        // start frame. Probably normalization to 0 because we know we sit on the source. Let's see.
        debug_assert!(args.start_frame >= 0);
        self.last_args = Some(args.clone());
        dbg!(&args);
        let request = PreBufferRequest::KeepFillingFrom { id: self.id, args };
        self.request_sender.try_send(request).unwrap();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn recycle_next_pre_buffered_block(&mut self) {
        let block = self.consumer.pop().unwrap();
        let request = PreBufferRequest::Recycle(block);
        self.request_sender.try_send(request).unwrap();
    }
}

impl<S: AudioSupplier + Clone + Send + 'static> PreBufferSourceSkill for PreBuffer<S> {
    fn pre_buffer_next_source_block(&mut self, args: PreBufferFillRequest) {
        // TODO-high Problem: This won't reset to zero if the pre-buffer worker already advanced
        //  and now should go back!
        if let Some(last_args) = self.last_args.as_ref() {
            if &args == last_args {
                return;
            }
        }
        self.keep_filling_from(args);
        // TODO-high This would only make sure the block is actually available for the next
        //  supply_audio call if the supply_audio call consumes all non-matching block up until
        //  that one - which it doesn't at the moment. We don't do it because the pre-buffered
        //  blocks might contain material that's relevant for the next, 2nd-next etc. request.
        //  But let's see how that turns out in practice. We could use slots() and read_chunk()
        //  in supply_audio() to look ahead (further than just the first available block) and check
        //  if one of the next ones contains the desired material.
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
        let requested_channel_count = dest_buffer.channel_count();
        let mut frame_offset = 0usize;
        // Return silence until frame 0 reached, if allowed.
        if self.skip_count_in_phase_material && request.start_frame < 0 {
            // This is an optimization we *can* (and should) apply only because we know we sit right
            // above the source, which by definition doesn't have any material in the count-in phase.
            let num_frames_until_zero = -request.start_frame as usize;
            let num_frames_to_be_silenced =
                cmp::min(num_frames_until_zero, dest_buffer.frame_count());
            dest_buffer.slice_mut(0..num_frames_to_be_silenced).clear();
            if num_frames_to_be_silenced == dest_buffer.frame_count() {
                // Pure count-in.
                return SupplyResponse::please_continue(num_frames_to_be_silenced);
            }
            frame_offset = num_frames_to_be_silenced;
        }
        // Get material
        loop {
            let mut remaining_dest_buffer = dest_buffer.slice_mut(frame_offset..);
            let criteria = MatchCriteria {
                start_frame: request.start_frame + frame_offset as isize,
                channel_count: requested_channel_count,
                frame_rate: request.dest_sample_rate,
                desired_frame_count: remaining_dest_buffer.frame_count(),
            };
            // Try to fill at least the beginning of the remaining portion of the requested material
            // with a pre-buffered block.
            enum Outcome {
                /// Pre-buffered block matched (completely or partially).
                Hit {
                    partial_response: SupplyResponse,
                    /// If not exhausted, the pre-buffered block still holds future material.
                    block_exhausted: bool,
                },
                /// No pre-buffered block found or match error.
                Miss { match_error: Option<MatchError> },
            }
            // TODO-high This is not greedy enough. We should fast-forward, popping all non-matching
            //  blocks. Otherwise we always fall back to supplier query although we could easily
            //  get what we wish for.
            let outcome = match self.consumer.peek() {
                Ok(block) => {
                    let pre_buf = block.buffer.to_buf();
                    match block.matches(&criteria) {
                        // Pre-buffered block available and matches.
                        Ok(range) => {
                            debug_assert!(range.end <= pre_buf.frame_count());
                            // Check if we reached end.
                            let (clamped_range_end, reached_end) = match block.response.status {
                                PleaseContinue => {
                                    // Pre-buffered block doesn't contain end.
                                    (range.end, false)
                                }
                                SupplyResponseStatus::ReachedEnd { num_frames_written } => {
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
                            let mut sliced_dest_buffer =
                                remaining_dest_buffer.slice_mut(0..range.len());
                            sliced_src_buffer.copy_to(&mut sliced_dest_buffer);
                            // Express outcome
                            Outcome::Hit {
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
                        // Pre-buffered block available but doesn't match.
                        Err(e) => Outcome::Miss {
                            match_error: Some(e),
                        },
                    }
                }
                // No pre-buffered block available.
                Err(_) => Outcome::Miss { match_error: None },
            };
            // Evaluate outcome
            let partial_pre_buffer_response = match outcome {
                // It's a match!
                Outcome::Hit {
                    partial_response: partial_response,
                    block_exhausted,
                } => {
                    // Consume block if exhausted.
                    if block_exhausted {
                        self.recycle_next_pre_buffered_block();
                    }
                    partial_response
                }
                // No matching pre-buffered block available.
                Outcome::Miss { mut match_error } => {
                    // Keep consuming this block if not relevant and all non-relevant subsequent blocks.
                    while match_error
                        .map(|e| e.should_consume_block())
                        .unwrap_or(false)
                    {
                        dbg!(match_error);
                        self.recycle_next_pre_buffered_block();
                        match_error = match self.consumer.peek() {
                            Ok(block) => block.matches(&criteria).err(),
                            Err(_) => None,
                        };
                    }
                    // Query supplier for the complete remaining portion and return early.
                    let inner_request = SupplyAudioRequest {
                        start_frame: criteria.start_frame,
                        dest_sample_rate: request.dest_sample_rate,
                        info: SupplyRequestInfo {
                            audio_block_frame_offset: request.info.audio_block_frame_offset
                                + frame_offset,
                            requester: "pre-buffer",
                            note: "",
                            is_realtime: false,
                        },
                        parent_request: Some(request),
                        general_info: request.general_info,
                    };
                    let inner_response = self
                        .supplier
                        .supply_audio(&inner_request, &mut remaining_dest_buffer);
                    use SupplyResponseStatus::*;
                    let num_frames_consumed = frame_offset + inner_response.num_frames_consumed;
                    println!("pre-buffer: fallback");
                    return SupplyResponse {
                        num_frames_consumed,
                        status: match inner_response.status {
                            PleaseContinue => PleaseContinue,
                            ReachedEnd { .. } => ReachedEnd {
                                num_frames_written: num_frames_consumed,
                            },
                        },
                    };
                }
            };
            // Advance offset
            frame_offset += partial_pre_buffer_response.num_frames_consumed;
            // Return early if end of material reached.
            use SupplyResponseStatus::*;
            if partial_pre_buffer_response.status.reached_end() {
                println!("pre-buffer: COMPLETE end!");
                return SupplyResponse {
                    num_frames_consumed: frame_offset,
                    status: ReachedEnd {
                        num_frames_written: frame_offset,
                    },
                };
            }
            // Return if block filled to satisfaction.
            debug_assert!(frame_offset <= dest_buffer.frame_count());
            if frame_offset == dest_buffer.frame_count() {
                // println!(
                //     "Finished with frame offset = frame count = num_frames_consumed {}",
                //     frame_offset
                // );
                // Satisfied complete block and supplier still has material.
                return SupplyResponse {
                    num_frames_consumed: dest_buffer.frame_count(),
                    status: PleaseContinue,
                };
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
        use SupplyResponseStatus::*;
        state.next_start_frame = match response.status {
            PleaseContinue => state.next_start_frame + response.num_frames_consumed as isize,
            // Starting over pre-buffering the start is a good default because we do looping mostly.
            ReachedEnd { .. } => 0,
        };
        // dbg!(&block);
        self.producer
            .push(block)
            .expect("ring buffer should not be full");
        Ok(())
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

const PRE_BUFFERED_BLOCK_LENGTH: usize = 128;
const RING_BUFFER_BLOCK_COUNT: usize = 375;
