use crate::domain::{
    classify_midi_message, AudioBlockProps, ControlEvent, ControlEventTimestamp,
    DisplayAsPrettyHex, IncomingMidiMessage, InstanceId, MidiControlInput, MidiEvent,
    MidiMessageClassification, MidiScanResult, MidiScanner, MidiTransformationContainer,
    RealTimeProcessor, SharedRealTimeInstance, UnitId, GLOBAL_AUDIO_STATE,
};
use base::byte_pattern::{BytePattern, PatternByte};
use base::metrics_util::{measure_time, record_duration};
use base::non_blocking_lock;
use helgoboss_learn::{MidiSourceValue, RawMidiEvent, RawMidiEvents};
use helgoboss_midi::{DataEntryByteOrder, RawShortMessage, ShortMessage, ShortMessageType};
use helgobox_allocator::*;
use reaper_common_types::DurationInSeconds;
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::{
    MidiInputDeviceId, MidiOutputDeviceId, OnAudioBuffer, OnAudioBufferArgs, SendMidiTime,
    MIDI_INPUT_FRAME_RATE,
};
use smallvec::SmallVec;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tinyvec::ArrayVec;

const AUDIO_HOOK_TASK_BULK_SIZE: usize = 1;
const FEEDBACK_TASK_BULK_SIZE: usize = 1000;

/// This needs to be thread-safe because if "Allow live FX multiprocessing" is active in the REAPER
/// preferences, the VST processing is executed in another thread than the audio hook!
pub type SharedRealTimeProcessor = Arc<Mutex<RealTimeProcessor>>;

pub type MidiCaptureSender = async_channel::Sender<MidiScanResult>;

#[derive(Debug)]
pub struct RequestMidiDeviceIdentityCommand {
    pub output_device_id: MidiOutputDeviceId,
    pub input_device_id: Option<MidiInputDeviceId>,
    pub sender: async_channel::Sender<RequestMidiDeviceIdentityReply>,
}

#[derive(Debug)]
struct MidiDeviceInquiryTask {
    command: RequestMidiDeviceIdentityCommand,
    inquiry_sent_at: Instant,
}

#[derive(Clone, Debug)]
pub struct RequestMidiDeviceIdentityReply {
    pub input_device_id: MidiInputDeviceId,
    pub device_inquiry_reply: MidiDeviceInquiryReply,
}

#[derive(Clone, Debug)]
pub struct MidiDeviceInquiryReply {
    pub message: ArrayVec<[u8; RawMidiEvent::MAX_LENGTH]>,
}

impl Display for MidiDeviceInquiryReply {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        DisplayAsPrettyHex(self.message.as_slice()).fmt(f)
    }
}

// This kind of tasks is always processed, even after a rebirth when multiple processor syncs etc.
// have already accumulated. Because at the moment there's no way to request a full resync of all
// real-time processors from the control surface. In practice there's no danger that too many of
// those infrequent tasks accumulate so it's not an issue. Therefore the convention for now is to
// also send them when audio is not running.
#[derive(Debug)]
pub enum NormalAudioHookTask {
    AddRealTimeInstance(InstanceId, SharedRealTimeInstance),
    RemoveRealTimeInstance(InstanceId),
    /// First parameter is the ID.
    //
    // Having the ID saves us from unnecessarily blocking the audio thread by looking into the
    // processor.
    AddRealTimeProcessor(UnitId, SharedRealTimeProcessor),
    RemoveRealTimeProcessor(UnitId),
    StartCapturingMidi(MidiCaptureSender),
    StopCapturingMidi,
    /// Instructs the audio hook to send a MIDI device inquiry to the given output device.
    ///
    /// Gives up after about one second if no response received (by dropping the sender).
    ///
    /// Gives up immediately if the output device or optional input device is not open.
    RequestMidiDeviceIdentity(RequestMidiDeviceIdentityCommand),
    #[cfg(feature = "playtime")]
    PlaytimeClipEngineCommand(playtime_clip_engine::rt::audio_hook::PlaytimeAudioHookCommand),
}

/// A global feedback task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackAudioHookTask {
    MidiDeviceFeedback(
        MidiOutputDeviceId,
        MidiSourceValue<'static, RawShortMessage>,
    ),
    SendMidi(MidiOutputDeviceId, RawMidiEvents),
}

pub fn send_midi_device_feedback(
    dev_id: MidiOutputDeviceId,
    value: MidiSourceValue<RawShortMessage>,
) {
    if let Some(events) = value.to_raw() {
        MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
            if let Some(mo) = mo {
                for event in events {
                    mo.send_msg(event, SendMidiTime::Instantly);
                }
            }
        });
    } else {
        let shorts = value.to_short_messages(DataEntryByteOrder::MsbFirst);
        if shorts[0].is_none() {
            return;
        }
        MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
            if let Some(mo) = mo {
                for short in shorts.iter().flatten() {
                    mo.send(*short, SendMidiTime::Instantly);
                }
            }
        });
    }
}

#[derive(Debug)]
pub struct RealearnAudioHook {
    state: AudioHookState,
    midi_device_inquiry_task: Option<MidiDeviceInquiryTask>,
    real_time_instances: SmallVec<[(InstanceId, SharedRealTimeInstance); 256]>,
    real_time_processors: SmallVec<[(UnitId, SharedRealTimeProcessor); 256]>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    time_of_last_run: Option<Instant>,
    initialized: bool,
    midi_transformation_container: MidiTransformationContainer,
    #[cfg(feature = "playtime")]
    clip_engine_audio_hook: playtime_clip_engine::rt::audio_hook::PlaytimeAudioHook,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AudioHookState {
    Normal,
    // This is not the instance-specific learning but the global one.
    LearningSource {
        sender: MidiCaptureSender,
        midi_scanner: MidiScanner,
    },
}

impl RealearnAudioHook {
    pub fn new(
        normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
        feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    ) -> RealearnAudioHook {
        Self {
            state: AudioHookState::Normal,
            midi_device_inquiry_task: None,
            real_time_instances: Default::default(),
            real_time_processors: Default::default(),
            normal_task_receiver,
            feedback_task_receiver,
            time_of_last_run: None,
            initialized: false,
            midi_transformation_container: MidiTransformationContainer::new(),
            #[cfg(feature = "playtime")]
            clip_engine_audio_hook: playtime_clip_engine::rt::audio_hook::PlaytimeAudioHook::new(),
        }
    }

    /// This should be called only once in the audio hardware thread, before everything else.
    ///
    /// It does some per-thread allocation.
    fn init_from_rt_thread(&mut self) {
        // We have code, e.g. triggered by crossbeam_channel that requests the ID of the
        // current thread. This operation needs an allocation at the first time it's executed
        // on a specific thread. Let's do it here, globally exactly once. Then we can
        // use assert_no_alloc() to detect real regular allocation issues.
        // Please note that this doesn't have an effect if
        // "Audio => Buffering => Allow live FX multiprocessing" is enabled in the REAPER prefs.
        // Because then worker threads will drive ReaLearn plug-in and clips. That's not an
        // issue for actual usage because the allocation is done only once per worker
        // thread, right at the beginning. It's only a problem for testing with
        // assert_no_alloc(). We introduced a similar thing in ColumnSource get_samples.
        let thread_id = std::thread::current().id();
        // The tracing library also does some allocation per thread (independent from the
        // allocations that a subscriber does anyway).
        tracing::info!(
            "Initializing real-time logging from preview register (thread {:?})",
            thread_id
        );
        #[cfg(feature = "playtime")]
        {
            self.clip_engine_audio_hook.init_from_rt_thread();
        }
    }

    fn on_pre(&mut self, args: OnAudioBufferArgs) {
        let current_time = Instant::now();
        let time_of_last_run = self.time_of_last_run.replace(current_time);
        // Increment counter
        let block_props = AudioBlockProps::from_on_audio_buffer_args(&args);
        let block_count = GLOBAL_AUDIO_STATE.advance(block_props);
        let sample_count = block_count * args.len as u64;
        // Call ReaLearn real-time processors (= process MIDI messages coming in from hardware devices).
        // We do this here already, *before* pre-polling recording and advancing Playtime's tempo buffer (done in `on_pre_poll_1`)!
        // Reason: When recording a new clip with tempo detection (= recording in silence mode), it's ideal
        // if pressing a stop button on the controller *instantly* stops recording, detects the new tempo,
        // applies it to the current block and starts playing the clip. Instantly = at the start of *this* block,
        // without waiting until the next block. This is only possible if at the very beginning of the
        // block it's already known that the stop button was pressed, before the block tempo props are determined.
        //
        // Playtime steps:
        //
        // 1. Process MIDI messages coming in from hardware devices
        // 2. For each record-clip task:
        // 2.1 Write incoming data to recording clip
        // 2.2 Commit recording if necessary (if tempo detection enabled, also reset timeline with new tempo and no count-in)
        //     - Maybe we can unify manual stop and scheduled stop this way
        //     - One way is to process RtColumn commands generally in the audio hook.
        //       - Pro: No need to make "stop recording" a special command. All commands will be executed before preview registers,
        //         that is, before other columns have been played.
        //       - Con: I had the idea of a future refactoring: To do resolving/scheduling eagerly when processing
        //         the command instead of later when processing the slots. Since block tempo props are by definition
        //         not 100% decided yet when processing commands from the audio hook in this early stage, this would
        //         become impossible.
        //
        // 3. Advance tempo buffer
        // 4. Play clip from start (process preview registers, which REAPER does after executing the pre-audio-hook)
        let might_be_rebirth = {
            if let Some(time) = time_of_last_run {
                current_time.duration_since(time) > Duration::from_secs(1)
            } else {
                false
            }
        };
        self.call_real_time_processors(block_props, sample_count, might_be_rebirth);
        // Process ReaLearn feedback commands
        self.process_feedback_commands();
        // Process incoming commands, including Playtime commands
        self.process_normal_commands(block_props);
        // Pre-poll Playtime
        #[cfg(feature = "playtime")]
        {
            self.clip_engine_audio_hook
                .on_pre_poll(block_props.to_playtime(), args.reg);
        }
        // Poll real-time instances. If an instance has Playtime enabled, this also polls the real-time matrix.
        // Important to do after pre-polling the Playtime audio hook, especially for one scenario:
        // Leaving silence mode immediately with playing ignited clips: In this case, we do a timeline reset to zero.
        // The ignited clips should start immediately, exactly from zero as soon as the timeline has been reset.
        // If we called this before pre-polling Playtime audio hook, the real-time matrix would be called in the
        // next audio cycle, after the timeline has already advanced on block from zero.
        self.pre_poll_real_time_instances(block_props);
        // Process some tasks
        self.check_for_midi_device_inquiry_response();
    }

    fn on_post(&mut self, args: OnAudioBufferArgs) {
        self.post_poll_real_time_instances();
        #[cfg(not(feature = "playtime"))]
        {
            let _ = args;
        }
        #[cfg(feature = "playtime")]
        {
            let block_props = AudioBlockProps::from_on_audio_buffer_args(&args);
            self.clip_engine_audio_hook
                .on_post(block_props.to_playtime(), args.reg);
        }
        // Record some metrics
        if let Some(time_of_last_run) = self.time_of_last_run {
            record_duration("helgobox.rt.audio_hook.total", time_of_last_run.elapsed());
        }
    }

    fn process_feedback_commands(&mut self) {
        // Process global direct device feedback (since v2.8.0-pre6) - in order to
        // have deterministic feedback ordering, which is important for multi-instance
        // orchestration.
        for task in self
            .feedback_task_receiver
            .try_iter()
            .take(FEEDBACK_TASK_BULK_SIZE)
        {
            use FeedbackAudioHookTask::*;
            match task {
                MidiDeviceFeedback(dev_id, value) => {
                    send_midi_device_feedback(dev_id, value);
                }
                SendMidi(dev_id, raw_midi_events) => {
                    MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                        if let Some(mo) = mo {
                            for event in &raw_midi_events {
                                mo.send_msg(event, SendMidiTime::Instantly);
                            }
                        }
                    });
                }
            }
        }
    }

    fn pre_poll_real_time_instances(&self, block_props: AudioBlockProps) {
        for (_, i) in self.real_time_instances.iter() {
            non_blocking_lock(i, "RealTimeInstance pre_poll").pre_poll(block_props);
        }
    }

    fn post_poll_real_time_instances(&self) {
        for (_, i) in self.real_time_instances.iter() {
            non_blocking_lock(i, "RealTimeInstance post_poll").post_poll();
        }
    }

    fn call_real_time_processors(
        &mut self,
        block_props: AudioBlockProps,
        sample_count: u64,
        might_be_rebirth: bool,
    ) {
        match &mut self.state {
            AudioHookState::Normal => {
                self.call_real_time_processors_in_normal_state(
                    block_props,
                    might_be_rebirth,
                    sample_count,
                );
            }
            AudioHookState::LearningSource {
                sender,
                midi_scanner,
            } => {
                for (_, p) in self.real_time_processors.iter() {
                    p.lock_recover()
                        .run_from_audio_hook_essential(block_props, might_be_rebirth);
                }
                for dev in Reaper::get().midi_input_devices() {
                    dev.with_midi_input(|mi| {
                        if let Some(mi) = mi {
                            for e in mi.get_read_buf() {
                                if let Some(res) = scan_midi(dev.id(), e, midi_scanner) {
                                    let _ = sender.try_send(res);
                                }
                            }
                        }
                    });
                }
                if let Some(res) = midi_scanner.poll() {
                    // Source detected via polling. Return to normal mode.
                    let _ = sender.try_send(res);
                }
            }
        };
    }

    fn call_real_time_processors_in_normal_state(
        &mut self,
        block_props: AudioBlockProps,
        might_be_rebirth: bool,
        sample_count: u64,
    ) {
        // 1a. Drive real-time processors and determine used MIDI devices "on the go".
        //
        // Calling the real-time processor *before* processing its remove task has
        // the benefit that it can still do some final work (e.g. clearing
        // LEDs by sending zero feedback) before it's removed. That's also
        // one of the reasons why we remove the real-time processor async by
        // sending a message. It's okay if it's around for one cycle after a
        // plug-in instance has unloaded (only the case if not the last instance).
        //
        let mut midi_dev_id_is_used = [false; MidiInputDeviceId::MAX_DEVICE_COUNT as usize];
        let mut midi_devs_used_at_all = false;
        let start_of_block_timestamp = ControlEventTimestamp::from_rt(
            sample_count,
            block_props.frame_rate,
            DurationInSeconds::ZERO,
        );
        for (_, p) in self.real_time_processors.iter() {
            // Since 1.12.0, we "drive" each plug-in instance's real-time processor
            // primarily by the global audio hook. See https://github.com/helgoboss/helgobox/issues/84 why this is
            // better. We also call it by the plug-in `process()` method though in order
            // to be able to send MIDI to <FX output> and to
            // stop doing so synchronously if the plug-in is
            // gone.
            let mut guard = p.lock_recover();
            guard.run_from_audio_hook_all(block_props, might_be_rebirth, start_of_block_timestamp);
            if guard.control_is_globally_enabled() {
                if let MidiControlInput::Device(dev_id) = guard.midi_control_input() {
                    midi_dev_id_is_used[dev_id.get() as usize] = true;
                    midi_devs_used_at_all = true;
                }
            }
        }
        // 1b. Forward MIDI events from MIDI devices to ReaLearn instances and filter
        //     them globally if desired by the instance.
        if midi_devs_used_at_all {
            self.distribute_midi_events_to_processors(
                block_props,
                &midi_dev_id_is_used,
                sample_count,
            );
        }
    }

    fn distribute_midi_events_to_processors(
        &mut self,
        block_props: AudioBlockProps,
        midi_dev_id_is_used: &[bool; MidiInputDeviceId::MAX_DEVICE_COUNT as usize],
        sample_count: u64,
    ) {
        self.midi_transformation_container
            .prepare(block_props.frame_rate);
        for dev_id in 0..MidiInputDeviceId::MAX_DEVICE_COUNT {
            if !midi_dev_id_is_used[dev_id as usize] {
                continue;
            }
            let dev_id = MidiInputDeviceId::new(dev_id);
            MidiInputDevice::new(dev_id).with_midi_input(|mi| {
                if let Some(mi) = mi {
                    let event_list = mi.get_read_buf();
                    let mut bpos = 0;
                    while let Some(res) = event_list.enum_items(bpos) {
                        let next_bpos = res.next_bpos;
                        // Current control mode is checked further down the callstack. No need to
                        // check it here.
                        let our_event =
                            match MidiEvent::from_reaper(res.midi_event, block_props.frame_rate) {
                                Err(_) => continue,
                                Ok(e) => e,
                            };
                        let frame_offset_in_secs =
                            res.midi_event.frame_offset() as f64 / MIDI_INPUT_FRAME_RATE.get();
                        let timestamp = ControlEventTimestamp::from_rt(
                            sample_count,
                            block_props.frame_rate,
                            DurationInSeconds::new_panic(frame_offset_in_secs),
                        );
                        let our_event = ControlEvent::new(our_event, timestamp);
                        let mut filter_out_event = false;
                        for (_, p) in self.real_time_processors.iter() {
                            let mut guard = p.lock_recover();
                            if guard.control_is_globally_enabled()
                                && guard.midi_control_input() == MidiControlInput::Device(dev_id)
                                && guard.process_incoming_midi_from_audio_hook(
                                    our_event,
                                    &mut self.midi_transformation_container,
                                )
                            {
                                filter_out_event = true;
                            }
                        }
                        if filter_out_event {
                            // Take event out of input buffer. In this case, we must not adjust bpos
                            // because just deleting the item has the same effect.
                            event_list.delete_item(bpos);
                        } else {
                            // Move cursor to next position
                            bpos = next_bpos;
                        }
                    }
                    // Add transformed events *after* iterating
                    for event in self
                        .midi_transformation_container
                        .drain_same_device_events()
                    {
                        let reaper_event = reaper_medium::MidiEvent::from_raw_ref(event.as_ref());
                        event_list.add_item(reaper_event);
                    }
                }
            });
        }
        // Process MIDI "MIDI: Send message" to "Device input" across multiple devices
        for evt in self
            .midi_transformation_container
            .drain_other_device_events()
        {
            MidiInputDevice::new(evt.input_device_id).with_midi_input(|mi| {
                if let Some(mi) = mi {
                    let event_list = mi.get_read_buf();
                    let reaper_event = reaper_medium::MidiEvent::from_raw_ref(evt.event.as_ref());
                    event_list.add_item(reaper_event);
                }
            });
        }
    }

    fn process_midi_device_inquiry_command(
        &mut self,
        command: RequestMidiDeviceIdentityCommand,
    ) -> Result<(), &'static str> {
        let output_dev_id = command.output_device_id;
        let output_dev = Reaper::get().midi_output_device_by_id(output_dev_id);
        output_dev.with_midi_output(|output| -> Result<(), &'static str> {
            let output = output.ok_or("MIDI output device not open")?;
            let inquiry = RawMidiEvent::try_from_slice(0, MIDI_DEVICE_INQUIRY_REQUEST)?;
            tracing::debug!(msg = "Sending MIDI device inquiry...", ?output_dev_id);
            output.send_msg(inquiry, SendMidiTime::Instantly);
            Ok(())
        })?;
        let task = MidiDeviceInquiryTask {
            command,
            inquiry_sent_at: Instant::now(),
        };
        self.midi_device_inquiry_task = Some(task);
        Ok(())
    }

    fn check_for_midi_device_inquiry_response(&mut self) {
        let Some(task) = self.midi_device_inquiry_task.as_ref() else {
            // No task
            return;
        };
        if !task.check_for_midi_device_inquiry_response() {
            // Task done
            self.midi_device_inquiry_task = None;
        }
    }

    fn process_normal_commands(&mut self, block_props: AudioBlockProps) {
        use NormalAudioHookTask::*;
        let mut count = 0;
        while let Ok(task) = self.normal_task_receiver.try_recv() {
            match task {
                AddRealTimeInstance(id, p) => {
                    self.real_time_instances.push((id, p));
                }
                RemoveRealTimeInstance(id) => {
                    if let Some(pos) = self.real_time_instances.iter().position(|(i, _)| i == &id) {
                        self.real_time_instances.swap_remove(pos);
                    }
                }
                AddRealTimeProcessor(id, p) => {
                    self.real_time_processors.push((id, p));
                }
                RemoveRealTimeProcessor(id) => {
                    if let Some(pos) = self.real_time_processors.iter().position(|(i, _)| i == &id)
                    {
                        self.real_time_processors.swap_remove(pos);
                    }
                }
                StartCapturingMidi(sender) => {
                    self.state = AudioHookState::LearningSource {
                        sender,
                        midi_scanner: Default::default(),
                    }
                }
                StopCapturingMidi => {
                    self.state = AudioHookState::Normal;
                }
                RequestMidiDeviceIdentity(command) => {
                    let _ = self.process_midi_device_inquiry_command(command);
                }
                #[cfg(feature = "playtime")]
                PlaytimeClipEngineCommand(command) => {
                    let _ = self
                        .clip_engine_audio_hook
                        .on_pre_process_command(command, block_props.to_playtime());
                }
            }
            // Don't take too much at once
            count += 1;
            if count == AUDIO_HOOK_TASK_BULK_SIZE {
                break;
            }
        }
        let _ = block_props;
    }
}

impl OnAudioBuffer for RealearnAudioHook {
    fn call(&mut self, args: OnAudioBufferArgs) {
        if !self.initialized {
            self.init_from_rt_thread();
            self.initialized = true;
        }
        assert_no_alloc(|| {
            let is_pre = !args.is_post;
            if is_pre {
                measure_time("helgobox.rt.audio_hook.pre", || {
                    self.on_pre(args);
                });
            } else {
                measure_time("helgobox.rt.audio_hook.post", || {
                    self.on_post(args);
                });
            }
        });
    }
}

fn scan_midi(
    dev_id: MidiInputDeviceId,
    evt: &reaper_medium::MidiEvent,
    midi_scanner: &mut MidiScanner,
) -> Option<MidiScanResult> {
    let msg = IncomingMidiMessage::from_reaper(evt.message()).ok()?;
    if classify_midi_message(msg) != MidiMessageClassification::Normal {
        return None;
    }
    use IncomingMidiMessage::*;
    match msg {
        Short(short_msg) => midi_scanner.feed_short(short_msg, Some(dev_id)),
        SysEx(bytes) => {
            // It's okay here to temporarily permit allocation because crackling during learning
            // is not a showstopper.
            permit_alloc(|| MidiScanResult::try_from_bytes(bytes, Some(dev_id)).ok())
        }
    }
}

pub trait RealTimeProcessorLocker {
    fn lock_recover(&self) -> MutexGuard<RealTimeProcessor>;
}

impl RealTimeProcessorLocker for SharedRealTimeProcessor {
    /// This ignores poisoning, which is okay in our case because if the real-time
    /// processor has panicked, we will see it in the REAPER console. No need to
    /// hide that error with lots of follow-up poisoning errors! This is a kind of
    /// recovery mechanism.
    fn lock_recover(&self) -> MutexGuard<RealTimeProcessor> {
        non_blocking_lock(self, "RealTimeProcessor")
    }
}

impl MidiDeviceInquiryTask {
    /// Returns `false` if task not necessary anymore.
    pub fn check_for_midi_device_inquiry_response(&self) -> bool {
        // Give up if waited too long for response.
        if self.inquiry_sent_at.elapsed() > Duration::from_secs(1) {
            tracing::debug!(msg = "Gave up waiting for MIDI device identity reply after timeout");
            return false;
        }
        // Check MIDI devices in question for response
        if let Some(id) = self.command.input_device_id {
            // Check user-defined input device for possible response
            let dev = Reaper::get().midi_input_device_by_id(id);
            if !self.process_input_dev(dev) {
                return false;
            }
        } else {
            // Check all input devices for possible response
            for dev in Reaper::get().midi_input_devices() {
                if !self.process_input_dev(dev) {
                    return false;
                }
            }
        }
        // Return true as long as we haven't got a response yet.
        true
    }

    /// Returns `false` if task not necessary anymore.
    fn process_input_dev(&self, dev: MidiInputDevice) -> bool {
        dev.with_midi_input(|mi| {
            let Some(mi) = mi else {
                return true;
            };
            for evt in mi.get_read_buf() {
                let msg = evt.message();
                if msg.r#type() == ShortMessageType::SystemExclusiveStart {
                    let reply_pattern = &MIDI_DEVICE_INQUIRY_REPLY_PATTERN;
                    let reply_pattern =
                        reply_pattern.get_or_init(create_device_inquiry_reply_pattern);
                    let is_identity_reply = reply_pattern.matches(msg.as_slice());
                    let Ok(message) = ArrayVec::try_from(msg.as_slice()) else {
                        // Couldn't store the reply in the array. Shouldn't happen here because
                        // we set the ArrayVec's capacity to the max size of the raw event.
                        // So at a maximum it will be cropped.
                        return false;
                    };
                    if is_identity_reply {
                        let reply = RequestMidiDeviceIdentityReply {
                            input_device_id: dev.id(),
                            device_inquiry_reply: MidiDeviceInquiryReply { message },
                        };
                        tracing::debug!(msg = "Received MIDI device identity reply", ?reply);
                        let _ = self.command.sender.try_send(reply);
                        return false;
                    }
                }
            }
            true
        })
    }
}

const MIDI_DEVICE_INQUIRY_REQUEST: &[u8] = &[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

static MIDI_DEVICE_INQUIRY_REPLY_PATTERN: OnceLock<BytePattern> = OnceLock::new();

fn create_device_inquiry_reply_pattern() -> BytePattern {
    use Fixed as F;
    use PatternByte::*;
    BytePattern::new(vec![
        F(0xF0),
        F(0x7E),
        Single,
        F(0x06),
        F(0x02),
        Multi,
        F(0xF7),
    ])
}
