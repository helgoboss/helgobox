use crate::domain::Mapping;
use helgoboss_midi::ShortMessage;

#[derive(Debug)]
pub struct RealTimeProcessor {
    receiver: crossbeam_channel::Receiver<RealTimeTask>,
    // TODO Check if receiving this vec will result in allocation
    mappings: Vec<Mapping>,
    let_matched_events_through: bool,
    let_unmatched_events_through: bool,
}

impl RealTimeProcessor {
    pub fn new(receiver: crossbeam_channel::Receiver<RealTimeTask>) -> RealTimeProcessor {
        RealTimeProcessor {
            receiver,
            mappings: vec![],
            let_matched_events_through: false,
            let_unmatched_events_through: false,
        }
    }

    pub fn process_midi(&self, msg: impl ShortMessage) {
        // TODO
    }

    /// Should be called regularly in real-time audio thread.
    pub fn idle(&mut self) {
        for task in self.receiver.try_iter().take(1) {
            use RealTimeTask::*;
            match task {
                UpdateMappings(mappings) => {
                    println!("Mappings synced: {:?}", &mappings);
                    self.mappings = mappings
                }
                UpdateFlags {
                    let_matched_events_through,
                    let_unmatched_events_through,
                } => {
                    println!("Flags synced");
                    self.let_matched_events_through = let_matched_events_through;
                    self.let_unmatched_events_through = let_unmatched_events_through;
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum RealTimeTask {
    UpdateMappings(Vec<Mapping>),
    UpdateFlags {
        let_matched_events_through: bool,
        let_unmatched_events_through: bool,
    },
}
