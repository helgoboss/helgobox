use crate::ClipEngineResult;
use itertools::Itertools;
use playtime_api::persistence as api;
use std::collections::HashSet;
use std::mem;
use std::time::{Duration, Instant};

/// Data structure holding the undo history.
#[derive(Debug)]
pub struct History {
    history_entry_buffer: HistoryEntryBuffer,
    undo_stack: Vec<State>,
    redo_stack: Vec<State>,
}

#[derive(Debug)]
struct HistoryEntryBuffer {
    time_of_last_addition: Instant,
    labels: HashSet<String>,
}

impl HistoryEntryBuffer {
    pub fn add(&mut self, label: String) {
        self.time_of_last_addition = Instant::now();
        self.labels.insert(label);
    }

    pub fn its_flush_time(&self) -> bool {
        const DEBOUNCE_DURATION: Duration = Duration::from_millis(1000);
        !self.labels.is_empty() && self.time_of_last_addition.elapsed() > DEBOUNCE_DURATION
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn flush(&mut self) -> HashSet<String> {
        mem::take(&mut self.labels)
    }
}

impl Default for HistoryEntryBuffer {
    fn default() -> Self {
        Self {
            time_of_last_addition: Instant::now(),
            labels: Default::default(),
        }
    }
}

impl History {
    pub fn new(initial_matrix: api::Matrix) -> Self {
        Self {
            history_entry_buffer: Default::default(),
            undo_stack: vec![State::new("Initial".to_string(), initial_matrix)],
            redo_stack: vec![],
        }
    }

    /// Returns the label of the next undoable action if there is one.
    pub fn next_undo_label(&self) -> Option<&str> {
        if !self.can_undo() {
            return None;
        }
        let state = self.undo_stack.last()?;
        Some(&state.label)
    }

    /// Returns the label of the next redoable action if there is one.
    pub fn next_redo_label(&self) -> Option<&str> {
        let state = self.redo_stack.last()?;
        Some(&state.label)
    }

    /// Returns if undo is possible.
    pub fn can_undo(&self) -> bool {
        self.undo_stack.len() > 1
    }

    /// Returns if redo is possible.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn its_flush_time(&self) -> bool {
        self.history_entry_buffer.its_flush_time()
    }

    /// Doesn't add a history event immediately but buffers it until no change has happened for
    /// some time.
    pub fn add_buffered(&mut self, label: String) {
        self.history_entry_buffer.add(label);
    }

    /// Adds the buffered history entries if the matrix is different from the one in the previous
    /// undo point.
    pub fn flush_buffer(&mut self, new_matrix: api::Matrix) {
        let label = build_multi_label(self.history_entry_buffer.flush());
        self.add_internal(label, new_matrix);
    }

    /// Adds the given history entry if the matrix is different from the one in the previous
    /// undo point.
    pub fn add(&mut self, label: String, new_matrix: api::Matrix) {
        let label = if self.history_entry_buffer.is_empty() {
            label
        } else {
            let mut labels = self.history_entry_buffer.flush();
            labels.insert(label);
            build_multi_label(labels)
        };
        self.add_internal(label, new_matrix);
    }

    fn add_internal(&mut self, label: String, new_matrix: api::Matrix) {
        if let Some(prev_state) = self.undo_stack.last() {
            if new_matrix == prev_state.matrix {
                return;
            }
        };
        self.redo_stack.clear();
        let new_state = State::new(label, new_matrix);
        self.undo_stack.push(new_state);
    }

    /// Marks the last action as undone and returns the matrix state to be loaded.
    pub fn undo(&mut self) -> ClipEngineResult<&api::Matrix> {
        if self.undo_stack.len() <= 1 {
            return Err("nothing to undo");
        }
        let state = self.undo_stack.pop().unwrap();
        self.redo_stack.push(state);
        Ok(&self.undo_stack.last().unwrap().matrix)
    }

    /// Marks the last undone action as redone and returns the matrix state to be loaded.
    pub fn redo(&mut self) -> ClipEngineResult<&api::Matrix> {
        let state = self.redo_stack.pop().ok_or("nothing to redo")?;
        self.undo_stack.push(state);
        Ok(&self.undo_stack.last().unwrap().matrix)
    }
}

// TODO-medium Make use of label
#[allow(dead_code)]
#[derive(Debug)]
struct State {
    label: String,
    matrix: api::Matrix,
}

impl State {
    fn new(label: String, matrix: api::Matrix) -> Self {
        Self { label, matrix }
    }
}

fn build_multi_label(labels: HashSet<String>) -> String {
    labels.iter().join(", ")
}
