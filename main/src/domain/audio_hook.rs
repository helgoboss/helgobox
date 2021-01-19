use crate::domain::RealTimeProcessor;
use reaper_medium::{OnAudioBuffer, OnAudioBufferArgs};
use smallvec::SmallVec;
use std::sync::{Arc, Mutex};

/// This needs to be thread-safe because if "Allow live FX multiprocessing" is active in the REAPER
/// preferences, the VST processing is executed in another thread than the audio hook!
pub type SharedRealTimeProcessor = Arc<Mutex<RealTimeProcessor>>;

pub enum RealearnAudioHookTask {
    /// First parameter is the ID.
    //
    // Having the ID saves us from unnecessarily blocking the audio thread by looking into the
    // processor.
    AddRealTimeProcessor(String, SharedRealTimeProcessor),
    RemoveRealTimeProcessor(String),
}

#[derive(Debug)]
pub struct RealearnAudioHook {
    real_time_processors: SmallVec<[(String, SharedRealTimeProcessor); 256]>,
    task_receiver: crossbeam_channel::Receiver<RealearnAudioHookTask>,
}

impl RealearnAudioHook {
    pub fn new(
        task_receiver: crossbeam_channel::Receiver<RealearnAudioHookTask>,
    ) -> RealearnAudioHook {
        Self {
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
        // 1. Call real-time processors.
        //
        // Calling the real-time processor *before* processing its remove task might have the
        // benefit, that it can still do some final work (e.g. clearing LEDs by sending zero
        // feedback) before it's removed. That's also one of the reasons why we remove the real-time
        // processor async by sending a message. It's okay if it's around for one cycle after a
        // plug-in instance has unloaded (only the case if not the last instance).
        for (_, p) in self.real_time_processors.iter() {
            // Since 1.12.0, we "drive" each plug-in instance's real-time processor primarily by the
            // global audio hook. See https://github.com/helgoboss/realearn/issues/84 why this is
            // better. We also call it by the plug-in `process()` method though in order to be able
            // to send MIDI to <FX output> and to stop doing so synchronously if the plug-in is
            // gone.
            p.lock().unwrap().run_from_audio_hook(args.len as _);
        }
        // 2. Process add/remove tasks.
        for task in self.task_receiver.try_iter().take(1) {
            use RealearnAudioHookTask::*;
            match task {
                AddRealTimeProcessor(id, p) => {
                    self.real_time_processors.push((id, p));
                }
                RemoveRealTimeProcessor(id) => {
                    self.real_time_processors.retain(|(i, _)| i != &id);
                }
            }
        }
    }
}
