use crate::ClipEngineResult;
use playtime_api as api;

/// Data structure holding the undo history.
#[derive(Debug, Default)]
pub struct History {
    undo_stack: Vec<State>,
    redo_stack: Vec<State>,
}

impl History {
    /// Clears the complete history.
    pub fn clear(&mut self) {
        self.redo_stack.clear();
        self.undo_stack.clear();
    }

    /// Returns if undo is possible.
    pub fn can_undo(&self) -> bool {
        self.undo_stack.len() > 1
    }

    /// Returns if redo is possible.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Adds the given history entry if the matrix is different from the one in the previous
    /// undo point.
    pub fn add(&mut self, label: String, new_matrix: api::Matrix) {
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
