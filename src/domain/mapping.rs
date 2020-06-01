use crate::domain::{Mode, ReaperTarget};
use helgoboss_learn::MidiSource;

#[derive(Debug)]
pub struct Mapping {
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
}

impl Mapping {
    pub fn new(source: MidiSource, mode: Mode, target: ReaperTarget) -> Mapping {
        Mapping {
            source,
            mode,
            target,
        }
    }
}
