use crate::domain::RealTimeProcessor;
use reaper_medium::{OnAudioBuffer, OnAudioBufferArgs};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::rc::Rc;

// Rc is cloned in main thread only. RefCell is entered in audio thread only.
pub type SharedRealTimeProcessor = Rc<RefCell<RealTimeProcessor>>;

pub enum RealearnAudioHookTask {
    AddRealTimeProcessor(SharedRealTimeProcessor),
    RemoveRealTimeProcessor(String),
}

pub struct RealearnAudioHook {
    real_time_processors: SmallVec<[SharedRealTimeProcessor; 256]>,
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
        for task in self.task_receiver.try_iter().take(1) {
            use RealearnAudioHookTask::*;
            match task {
                AddRealTimeProcessor(p) => {
                    self.real_time_processors.push(p);
                }
                RemoveRealTimeProcessor(id) => {
                    self.real_time_processors
                        .retain(|p| p.borrow().instance_id() != id);
                }
            }
        }
        for p in self.real_time_processors.iter() {
            p.borrow_mut().run(args.len as _);
        }
    }
}
