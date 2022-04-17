use crate::rt::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiSupplier, PositionTranslationSkill,
    PreBufferFillRequest, PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest,
    SupplyRequestInfo, SupplyResponse, SupplyResponseStatus, WithMaterialInfo,
};
use crate::ClipEngineResult;
use core::cmp;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use derive_more::Display;
use reaper_medium::{BorrowedMidiEventList, MidiFrameOffset};
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::atomic;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use std::{iter, thread};
use twox_hash::XxHash64;

#[derive(Debug)]
pub struct PreBuffer<S, F, C> {
    id: PreBufferInstanceId,
    enabled: bool,
    state: State,
    request_sender: Sender<PreBufferRequest<S, C>>,
    supplier: S,
    options: PreBufferOptions,
    command_processor: F,
}

pub trait CommandProcessor {
    type Supplier;
    type Command;

    fn process_command(&self, command: Self::Command, supplier: &Self::Supplier);
}

#[derive(Debug)]
enum State {
    Inactive,
    Active(ActiveState),
}

impl State {
    pub fn is_active(&self) -> bool {
        matches!(self, State::Active(_))
    }
}

#[derive(Debug)]
struct ActiveState {
    consumer: Consumer<PreBufferedBlock>,
    cached_material_info: AudioMaterialInfo,
}

impl ActiveState {
    /// A successful result means that the complete request could be satisfied using pre-buffered
    /// blocks. An error means that some frames are left to be filled. It contains the frame offset.
    pub fn use_pre_buffers_as_far_as_possible<S, C>(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
        initial_frame_offset: usize,
        request_sender: &Sender<PreBufferRequest<S, C>>,
    ) -> Result<SupplyResponse, StepFailure> {
        let mut frame_offset = initial_frame_offset;
        loop {
            let outcome = self.step(request, dest_buffer, frame_offset, request_sender)?;
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
    pub fn step<S, C>(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
        frame_offset: usize,
        request_sender: &Sender<PreBufferRequest<S, C>>,
    ) -> Result<StepSuccess, StepFailure> {
        let mut remaining_dest_buffer = dest_buffer.slice_mut(frame_offset..);
        let criteria = MatchCriteria {
            start_frame: request.start_frame + frame_offset as isize,
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
                    request_sender.recycle_block(block);
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
                        Err(failure)
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
                        self.recycle_next_n_blocks(num_blocks_to_be_consumed, request_sender);
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

    pub fn pre_buffer<S, C>(
        &mut self,
        args: PreBufferFillRequest,
        instance_id: PreBufferInstanceId,
        request_sender: &Sender<PreBufferRequest<S, C>>,
    ) {
        // Not sufficiently thought about what to do if consumer wants to pre-buffer from a negative
        // start frame. Probably normalization to 0 because we know we the downbeat handler is
        // above us. Let's see.
        debug_assert!(args.start_frame >= 0);
        request_sender.keep_filling(instance_id, args);
        self.recycle_next_n_blocks(self.consumer.slots(), request_sender);
    }

    pub fn recycle_next_n_blocks<S, C>(
        &mut self,
        count: usize,
        request_sender: &Sender<PreBufferRequest<S, C>>,
    ) {
        for block in self.consumer.read_chunk(count).unwrap().into_iter() {
            request_sender.recycle_block(block);
        }
    }
}

#[derive(Debug)]
pub struct PreBufferOptions {
    /// If we know the underlying supplier doesn't deliver count-in material, we should set this to
    /// `true`. An important optimization that prevents unnecessary supplier queries.
    pub skip_count_in_phase_material: bool,
    pub cache_miss_behavior: PreBufferCacheMissBehavior,
    /// Doesn't seem to work well.
    pub recalibrate_on_cache_miss: bool,
}

/// Decides what to do if the pre-buffer doesn't contain usable data.
#[derive(Copy, Clone, Debug)]
pub enum PreBufferCacheMissBehavior {
    /// Simply outputs silence.
    ///
    /// Safest but also the most silent option ;)
    OutputSilence,
    /// Falls back to querying the supplier directly.
    ///
    /// It's risky:
    ///
    /// - Might block due to mutex contention (if the underlying supplier is a mutex)
    /// - Might block due to file system access
    QuerySupplierEvenIfContended,
    /// Falls back to querying the supplier only if it's uncontended.
    ///
    /// Still risky:
    ///
    /// - Might block due to file system access
    QuerySupplierIfUncontended,
}

trait PreBufferSender {
    type Supplier;
    type Command;

    fn register_instance(
        &self,
        id: PreBufferInstanceId,
        producer: Producer<PreBufferedBlock>,
        supplier: Self::Supplier,
    );

    fn unregister_instance(&self, id: PreBufferInstanceId);

    fn recycle_block(&self, block: PreBufferedBlock);

    fn keep_filling(&self, id: PreBufferInstanceId, args: PreBufferFillRequest);

    fn send_command(&self, id: PreBufferInstanceId, command: Self::Command);

    fn send_request(&self, request: PreBufferRequest<Self::Supplier, Self::Command>);
}

#[derive(Debug)]
pub struct PreBufferedBlock {
    start_frame: isize,
    buffer: OwnedAudioBuffer,
    response: SupplyResponse,
}

struct MatchCriteria {
    start_frame: isize,
    /// This is just a wish. We are also satisfied if the block offers less frames.
    desired_frame_count: usize,
}

#[derive(Copy, Clone, Debug)]
#[allow(clippy::enum_variant_names)]
enum MatchError {
    /// Start frame of the pre-buffered block is in the future but all of its material would
    /// belong into the requested block.
    BlockContainsOnlyRelevantMaterialButStartFrameIsInFuture,
    /// Start frame of the pre-buffered block is in the future and it contains material that
    /// doesn't belong into the requested block.
    BlockContainsFutureMaterial,
    /// All material in the pre-buffered block is in the past.
    BlockContainsOnlyPastMaterial,
}

#[derive(Debug)]
pub enum PreBufferRequest<S, C> {
    RegisterInstance {
        id: PreBufferInstanceId,
        producer: Producer<PreBufferedBlock>,
        supplier: S,
    },
    UnregisterInstance(PreBufferInstanceId),
    Recycle(PreBufferedBlock),
    KeepFillingFrom {
        id: PreBufferInstanceId,
        args: crate::rt::supplier::PreBufferFillRequest,
    },
    SendCommand(PreBufferInstanceId, C),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Display)]
pub struct PreBufferInstanceId(usize);

impl PreBufferInstanceId {
    pub fn next() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        Self(COUNTER.fetch_add(1, atomic::Ordering::SeqCst))
    }
}

impl<S, F, C> PreBuffer<S, F, C>
where
    S: AudioSupplier + Clone + Send + 'static,
    F: CommandProcessor<Supplier = S, Command = C>,
{
    /// Don't call in real-time thread.
    pub fn new(
        supplier: S,
        request_sender: Sender<PreBufferRequest<S, C>>,
        options: PreBufferOptions,
        command_processor: F,
    ) -> Self {
        Self {
            id: PreBufferInstanceId::next(),
            enabled: false,
            state: State::Inactive,
            request_sender,
            supplier,
            options,
            command_processor,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn send_command(&self, command: C) {
        if !self.enabled {
            self.command_processor
                .process_command(command, &self.supplier);
            return;
        }
        match &self.state {
            State::Inactive => {
                // When inactive, we process the command synchronously. Fast and straightforward.
                self.command_processor
                    .process_command(command, &self.supplier);
            }
            State::Active(_) => {
                // When enabled, we let a worker thread to the work because accessing the supplier
                // might take too long for doing it in a real-time thread (either because the
                // operation itself is expensive or because we might need to obtain a lock in a
                // blocking way because the worker is currently using the supplier as well).
                self.request_sender.send_command(self.id, command);
            }
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// # Errors
    ///
    /// Returns an error if the material can't or doesn't need to be buffered. In that case
    /// it just leaves the pre-buffer disabled.
    pub fn activate(&mut self) -> ClipEngineResult<()> {
        if !self.enabled {
            return Err("disabled");
        }
        if !self.state.is_active() {
            return Err("inactive");
        }
        let audio_material_info = require_audio_material_info(self.supplier.material_info()?)?;
        let (producer, consumer) = RingBuffer::new(RING_BUFFER_BLOCK_COUNT);
        self.request_sender
            .register_instance(self.id, producer, self.supplier.clone());
        let enabled_state = ActiveState {
            consumer,
            cached_material_info: audio_material_info,
        };
        self.state = State::Active(enabled_state);
        Ok(())
    }

    pub fn deactivate(&mut self) {
        if !self.enabled || !self.state.is_active() {
            return;
        }
        self.request_sender.unregister_instance(self.id);
        self.state = State::Inactive;
    }

    /// Invalidates the material info cache.
    ///
    /// This should be called whenever the underlying material info might change. However, it
    /// accesses the supplier and therefore should be used with care (especially if the supplier
    /// is a mutex).
    pub fn invalidate_material_info_cache(&mut self) -> ClipEngineResult<()> {
        if !self.enabled {
            return Err("disabled");
        }
        match &mut self.state {
            State::Inactive => Err("inactive"),
            State::Active(s) => {
                let audio_material_info =
                    require_audio_material_info(self.supplier.material_info()?)?;
                s.cached_material_info = audio_material_info;
                Ok(())
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

impl<S, F, C> PreBufferSourceSkill for PreBuffer<S, F, C>
where
    S: AudioSupplier + Clone + Send + 'static,
    F: Debug + CommandProcessor<Supplier = S, Command = C>,
    C: Debug,
{
    fn pre_buffer(&mut self, args: PreBufferFillRequest) {
        if !self.enabled {
            return;
        }
        match &mut self.state {
            State::Inactive => {}
            State::Active(s) => {
                s.pre_buffer(args, self.id, &self.request_sender);
            }
        }
    }
}

impl<S, F, C> PositionTranslationSkill for PreBuffer<S, F, C>
where
    S: PositionTranslationSkill,
    F: Debug,
    C: Debug,
{
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        self.supplier.translate_play_pos_to_source_pos(play_pos)
    }
}

impl<S, F, C> AudioSupplier for PreBuffer<S, F, C>
where
    S: AudioSupplier + Clone + Send + 'static,
    F: Debug + CommandProcessor<Supplier = S, Command = C>,
    C: Debug,
{
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        if !self.enabled {
            return self.supplier.supply_audio(request, dest_buffer);
        }
        let state = match &mut self.state {
            State::Inactive => {
                // Inactive means we may access the supplier directly.
                return self.supplier.supply_audio(request, dest_buffer);
            }
            State::Active(s) => s,
        };
        #[cfg(debug_assertions)]
        {
            request.assert_wants_source_frame_rate(state.cached_material_info.frame_rate);
        }
        let initial_frame_offset = if self.options.skip_count_in_phase_material {
            // Return silence until frame 0 reached, if allowed.
            use SkipCountInPhaseOutcome::*;
            match skip_count_in_phase(request.start_frame, dest_buffer) {
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
        match state.use_pre_buffers_as_far_as_possible(
            request,
            dest_buffer,
            initial_frame_offset,
            &self.request_sender,
        ) {
            Ok(response) => response,
            Err(step_failure) => {
                use PreBufferCacheMissBehavior::*;
                let response = match self.options.cache_miss_behavior {
                    OutputSilence => {
                        let mut remaining_dest_buffer =
                            dest_buffer.slice_mut(step_failure.frame_offset..);
                        remaining_dest_buffer.clear();
                        SupplyResponse::please_continue(
                            step_failure.frame_offset + remaining_dest_buffer.frame_count(),
                        )
                    }
                    QuerySupplierEvenIfContended => query_supplier_for_remaining_portion(
                        request,
                        dest_buffer,
                        step_failure.frame_offset,
                        &mut self.supplier,
                    ),
                    QuerySupplierIfUncontended => unimplemented!(),
                };
                if step_failure.non_matching_block_count > 0 {
                    // We found non-matching blocks.
                    // First, we can assume that the pre-buffer worker somehow is somehow on the
                    // wrong track. "Recalibrate" it.
                    if self.options.recalibrate_on_cache_miss {
                        let fill_request = PreBufferFillRequest {
                            start_frame: calculate_next_reasonable_frame(
                                request.start_frame,
                                &response,
                            ),
                        };
                        state.pre_buffer(fill_request, self.id, &self.request_sender);
                    }
                    // Second, let's drain all non-matching blocks. Not useful!
                    state.recycle_next_n_blocks(
                        step_failure.non_matching_block_count,
                        &self.request_sender,
                    );
                }
                response
            }
        }
    }
}

impl<S: MidiSupplier, F: Debug, C: Debug> MidiSupplier for PreBuffer<S, F, C> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        // MIDI doesn't need pre-buffering.
        self.supplier.supply_midi(request, event_list)
    }

    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    ) {
        self.supplier.release_notes(frame_offset, event_list);
    }
}

impl<S: WithMaterialInfo, F, C> WithMaterialInfo for PreBuffer<S, F, C> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        if !self.enabled {
            return self.supplier.material_info();
        }
        match &self.state {
            State::Inactive => self.supplier.material_info(),
            State::Active(s) => Ok(MaterialInfo::Audio(s.cached_material_info.clone())),
        }
    }
}

struct PreBufferWorker<S, F, C> {
    instances: HashMap<PreBufferInstanceId, Instance<S>, BuildHasherDefault<XxHash64>>,
    spare_buffer_chunks: Vec<Vec<f64>>,
    command_processor: F,
    phantom: PhantomData<C>,
}

impl<S, F, C> PreBufferWorker<S, F, C>
where
    S: AudioSupplier + WithMaterialInfo,
    F: CommandProcessor<Supplier = S, Command = C>,
{
    pub fn process_request(&mut self, request: PreBufferRequest<S, C>) {
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
            UnregisterInstance(id) => {
                self.unregister_instance(id);
            }
            Recycle(block) => {
                self.recycle(block);
            }
            KeepFillingFrom { id, args } => {
                let _ = self.keep_filling_from(id, args);
            }
            SendCommand(id, command) => {
                let instance = match self.instances.get(&id) {
                    None => return,
                    Some(i) => i,
                };
                self.command_processor
                    .process_command(command, &instance.supplier);
            }
        }
    }

    pub fn fill_all(&mut self) {
        let spare_buffer_chunks = &mut self.spare_buffer_chunks;
        let mut get_spare_buffer = |channel_count: usize| {
            iter::repeat_with(|| spare_buffer_chunks.pop())
                .take_while(|buffer| buffer.is_some())
                .flatten()
                .find_map(|chunk| {
                    OwnedAudioBuffer::try_recycle(chunk, channel_count, PRE_BUFFERED_BLOCK_LENGTH)
                        .ok()
                })
                .unwrap_or_else(|| OwnedAudioBuffer::new(channel_count, PRE_BUFFERED_BLOCK_LENGTH))
        };
        self.instances.retain(|_, instance| {
            let outcome = instance.fill(&mut get_spare_buffer);
            // Unregister instance if consumer gone.
            !matches!(outcome, Err(FillError::ConsumerGone))
        });
    }

    fn register_instance(&mut self, id: PreBufferInstanceId, instance: Instance<S>) {
        self.instances.insert(id, instance);
    }

    fn unregister_instance(&mut self, id: PreBufferInstanceId) {
        self.instances.remove(&id);
    }

    fn recycle(&mut self, block: PreBufferedBlock) {
        self.spare_buffer_chunks.push(block.buffer.into_inner());
    }

    fn keep_filling_from(
        &mut self,
        id: PreBufferInstanceId,
        args: PreBufferFillRequest,
    ) -> ClipEngineResult<()> {
        debug!("Pre-buffer request for instance {}: {:?}", id, &args);
        let instance = self
            .instances
            .get_mut(&id)
            .ok_or("instance doesn't exist")?;
        let filling_state = FillingState {
            next_start_frame: args.start_frame,
        };
        instance.state = InstanceState::Filling(filling_state);
        Ok(())
    }
}

struct Instance<S> {
    producer: Producer<PreBufferedBlock>,
    supplier: S,
    state: InstanceState,
}

impl<S: AudioSupplier + WithMaterialInfo> Instance<S> {
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
        let material_info = self
            .supplier
            .material_info()
            .map_err(|_| FillError::MaterialUnavailable)?;
        let source_channel_count = material_info.channel_count();
        let mut buffer = get_spare_buffer(source_channel_count);
        let request = SupplyAudioRequest {
            start_frame: state.next_start_frame,
            dest_sample_rate: None,
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
        // Starting over pre-buffering the start is a good default.
        ReachedEnd { .. } => 0,
    }
}

enum FillError {
    NotFilling,
    ConsumerGone,
    Full,
    MaterialUnavailable,
}

enum InstanceState {
    Initialized,
    Filling(FillingState),
}

struct FillingState {
    next_start_frame: isize,
}

pub fn keep_processing_pre_buffer_requests<S, C>(
    receiver: Receiver<PreBufferRequest<S, C>>,
    command_processor: impl CommandProcessor<Supplier = S, Command = C>,
) where
    S: AudioSupplier,
{
    let mut worker = PreBufferWorker {
        instances: Default::default(),
        spare_buffer_chunks: vec![],
        command_processor,
        phantom: PhantomData,
    };
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

impl<S, C> PreBufferSender for Sender<PreBufferRequest<S, C>> {
    type Supplier = S;
    type Command = C;

    fn register_instance(
        &self,
        id: PreBufferInstanceId,
        producer: Producer<PreBufferedBlock>,
        supplier: Self::Supplier,
    ) {
        let request = PreBufferRequest::RegisterInstance {
            id,
            producer,
            supplier,
        };
        self.send_request(request);
    }

    fn unregister_instance(&self, id: PreBufferInstanceId) {
        let request = PreBufferRequest::UnregisterInstance(id);
        self.send_request(request);
    }

    fn recycle_block(&self, block: PreBufferedBlock) {
        let request = PreBufferRequest::Recycle(block);
        self.send_request(request);
    }

    fn keep_filling(&self, id: PreBufferInstanceId, args: PreBufferFillRequest) {
        let request = PreBufferRequest::KeepFillingFrom { id, args };
        self.send_request(request);
    }

    fn send_command(&self, id: PreBufferInstanceId, command: Self::Command) {
        self.send_request(PreBufferRequest::SendCommand(id, command));
    }

    fn send_request(&self, request: PreBufferRequest<S, C>) {
        self.try_send(request).unwrap();
    }
}
impl PreBufferedBlock {
    fn try_apply_to(
        &self,
        remaining_dest_buffer: &mut AudioBufMut,
        criteria: &MatchCriteria,
    ) -> Result<ApplyOutcome, MatchError> {
        let range = self.matches(criteria)?;
        // Pre-buffered block available and matches.
        Ok(self.copy_range_to(remaining_dest_buffer, range))
    }

    /// Checks whether this block contains at least the given start position and hopefully more.
    ///
    /// It returns the range of this block which can be used to fill the
    fn matches(&self, criteria: &MatchCriteria) -> Result<Range<usize>, MatchError> {
        let self_buffer = self.buffer.to_buf();
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

fn require_audio_material_info(material_info: MaterialInfo) -> ClipEngineResult<AudioMaterialInfo> {
    match material_info {
        MaterialInfo::Audio(i) => Ok(i),
        MaterialInfo::Midi(_) => {
            Err("supplier provides MIDI material which doesn't need to be pre-buffered")
        }
    }
}

/// This is an optimization we *can* (and should) apply only because we know we sit right
/// above the source, which by definition doesn't have any material in the count-in phase.
fn skip_count_in_phase(
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
fn query_supplier_for_remaining_portion<S: AudioSupplier>(
    request: &SupplyAudioRequest,
    dest_buffer: &mut AudioBufMut,
    frame_offset: usize,
    supplier: &mut S,
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
    let inner_response = supplier.supply_audio(&inner_request, &mut remaining_dest_buffer);
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
