use crate::{ColumnSourceCommandSender, SharedColumnSource, WeakColumnSource};
use crossbeam_channel::Receiver;

#[derive(Debug)]
pub struct RealTimeClipMatrix {
    column: Vec<WeakColumnSource>,
    command_receiver: Receiver<RealTimeClipMatrixCommand>,
}

impl RealTimeClipMatrix {
    pub fn new(command_receiver: Receiver<RealTimeClipMatrixCommand>) -> Self {
        Self {
            // TODO-high Choose capacity wisely
            column: Vec::with_capacity(500),
            command_receiver,
        }
    }

    pub fn poll(&mut self) {
        while let Ok(command) = self.command_receiver.try_recv() {
            use RealTimeClipMatrixCommand::*;
            match command {
                InsertColumn(index, source) => {
                    self.column.insert(index, source);
                }
                RemoveColumn(index) => {
                    self.column.remove(index);
                }
                Clear => {
                    self.column.clear();
                }
            }
        }
    }

    pub fn column(&self, index: usize) -> Option<&WeakColumnSource> {
        self.column.get(index)
    }
}

pub enum RealTimeClipMatrixCommand {
    InsertColumn(usize, WeakColumnSource),
    RemoveColumn(usize),
    Clear,
}
