use crate::base::non_blocking_lock;
use crate::domain::{
    classify_midi_message, AudioBlockProps, ControlEvent, ControlEventTimestamp, Garbage,
    GarbageBin, IncomingMidiMessage, InstanceId, MidiControlInput, MidiEvent,
    MidiMessageClassification, MidiScanResult, MidiScanner, RealTimeProcessor,
};
use assert_no_alloc::*;
use helgoboss_learn::{MidiSourceValue, RawMidiEvents};
use helgoboss_midi::{Channel, DataEntryByteOrder, RawShortMessage};
use playtime_clip_engine::global_steady_timeline_state;
use playtime_clip_engine::main::{
    ClipRecordDestination, ClipRecordHardwareInput, ClipRecordHardwareMidiInput,
    VirtualClipRecordHardwareMidiInput,
};
use playtime_clip_engine::rt::supplier::{WriteAudioRequest, WriteMidiRequest};
use playtime_clip_engine::rt::{AudioBuf, BasicAudioRequestProps, Column};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::{
    AudioHookRegister, MidiInputDeviceId, MidiOutputDeviceId, OnAudioBuffer, OnAudioBufferArgs,
    SendMidiTime,
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
    /// First parameter is the ID.
    //
    // Having the ID saves us from unnecessarily blocking the audio thread by looking into the
    // processor.
    AddRealTimeProcessor(InstanceId, SharedRealTimeProcessor),
    RemoveRealTimeProcessor(InstanceId),
    StartCapturingMidi(MidiCaptureSender),
    StopCapturingMidi,
    StartClipRecording(HardwareInputClipRecordTask),
}

#[derive(Debug)]
pub struct HardwareInputClipRecordTask {
    pub input: ClipRecordHardwareInput,
    pub destination: ClipRecordDestination,
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
    real_time_processors: SmallVec<[(InstanceId, SharedRealTimeProcessor); 256]>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    time_of_last_run: Option<Instant>,
    garbage_bin: GarbageBin,
    clip_record_task: Option<HardwareInputClipRecordTask>,
    initialized: bool,
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
        garbage_bin: GarbageBin,
    ) -> RealearnAudioHook {
        Self {
            state: AudioHookState::Normal,
            real_time_processors: Default::default(),
            normal_task_receiver,
            feedback_task_receiver,
            time_of_last_run: None,
            garbage_bin,
            clip_record_task: None,
            initialized: false,
        }
    }

    fn process_feedback_tasks(&mut self) {
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
                                    mo.send_msg(&*event, SendMidiTime::Instantly);
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
                    if let Some(garbage) = value.into_garbage() {
                        self.garbage_bin.dispose(Garbage::RawMidiEvents(garbage));
                    }
                }
                SendMidi(dev_id, raw_midi_events) => {
                    MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                        if let Some(mo) = mo {
                            for event in &raw_midi_events {
                                mo.send_msg(&*event, SendMidiTime::Instantly);
                            }
                        }
                    });
                    self.garbage_bin
                        .dispose(Garbage::RawMidiEvents(raw_midi_events));
                }
            }
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

    fn process_clip_record_task(&mut self, args: &OnAudioBufferArgs) {
        if let Some(t) = &mut self.clip_record_task {
            let its_our_turn = (t.destination.is_midi_overdub && args.is_post)
                || (!t.destination.is_midi_overdub && !args.is_post);
            if its_our_turn && !process_clip_record_task(args, t) {
                tracing_debug!("Clearing clip record task from audio hook");
                self.clip_record_task = None;
            }
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

    fn process_normal_tasks(&mut self) {
        for task in self
            .normal_task_receiver
            .try_iter()
            .take(AUDIO_HOOK_TASK_BULK_SIZE)
        {
            use NormalAudioHookTask::*;
            match task {
                AddRealTimeProcessor(id, p) => {
                    self.real_time_processors.push((id, p));
                }
                RemoveRealTimeProcessor(id) => {
                    if let Some(pos) = self.real_time_processors.iter().position(|(i, _)| i == &id)
                    {
                        let (_, proc) = self.real_time_processors.swap_remove(pos);
                        self.garbage_bin.dispose(Garbage::RealTimeProcessor(proc));
                    }
                }
                StartCapturingMidi(sender) => {
                    self.state = AudioHookState::LearningSource {
                        sender,
                        midi_scanner: Default::default(),
                    }
                }
                StopCapturingMidi => {
                    let last_state = std::mem::replace(&mut self.state, AudioHookState::Normal);
                    if let AudioHookState::LearningSource { sender, .. } = last_state {
                        self.garbage_bin.dispose(Garbage::MidiCaptureSender(sender));
                    }
                }
                StartClipRecording(task) => {
                    tracing_debug!("Audio hook received clip record task");
                    self.clip_record_task = Some(task);
                }
            }
        }
    }
}

impl OnAudioBuffer for RealearnAudioHook {
    fn call(&mut self, args: OnAudioBufferArgs) {
        if !self.initialized {
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
            self.initialized = true;
        }
        assert_no_alloc(|| {
            if !args.is_post {
                let block_props = AudioBlockProps::from_on_audio_buffer_args(&args);
                global_steady_timeline_state().on_audio_buffer(block_props.to_playtime());
                let current_time = Instant::now();
                let time_of_last_run = self.time_of_last_run.replace(current_time);
                let might_be_rebirth = if let Some(time) = time_of_last_run {
                    current_time.duration_since(time) > Duration::from_secs(1)
                } else {
                    false
                };
                self.process_feedback_tasks();
                self.call_real_time_processors(block_props, might_be_rebirth);
            }
            self.process_clip_record_task(&args);
            // Process normal tasks after processing the clip record task so that clip recording
            // starts in next cycle, not in this one (in this one, the clip is not yet prepared
            // for recording if this is a is_post = false record task).
            if !args.is_post {
                self.process_normal_tasks();
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

/// Returns whether task still relevant.
fn process_clip_record_task(
    args: &OnAudioBufferArgs,
    record_task: &mut HardwareInputClipRecordTask,
) -> bool {
    let column_source = match record_task.destination.column_source.upgrade() {
        None => return false,
        Some(s) => s,
    };
    let mut src = column_source.lock();
    let block_props = BasicAudioRequestProps::from_on_audio_buffer_args(args);
    if !src.recording_poll(record_task.destination.slot_index, block_props) {
        return false;
    }
    match &mut record_task.input {
        ClipRecordHardwareInput::Midi(input) => {
            use VirtualClipRecordHardwareMidiInput::*;
            let specific_input = match input {
                Specific(s) => *s,
                Detect => {
                    // Detect
                    match find_first_dev_with_play_msg() {
                        None => {
                            // No play message detected so far in any input device.
                            return true;
                        }
                        Some(dev_id) => {
                            // Found first play message in this device. Leave "Detect" mode and
                            // capture from this specific device from now on.
                            let specific_input = ClipRecordHardwareMidiInput {
                                device_id: Some(dev_id),
                                channel: None,
                            };
                            *input = Specific(specific_input);
                            specific_input
                        }
                    }
                }
            };
            if let Some(dev_id) = specific_input.device_id {
                // Read from specific MIDI input device
                let dev = Reaper::get().midi_input_device_by_id(dev_id);
                write_midi_to_clip_slot(
                    block_props,
                    &mut src,
                    record_task.destination.slot_index,
                    dev,
                    specific_input.channel,
                );
            } else {
                // Read from all open MIDI input devices
                for dev in Reaper::get().midi_input_devices() {
                    write_midi_to_clip_slot(
                        block_props,
                        &mut src,
                        record_task.destination.slot_index,
                        dev,
                        specific_input.channel,
                    );
                }
            }
        }
        ClipRecordHardwareInput::Audio(input) => {
            let channel_offset = input.channel_offset().unwrap();
            let write_audio_request =
                AudioHookWriteAudioRequest::new(args.reg, block_props, channel_offset as _);
            src.write_clip_audio(record_task.destination.slot_index, write_audio_request)
                .unwrap();
        }
    }
    true
}

fn find_first_dev_with_play_msg() -> Option<MidiInputDeviceId> {
    for dev in Reaper::get().midi_input_devices() {
        let contains_play_msg = dev.with_midi_input(|mi| match mi {
            None => false,
            Some(mi) => mi
                .get_read_buf()
                .into_iter()
                .any(|e| playtime_clip_engine::midi_util::is_play_message(e.message())),
        });
        if contains_play_msg {
            return Some(dev.id());
        }
    }
    None
}

fn write_midi_to_clip_slot(
    block_props: BasicAudioRequestProps,
    src: &mut MutexGuard<Column>,
    slot_index: usize,
    dev: MidiInputDevice,
    channel_filter: Option<Channel>,
) {
    dev.with_midi_input(|mi| {
        let mi = match mi {
            None => return,
            Some(m) => m,
        };
        let events = mi.get_read_buf();
        if events.get_size() == 0 {
            return;
        }
        let req = WriteMidiRequest {
            audio_request_props: block_props,
            events,
            channel_filter,
        };
        src.write_clip_midi(slot_index, req).unwrap();
    });
}

#[derive(Copy, Clone)]
struct AudioHookWriteAudioRequest<'a> {
    channel_offset: usize,
    register: &'a AudioHookRegister,
    block_props: BasicAudioRequestProps,
}

impl<'a> AudioHookWriteAudioRequest<'a> {
    pub fn new(
        register: &'a AudioHookRegister,
        block_props: BasicAudioRequestProps,
        channel_offset: usize,
    ) -> Self {
        Self {
            channel_offset,
            register,
            block_props,
        }
    }
}

impl<'a> WriteAudioRequest for AudioHookWriteAudioRequest<'a> {
    fn audio_request_props(&self) -> BasicAudioRequestProps {
        self.block_props
    }

    fn get_channel_buffer(&self, channel_index: usize) -> Option<AudioBuf> {
        let reg = unsafe { self.register.get().as_ref() };
        let get_buffer = match reg.GetBuffer {
            None => return None,
            Some(f) => f,
        };
        let effective_channel_index = self.channel_offset + channel_index;
        let buf = unsafe { (get_buffer)(false, effective_channel_index as _) };
        if buf.is_null() {
            return None;
        }
        let buf = unsafe { AudioBuf::from_raw(buf, 1, self.block_props.block_length) };
        Some(buf)
    }
}
