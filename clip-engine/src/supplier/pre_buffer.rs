use crate::buffer::{AudioBuf, AudioBufMut, OwnedAudioBuffer};
use crate::source_util::pcm_source_is_midi;
use crate::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::supplier::{
    AudioSupplier, ExactFrameCount, MidiSupplier, SupplyAudioRequest, SupplyMidiRequest,
    SupplyResponse, WithFrameRate,
};
use crate::{
    get_source_frame_rate, ExactDuration, SupplyRequestInfo, SupplyResponseStatus, WithSource,
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
use std::iter;
use std::sync::atomic;
use std::sync::atomic::AtomicUsize;
use twox_hash::XxHash64;

#[derive(Debug)]
pub struct PreBuffer<S> {
    id: PreBufferInstanceId,
    enabled: bool,
    request_sender: Sender<PreBufferRequest>,
    supplier: S,
    consumer: Consumer<PreBufferedBlock>,
    last_args: Option<PreBufferFillArgs>,
}

#[derive(Debug)]
pub struct PreBufferedBlock {
    start_frame: isize,
    buffer: OwnedAudioBuffer,
    response: SupplyResponse,
}

impl PreBufferedBlock {
    fn new(props: &BlockProps) -> Self {
        Self {
            start_frame: 0,
            buffer: OwnedAudioBuffer::new(props.channel_count, props.frame_count),
            response: SupplyResponse::default(),
        }
    }

    fn has_props(&self, props: &BlockProps) -> bool {
        let buf = self.buffer.to_buf();
        buf.frame_count() == props.frame_count && buf.channel_count() == props.channel_count
    }

    fn validate(&self, criteria: &PreBufferFillArgs) -> ValidationOutcome {
        if !self.has_props(&criteria.block_props) {
            // The block length or channel count is different. That means the block is old and can be discarded.
            // We know it's old because the preview register always queries using the current block
            // length and channel count.
            return ValidationOutcome::Discard;
        }
        use Ordering::*;
        match self.start_frame.cmp(&criteria.start_frame) {
            Less => ValidationOutcome::Discard,
            Equal => ValidationOutcome::Matches,
            Greater => ValidationOutcome::Keep,
        }
    }
}

#[derive(Debug)]
enum ValidationOutcome {
    /// Block matches perfectly.
    Matches,
    /// Block can be discarded because it's not relevant anymore.
    Discard,
    /// Block might be relevant for subsequent requests.
    Keep,
}

impl ValidationOutcome {
    fn matches(&self) -> bool {
        matches!(self, Self::Matches)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct PreBufferFillArgs {
    pub start_frame: isize,
    pub frame_rate: Hz,
    pub block_props: BlockProps,
}

#[derive(Clone, PartialEq, Debug)]
pub struct BlockProps {
    pub frame_count: usize,
    pub channel_count: usize,
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
        args: PreBufferFillArgs,
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
        // At this point, we probably mostly have requests for blocks of 128 frames each
        // (initiated by the time stretcher). So we pre-buffer 16 * 128 = 2048 frames.
        // TODO-high We don't necessarily start at zero. It can be negative and overlap zero.
        let (producer, consumer) = RingBuffer::new(16);
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
        }
    }

    /// Does its best to make sure that the next block in the ring buffer fulfills the given
    /// criteria.
    pub fn ensure_next_block_is_pre_buffered(&mut self, args: PreBufferFillArgs) {
        if let Some(last_args) = self.last_args.as_ref() {
            if &args == last_args {
                return;
            }
        }
        self.keep_filling_from(args);
    }

    fn keep_filling_from(&mut self, args: PreBufferFillArgs) {
        self.last_args = Some(args.clone());
        let request = PreBufferRequest::KeepFillingFrom { id: self.id, args };
        self.request_sender.try_send(request).unwrap();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn recycle(&self, block: PreBufferedBlock) {
        let request = PreBufferRequest::Recycle(block);
        self.request_sender.try_send(request).unwrap();
    }
}

impl<S: AudioSupplier + Clone + Send + 'static> AudioSupplier for PreBuffer<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        if !self.enabled {
            return self.supplier.supply_audio(request, dest_buffer);
        }
        let criteria = PreBufferFillArgs {
            start_frame: request.start_frame,
            frame_rate: request.dest_sample_rate,
            block_props: BlockProps {
                frame_count: dest_buffer.frame_count(),
                channel_count: dest_buffer.channel_count(),
            },
        };
        let matching_block = loop {
            // At first validate the next available block.
            let outcome = match self.consumer.peek() {
                Ok(b) => b.validate(&criteria),
                Err(_) => return self.supplier.supply_audio(request, dest_buffer),
            };
            // Use, discard or keep the block.
            use ValidationOutcome::*;
            dbg!(&outcome);
            match outcome {
                Matches => break self.consumer.pop().unwrap(),
                Discard => {
                    // TODO-high Mmh, but this makes the other thread produce more blocks.
                    let block = self.consumer.pop().unwrap();
                    self.recycle(block);
                }
                Keep => return self.supplier.supply_audio(request, dest_buffer),
            }
        };
        matching_block.buffer.to_buf().copy_to(dest_buffer);
        let response = matching_block.response;
        self.recycle(matching_block);
        response
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
    spare_blocks: Vec<PreBufferedBlock>,
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
        let spare_blocks = &mut self.spare_blocks;
        let mut get_spare_block = |props: &BlockProps| {
            iter::repeat_with(|| spare_blocks.pop())
                .take_while(|block| block.is_some())
                .flatten()
                .find(|block| block.has_props(props))
                .unwrap_or_else(|| PreBufferedBlock::new(props))
        };
        self.instances.retain(|_, instance| {
            let outcome = instance.fill(&mut get_spare_block);
            // Unregister instance if consumer gone.
            !matches!(outcome, Err(FillError::ConsumerGone))
        });
    }

    fn register_instance(&mut self, id: PreBufferInstanceId, instance: Instance) {
        self.instances.insert(id, instance);
    }

    fn recycle(&mut self, block: PreBufferedBlock) {
        self.spare_blocks.push(block);
    }

    fn keep_filling_from(
        &mut self,
        id: PreBufferInstanceId,
        args: PreBufferFillArgs,
    ) -> Result<(), &'static str> {
        let instance = self
            .instances
            .get_mut(&id)
            .ok_or("instance doesn't exist")?;
        let filling_state = FillingState {
            next_start_frame: args.start_frame,
            required_frame_rate: args.frame_rate,
            required_block_props: args.block_props,
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
        mut get_spare_block: impl FnMut(&BlockProps) -> PreBufferedBlock,
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
        let mut block = get_spare_block(&state.required_block_props);
        block.start_frame = state.next_start_frame;
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
            .supply_audio(&request, &mut block.buffer.to_buf_mut());
        use SupplyResponseStatus::*;
        state.next_start_frame = match response.status {
            PleaseContinue => state.next_start_frame + response.num_frames_consumed as isize,
            ReachedEnd { .. } => 0,
        };
        block.response = response;
        dbg!(&block);
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
    required_block_props: BlockProps,
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
    }
}
