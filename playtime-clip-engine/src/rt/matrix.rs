use crate::main;
use crate::main::MainMatrixCommandSender;
use crate::rt::WeakColumnSource;
use crossbeam_channel::{Receiver, Sender};

#[derive(Debug)]
pub struct Matrix {
    columns: Vec<WeakColumnSource>,
    command_receiver: Receiver<MatrixCommand>,
    main_command_sender: Sender<main::MatrixCommand>,
}

impl Matrix {
    pub fn new(
        command_receiver: Receiver<MatrixCommand>,
        command_sender: Sender<main::MatrixCommand>,
    ) -> Self {
        Self {
            // TODO-high Choose capacity wisely
            columns: Vec::with_capacity(500),
            command_receiver,
            main_command_sender: command_sender,
        }
    }

    pub fn poll(&mut self) {
        while let Ok(command) = self.command_receiver.try_recv() {
            use MatrixCommand::*;
            match command {
                InsertColumn(index, source) => {
                    self.columns.insert(index, source);
                }
                RemoveColumn(index) => {
                    let column = self.columns.remove(index);
                    self.main_command_sender.throw_away(column);
                }
                Clear => {
                    for column in self.columns.drain(..) {
                        self.main_command_sender.throw_away(column);
                    }
                }
            }
        }
    }

    pub fn column(&self, index: usize) -> Option<&WeakColumnSource> {
        self.columns.get(index)
    }
}

pub enum MatrixCommand {
    InsertColumn(usize, WeakColumnSource),
    RemoveColumn(usize),
    Clear,
}

pub trait RtMatrixCommandSender {
    fn insert_column(&self, index: usize, source: WeakColumnSource);
    fn remove_column(&self, index: usize);
    fn clear(&self);
    fn send_command(&self, command: MatrixCommand);
}

impl RtMatrixCommandSender for Sender<MatrixCommand> {
    fn insert_column(&self, index: usize, source: WeakColumnSource) {
        self.send_command(MatrixCommand::InsertColumn(index, source));
    }

    fn remove_column(&self, index: usize) {
        self.send_command(MatrixCommand::RemoveColumn(index));
    }

    fn clear(&self) {
        self.send_command(MatrixCommand::Clear);
    }

    fn send_command(&self, command: MatrixCommand) {
        self.try_send(command).unwrap();
    }
}
