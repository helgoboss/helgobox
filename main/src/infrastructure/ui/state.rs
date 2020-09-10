use crate::core::Prop;
use crate::domain::{NormalMappingSource, ReaperTarget};
use helgoboss_learn::MidiSource;
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedMainState = Rc<RefCell<MainState>>;

#[derive(Debug, Default)]
pub struct MainState {
    pub target_filter: Prop<Option<ReaperTarget>>,
    pub is_learning_target_filter: Prop<bool>,
    pub source_filter: Prop<Option<NormalMappingSource>>,
    pub is_learning_source_filter: Prop<bool>,
    pub search_expression: Prop<String>,
}

impl MainState {
    pub fn clear_filters(&mut self) {
        self.clear_source_filter();
        self.clear_target_filter();
    }

    pub fn clear_source_filter(&mut self) {
        self.source_filter.set(None)
    }

    pub fn clear_target_filter(&mut self) {
        self.target_filter.set(None)
    }
}
