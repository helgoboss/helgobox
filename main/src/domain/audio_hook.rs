use crate::domain::{
    classify_midi_message, AudioBlockProps, ControlEvent, ControlEventTimestamp,
    IncomingMidiMessage, InstanceId, MidiControlInput, MidiEvent, MidiMessageClassification,
    MidiScanResult, MidiScanner, RealTimeProcessor, SharedRealTimeInstance, UnitId,
};
use base::metrics_util::record_duration;
use base::non_blocking_lock;
use helgoboss_allocator::*;
use helgoboss_learn::{AbstractTimestamp, MidiSourceValue, RawMidiEvents};
use helgoboss_midi::{DataEntryByteOrder, RawShortMessage};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::{
    MidiInputDeviceId, MidiOutputDeviceId, OnAudioBuffer, OnAudioBufferArgs, SendMidiTime,
};
use smallvec::SmallVec;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const AUDIO_HOOK_TASK_BULK_SIZE: usize = 1;
const FEEDBACK_TASK_BULK_SIZE: usize = 1000;

/// This needs to be thread-safe because if "Allow live FX multiprocessing" is active in the REAPER
/// preferences, the VST processing is executed in another thread than the audio hook!
pub type SharedRealTimeProcessor = Arc<Mutex<RealTimeProcessor>>;

pub type MidiCaptureSender = async_channel::Sender<MidiScanResult>;

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
    #[cfg(feature = "playtime")]
    PlaytimeClipEngineCommand(playtime_clip_engine::rt::audio_hook::ClipEngineAudioHookCommand),
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

#[derive(Debug)]
pub struct RealearnAudioHook {
    state: AudioHookState,
    real_time_instances: SmallVec<[(InstanceId, SharedRealTimeInstance); 256]>,
    real_time_processors: SmallVec<[(UnitId, SharedRealTimeProcessor); 256]>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    time_of_last_run: Option<Instant>,
    initialized: bool,
    #[cfg(feature = "playtime")]
    clip_engine_audio_hook: playtime_clip_engine::rt::audio_hook::ClipEngineAudioHook,
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
            real_time_instances: Default::default(),
            real_time_processors: Default::default(),
            normal_task_receiver,
            feedback_task_receiver,
            time_of_last_run: None,
            initialized: false,
            #[cfg(feature = "playtime")]
            clip_engine_audio_hook: playtime_clip_engine::rt::audio_hook::ClipEngineAudioHook::new(
            ),
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
        let block_props = AudioBlockProps::from_on_audio_buffer_args(&args);
        // Pre-poll Playtime
        #[cfg(feature = "playtime")]
        {
            self.clip_engine_audio_hook
                .on_pre_poll_1(block_props.to_playtime());
        }
        // Detect rebirth
        let might_be_rebirth = {
            let current_time = Instant::now();
            let time_of_last_run = self.time_of_last_run.replace(current_time);
            if let Some(time) = time_of_last_run {
                current_time.duration_since(time) > Duration::from_secs(1)
            } else {
                false
            }
        };
        // Do some ReaLearn things. The order probably matters here!
        self.process_feedback_commands();
        self.call_real_time_instances(block_props);
        self.call_real_time_processors(block_props, might_be_rebirth);
        // Process incoming commands, including Playtime commands
        self.process_normal_commands(block_props);
        // Pre-poll Playtime
        #[cfg(feature = "playtime")]
        {
            self.clip_engine_audio_hook
                .on_pre_poll_2(block_props.to_playtime(), args.reg);
        }
    }

    fn on_post(&mut self, args: OnAudioBufferArgs) {
        // Let Playtime do its processing
        #[cfg(feature = "playtime")]
        {
            let block_props = AudioBlockProps::from_on_audio_buffer_args(&args);
            self.clip_engine_audio_hook
                .on_post(block_props.to_playtime(), args.reg);
        }
        // Record some metrics
        let _ = args;
        if let Some(time_of_last_run) = self.time_of_last_run {
            record_duration("audio_callback_total", time_of_last_run.elapsed());
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

    fn call_real_time_instances(&self, block_props: AudioBlockProps) {
        for (_, i) in self.real_time_instances.iter() {
            non_blocking_lock(i, "RealTimeInstance").poll(block_props);
        }
    }

    fn call_real_time_processors(&mut self, block_props: AudioBlockProps, might_be_rebirth: bool) {
        match &mut self.state {
            AudioHookState::Normal => {
                let timestamp = ControlEventTimestamp::now();
                self.call_real_time_processors_in_normal_state(
                    block_props,
                    might_be_rebirth,
                    timestamp,
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
        timestamp: ControlEventTimestamp,
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
        for (_, p) in self.real_time_processors.iter() {
            // Since 1.12.0, we "drive" each plug-in instance's real-time processor
            // primarily by the global audio hook. See https://github.com/helgoboss/realearn/issues/84 why this is
            // better. We also call it by the plug-in `process()` method though in order
            // to be able to send MIDI to <FX output> and to
            // stop doing so synchronously if the plug-in is
            // gone.
            let mut guard = p.lock_recover();
            guard.run_from_audio_hook_all(block_props, might_be_rebirth, timestamp);
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
            self.distribute_midi_events_to_processors(block_props, &midi_dev_id_is_used, timestamp);
        }
    }

    fn distribute_midi_events_to_processors(
        &mut self,
        block_props: AudioBlockProps,
        midi_dev_id_is_used: &[bool; MidiInputDeviceId::MAX_DEVICE_COUNT as usize],
        timestamp: ControlEventTimestamp,
    ) {
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
                        // Current control mode is checked further down the callstack. No need to
                        // check it here.
                        let our_event =
                            match MidiEvent::from_reaper(res.midi_event, block_props.frame_rate) {
                                Err(_) => continue,
                                Ok(e) => e,
                            };
                        let our_event = ControlEvent::new(our_event, timestamp);
                        let mut filter_out_event = false;
                        for (_, p) in self.real_time_processors.iter() {
                            let mut guard = p.lock_recover();
                            if guard.control_is_globally_enabled()
                                && guard.midi_control_input() == MidiControlInput::Device(dev_id)
                                && guard.process_incoming_midi_from_audio_hook(our_event)
                            {
                                filter_out_event = true;
                            }
                        }
                        if filter_out_event {
                            event_list.delete_item(bpos);
                        } else {
                            bpos = res.next_bpos;
                        }
                    }
                }
            });
        }
    }

    fn process_normal_commands(&mut self, block_props: AudioBlockProps) {
        for task in self
            .normal_task_receiver
            .try_iter()
            .take(AUDIO_HOOK_TASK_BULK_SIZE)
        {
            use NormalAudioHookTask::*;
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
                #[cfg(feature = "playtime")]
                PlaytimeClipEngineCommand(command) => {
                    let _ = self
                        .clip_engine_audio_hook
                        .on_pre_process_command(command, block_props.to_playtime());
                }
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
                self.on_pre(args);
            } else {
                self.on_post(args);
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
