use crate::ColumnSourceCommandSender;
use crossbeam_channel::Receiver;

#[derive(Debug)]
pub struct RealTimeClipMatrix {
    columns: Vec<ColumnSourceCommandSender>,
    command_receiver: Receiver<RealTimeClipMatrixCommand>,
}

impl RealTimeClipMatrix {
    pub fn new(command_receiver: Receiver<RealTimeClipMatrixCommand>) -> Self {
        Self {
            // TODO-high Choose capacity wisely
            columns: Vec::with_capacity(500),
            command_receiver,
        }
    }

    pub fn poll(&mut self) {
        while let Ok(command) = self.command_receiver.try_recv() {
            use RealTimeClipMatrixCommand::*;
            match command {
                InsertColumn(index, sender) => {
                    self.columns.insert(index, sender);
                }
                RemoveColumn(index) => {
                    self.columns.remove(index);
                }
                Clear => {
                    self.columns.clear();
                }
            }
        }
    }

    pub fn column(&self, index: usize) -> Option<&ColumnSourceCommandSender> {
        self.columns.get(index)
    }
}

pub enum RealTimeClipMatrixCommand {
    InsertColumn(usize, ColumnSourceCommandSender),
    RemoveColumn(usize),
    Clear,
}
