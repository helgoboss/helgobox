use crate::domain::{
    classify_midi_message, MidiMessageClassification, MidiSourceScanner, RealTimeProcessor,
};
use helgoboss_learn::MidiSource;
use helgoboss_midi::ShortMessage;
use reaper_high::Reaper;
use reaper_medium::{MidiEvent, MidiInputDeviceId, OnAudioBuffer, OnAudioBufferArgs};
use smallvec::SmallVec;
use std::sync::{Arc, Mutex};

const AUDIO_HOOK_TASK_BULK_SIZE: usize = 1;

/// This needs to be thread-safe because if "Allow live FX multiprocessing" is active in the REAPER
/// preferences, the VST processing is executed in another thread than the audio hook!
pub type SharedRealTimeProcessor = Arc<Mutex<RealTimeProcessor>>;

type LearnSourceSender = async_channel::Sender<(MidiInputDeviceId, MidiSource)>;

pub enum RealearnAudioHookTask {
    /// First parameter is the ID.
    //
    // Having the ID saves us from unnecessarily blocking the audio thread by looking into the
    // processor.
    AddRealTimeProcessor(String, SharedRealTimeProcessor),
    RemoveRealTimeProcessor(String),
    StartLearningSources(LearnSourceSender),
    StopLearningSources,
}

#[derive(Debug)]
pub struct RealearnAudioHook {
    state: AudioHookState,
    real_time_processors: SmallVec<[(String, SharedRealTimeProcessor); 256]>,
    task_receiver: crossbeam_channel::Receiver<RealearnAudioHookTask>,
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
        task_receiver: crossbeam_channel::Receiver<RealearnAudioHookTask>,
    ) -> RealearnAudioHook {
        Self {
            state: AudioHookState::Normal,
            real_time_processors: Default::default(),
            task_receiver,
        }
    }
}

impl OnAudioBuffer for RealearnAudioHook {
    fn call(&mut self, args: OnAudioBufferArgs) {
        if args.is_post {
            return;
        }
        match &mut self.state {
            AudioHookState::Normal => {
                // 1. Call real-time processors.
                //
                // Calling the real-time processor *before* processing its remove task might have
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
                    p.lock().unwrap().run_from_audio_hook_all(args.len as _);
                }
            }
            AudioHookState::LearningSource {
                sender,
                midi_source_scanner,
            } => {
                for (_, p) in self.real_time_processors.iter() {
                    p.lock()
                        .unwrap()
                        .run_from_audio_hook_essential(args.len as _);
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
            .task_receiver
            .try_iter()
            .take(AUDIO_HOOK_TASK_BULK_SIZE)
        {
            use RealearnAudioHookTask::*;
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
    evt: MidiEvent,
    midi_source_scanner: &mut MidiSourceScanner,
) -> Option<MidiSource> {
    let raw_msg = evt.message().to_other();
    if classify_midi_message(raw_msg) != MidiMessageClassification::Normal {
        return None;
    }
    midi_source_scanner.feed_short(raw_msg, Some(dev_id))
}
