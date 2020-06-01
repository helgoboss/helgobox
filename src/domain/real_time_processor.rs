use crate::domain::{MainProcessorTask, RealTimeProcessorMapping};
use helgoboss_learn::MidiSourceValue;
use helgoboss_midi::{RawShortMessage, ShortMessage};

const BULK_SIZE: usize = 1;

#[derive(Debug)]
pub struct RealTimeProcessor {
    receiver: crossbeam_channel::Receiver<RealTimeProcessorTask>,
    main_processor_sender: crossbeam_channel::Sender<MainProcessorTask>,
    mappings: Vec<RealTimeProcessorMapping>,
    let_matched_events_through: bool,
    let_unmatched_events_through: bool,
}

impl RealTimeProcessor {
    pub fn new(
        receiver: crossbeam_channel::Receiver<RealTimeProcessorTask>,
        main_processor_sender: crossbeam_channel::Sender<MainProcessorTask>,
    ) -> RealTimeProcessor {
        RealTimeProcessor {
            receiver,
            main_processor_sender: main_processor_sender,
            mappings: vec![],
            let_matched_events_through: false,
            let_unmatched_events_through: false,
        }
    }

    pub fn process_short_from_fx_input(&self, msg: impl ShortMessage) {
        // TODO-high Only process if dev not set
        self.process_short(msg);
    }

    /// Should be called regularly in real-time audio thread.
    pub fn idle(&mut self) {
        for task in self.receiver.try_iter().take(BULK_SIZE) {
            use RealTimeProcessorTask::*;
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

    fn process_short(&self, msg: impl ShortMessage) {
        // TODO Also handle (N)RPN, 14-bit messages, MIDI clock etc.
        self.process(&MidiSourceValue::Plain(msg));
    }

    fn process(&self, value: &MidiSourceValue<impl ShortMessage>) -> bool {
        let mut processed = false;
        for m in &self.mappings {
            if let Some(control_value) = m.source.control(&value) {
                let main_processor_task = MainProcessorTask::Control {
                    mapping_id: m.mapping_id,
                    value: control_value,
                };
                self.main_processor_sender.send(main_processor_task);
                processed = true;
            }
        }
        processed
    }
}

#[derive(Debug)]
pub enum RealTimeProcessorTask {
    UpdateMappings(Vec<RealTimeProcessorMapping>),
    UpdateFlags {
        let_matched_events_through: bool,
        let_unmatched_events_through: bool,
    },
}
