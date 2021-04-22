use crate::domain::{
    classify_midi_message, MidiMessageClassification, MidiSource, MidiSourceScanner,
    RealTimeProcessor,
};
use helgoboss_learn::{MidiSourceValue, RawMidiEvent};
use helgoboss_midi::{DataEntryByteOrder, RawShortMessage, ShortMessage};
use reaper_high::{MidiOutputDevice, Reaper};
use reaper_medium::{
    BorrowedMidiEvent, MidiInputDeviceId, MidiOutputDeviceId, OnAudioBuffer, OnAudioBufferArgs,
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

type LearnSourceSender = async_channel::Sender<(MidiInputDeviceId, MidiSource)>;

// This kind of tasks is always processed, even after a rebirth when multiple processor syncs etc.
// have already accumulated. Because at the moment there's no way to request a full resync of all
// real-time processors from the control surface. In practice there's no danger that too many of
// those infrequent tasks accumulate so it's not an issue. Therefore the convention for now is to
// also send them when audio is not running.
pub enum NormalAudioHookTask {
    /// First parameter is the ID.
    //
    // Having the ID saves us from unnecessarily blocking the audio thread by looking into the
    // processor.
    AddRealTimeProcessor(String, SharedRealTimeProcessor),
    RemoveRealTimeProcessor(String),
    StartLearningSources(LearnSourceSender),
    StopLearningSources,
}

/// A global feedback task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackAudioHookTask {
    MidiDeviceFeedback(MidiOutputDeviceId, MidiSourceValue<RawShortMessage>),
    SendMidi(MidiOutputDeviceId, Box<RawMidiEvent>),
}

#[derive(Debug)]
pub struct RealearnAudioHook {
    state: AudioHookState,
    real_time_processors: SmallVec<[(String, SharedRealTimeProcessor); 256]>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    time_of_last_run: Option<Instant>,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum AudioHookState {
    Normal,
    // This is not the instance-specific learning but the global one.
    LearningSource {
        sender: LearnSourceSender,
        midi_source_scanner: MidiSourceScanner,
    },
}

impl RealearnAudioHook {
    pub fn new(
        normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
        feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    ) -> RealearnAudioHook {
        Self {
            state: AudioHookState::Normal,
            real_time_processors: Default::default(),
            normal_task_receiver,
            feedback_task_receiver,
            time_of_last_run: None,
        }
    }
}

impl OnAudioBuffer for RealearnAudioHook {
    fn call(&mut self, args: OnAudioBufferArgs) {
        if args.is_post {
            return;
        }
        let current_time = Instant::now();
        let time_of_last_run = self.time_of_last_run.replace(current_time);
        let might_be_rebirth = if let Some(time) = time_of_last_run {
            current_time.duration_since(time) > Duration::from_secs(1)
        } else {
            false
        };
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
                    if let MidiSourceValue::Raw(msg) = value {
                        MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                            if let Some(mo) = mo {
                                mo.send_msg(&*msg, SendMidiTime::Instantly);
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
                SendMidi(dev_id, raw_midi_event) => {
                    MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                        if let Some(mo) = mo {
                            mo.send_msg(&*raw_midi_event, SendMidiTime::Instantly);
                        }
                    });
                }
            }
        }
        // Process depending on state
        match &mut self.state {
            AudioHookState::Normal => {
                // 1. Call real-time processors.
                //
                // Calling the real-time processor *before* processing its remove task has
                // the benefit, that it can still do some final work (e.g. clearing
                // LEDs by sending zero feedback) before it's removed. That's also
                // one of the reasons why we remove the real-time processor async by
                // sending a message. It's okay if it's around for one cycle after a
                // plug-in instance has unloaded (only the case if not the last instance).
                for (_, p) in self.real_time_processors.iter() {
                    // Since 1.12.0, we "drive" each plug-in instance's real-time processor
                    // primarily by the global audio hook. See https://github.com/helgoboss/realearn/issues/84 why this is
                    // better. We also call it by the plug-in `process()` method though in order to
                    // be able to send MIDI to <FX output> and to stop doing so
                    // synchronously if the plug-in is gone.
                    p.lock_recover()
                        .run_from_audio_hook_all(args.len as _, might_be_rebirth);
                }
            }
            AudioHookState::LearningSource {
                sender,
                midi_source_scanner,
            } => {
                for (_, p) in self.real_time_processors.iter() {
                    p.lock_recover()
                        .run_from_audio_hook_essential(args.len as _, might_be_rebirth);
                }
                for dev in Reaper::get().midi_input_devices() {
                    dev.with_midi_input(|mi| {
                        if let Some(mi) = mi {
                            for evt in mi.get_read_buf().enum_items(0) {
                                if let Some(source) =
                                    process_midi_event(dev.id(), evt, midi_source_scanner)
                                {
                                    let _ = sender.try_send((dev.id(), source));
                                }
                            }
                        }
                    });
                }
                if let Some((source, Some(dev_id))) = midi_source_scanner.poll() {
                    // Source detected via polling. Return to normal mode.
                    let _ = sender.try_send((dev_id, source));
                }
            }
        };
        // 2. Process add/remove tasks.
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
                    self.real_time_processors.retain(|(i, _)| i != &id);
                }
                StartLearningSources(sender) => {
                    self.state = AudioHookState::LearningSource {
                        sender,
                        midi_source_scanner: Default::default(),
                    }
                }
                StopLearningSources => self.state = AudioHookState::Normal,
            }
        }
    }
}

fn process_midi_event(
    dev_id: MidiInputDeviceId,
    evt: &BorrowedMidiEvent,
    midi_source_scanner: &mut MidiSourceScanner,
) -> Option<MidiSource> {
    let raw_msg = evt.message().to_other();
    if classify_midi_message(raw_msg) != MidiMessageClassification::Normal {
        return None;
    }
    midi_source_scanner.feed_short(raw_msg, Some(dev_id))
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
        match self.lock() {
            Ok(guard) => guard,
            Err(e) => e.into_inner(),
        }
    }
}
