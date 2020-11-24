use crate::core::{prop, Prop};
use crate::domain::{CompoundMappingSource, MappingCompartment, ReaperTarget};

use std::cell::RefCell;
use std::rc::Rc;

pub type SharedMainState = Rc<RefCell<MainState>>;

#[derive(Debug)]
pub struct MainState {
    pub target_filter: Prop<Option<ReaperTarget>>,
    pub is_learning_target_filter: Prop<bool>,
    pub source_filter: Prop<Option<CompoundMappingSource>>,
    pub is_learning_source_filter: Prop<bool>,
    pub active_compartment: Prop<MappingCompartment>,
    pub search_expression: Prop<String>,
    pub status_msg: Prop<String>,
}

impl Default for MainState {
    fn default() -> Self {
        MainState {
            target_filter: prop(None),
            is_learning_target_filter: prop(false),
            source_filter: prop(None),
            is_learning_source_filter: prop(false),
            active_compartment: prop(MappingCompartment::PrimaryMappings),
            search_expression: Default::default(),
            status_msg: Default::default(),
        }
    }
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
