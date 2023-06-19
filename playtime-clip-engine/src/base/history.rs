use crate::ClipEngineResult;
use itertools::Itertools;
use playtime_api::persistence as api;
use reaper_high::{Guid, Project, Track};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Debug;
use std::mem;
use std::time::{Duration, Instant};

/// Data structure holding the undo history.
#[derive(Debug)]
pub struct History {
    history_entry_buffer: HistoryEntryBuffer,
    undo_stack: Vec<HistoryState>,
    redo_stack: Vec<HistoryState>,
}

#[derive(Debug)]
struct HistoryEntryBuffer {
    time_of_last_addition: Instant,
    content: HistoryEntryBufferContent,
}

#[derive(Debug, Default)]
struct HistoryEntryBufferContent {
    labels: BTreeSet<String>,
    reaper_changes: Vec<BoxedReaperChange>,
}

impl HistoryEntryBufferContent {
    pub fn summary_label(&self) -> String {
        self.labels.iter().join(", ")
    }
}

impl HistoryEntryBuffer {
    pub fn add(&mut self, label: String, reaper_changes: Vec<BoxedReaperChange>) {
        self.time_of_last_addition = Instant::now();
        self.content.labels.insert(label);
        self.content.reaper_changes.extend(reaper_changes);
    }

    pub fn its_flush_time(&self) -> bool {
        const DEBOUNCE_DURATION: Duration = Duration::from_millis(1000);
        !self.is_empty() && self.time_of_last_addition.elapsed() > DEBOUNCE_DURATION
    }

    pub fn is_empty(&self) -> bool {
        self.content.labels.is_empty()
    }

    pub fn flush(&mut self) -> HistoryEntryBufferContent {
        HistoryEntryBufferContent {
            labels: mem::take(&mut self.content.labels),
            reaper_changes: mem::take(&mut self.content.reaper_changes),
        }
    }
}

impl Default for HistoryEntryBuffer {
    fn default() -> Self {
        Self {
            time_of_last_addition: Instant::now(),
            content: Default::default(),
        }
    }
}

impl History {
    pub fn new(initial_matrix: api::Matrix) -> Self {
        Self {
            history_entry_buffer: Default::default(),
            undo_stack: vec![HistoryState::new(
                "Initial".to_string(),
                initial_matrix,
                vec![],
            )],
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
    pub fn add_buffered(&mut self, label: String, reaper_changes: Vec<BoxedReaperChange>) {
        self.history_entry_buffer.add(label, reaper_changes);
    }

    /// Adds the buffered history entries if the matrix is different from the one in the previous
    /// undo point.
    pub fn flush_buffer(&mut self, new_matrix: api::Matrix) {
        let content = self.history_entry_buffer.flush();
        let label = content.summary_label();
        self.add_internal(label, new_matrix, content.reaper_changes);
    }

    /// Adds the given history entry if the matrix is different from the one in the previous
    /// undo point.
    pub fn add(
        &mut self,
        label: String,
        new_matrix: api::Matrix,
        reaper_changes: Vec<BoxedReaperChange>,
    ) {
        let label = if self.history_entry_buffer.is_empty() {
            label
        } else {
            let mut content = self.history_entry_buffer.flush();
            content.labels.insert(label);
            content.summary_label()
        };
        self.add_internal(label, new_matrix, reaper_changes);
    }

    fn add_internal(
        &mut self,
        label: String,
        new_matrix: api::Matrix,
        reaper_changes: Vec<BoxedReaperChange>,
    ) {
        if let Some(prev_state) = self.undo_stack.last() {
            if new_matrix == prev_state.matrix {
                return;
            }
        };
        self.redo_stack.clear();
        let new_state = HistoryState::new(label, new_matrix, reaper_changes);
        self.undo_stack.push(new_state);
    }

    /// Marks the last action as undone and returns the matrix state to be loaded.
    pub fn undo(&mut self) -> ClipEngineResult<&HistoryState> {
        if self.undo_stack.len() <= 1 {
            return Err("nothing to undo");
        }
        let state = self.undo_stack.pop().unwrap();
        self.redo_stack.push(state);
        Ok(&self.undo_stack.last().unwrap())
    }

    /// Marks the last undone action as redone and returns the matrix state to be loaded.
    pub fn redo(&mut self) -> ClipEngineResult<&HistoryState> {
        let state = self.redo_stack.pop().ok_or("nothing to redo")?;
        self.undo_stack.push(state);
        Ok(&self.undo_stack.last().unwrap())
    }
}

#[derive(Debug)]
pub struct HistoryState {
    pub label: String,
    pub matrix: api::Matrix,
    pub reaper_changes: Vec<BoxedReaperChange>,
}

pub type BoxedReaperChange = Box<dyn ReaperChange>;

pub trait ReaperChange: Debug {
    /// Executed before undoing the clip matrix change.
    fn pre_undo(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    /// Executed after undoing the clip matrix change.
    fn post_undo(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    /// Executed before redoing the clip matrix change.
    fn pre_redo(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    /// Executed after redoing the clip matrix change.
    fn post_redo(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl HistoryState {
    fn new(label: String, matrix: api::Matrix, reaper_changes: Vec<BoxedReaperChange>) -> Self {
        Self {
            label,
            matrix,
            reaper_changes,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AddedTrack {
    project: Project,
    guid: Guid,
    index: u32,
}

impl AddedTrack {
    pub fn new(track: &Track) -> Self {
        Self {
            project: track.project(),
            guid: track.guid().clone(),
            index: track.index().expect("added track doesn't have index"),
        }
    }
}

impl ReaperChange for AddedTrack {
    fn pre_redo(&self) -> Result<(), Box<dyn Error>> {
        let track = self.project.insert_track_at(self.index)?;
        // self.guid = track.guid().clone();
        Ok(())
    }

    fn post_undo(&self) -> Result<(), Box<dyn Error>> {
        let track = self.project.track_by_guid(&self.guid)?;
        if !track.is_available() {
            // Track doesn't exist anymore, so no need to remove it
            return Ok(());
        }
        self.project.remove_track(&track);
        Ok(())
    }
}
