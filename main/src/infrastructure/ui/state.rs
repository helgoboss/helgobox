use crate::core::{prop, Prop};
use crate::domain::{CompoundMappingSource, MappingCompartment, ReaperTarget};

use crate::application::{GroupId, MappingModel};
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
    pub group_filter: Prop<Option<GroupFilter>>,
    pub search_expression: Prop<String>,
    pub status_msg: Prop<String>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GroupFilter(pub GroupId);

impl GroupFilter {
    pub fn matches(&self, mapping: &MappingModel) -> bool {
        mapping.group_id.get() == self.0
    }

    pub fn group_id(&self) -> GroupId {
        self.0
    }
}

impl Default for MainState {
    fn default() -> Self {
        MainState {
            target_filter: prop(None),
            is_learning_target_filter: prop(false),
            source_filter: prop(None),
            is_learning_source_filter: prop(false),
            active_compartment: prop(MappingCompartment::MainMappings),
            group_filter: prop(Some(GroupFilter(GroupId::default()))),
            search_expression: Default::default(),
            status_msg: Default::default(),
        }
    }
}

impl MainState {
    pub fn clear_all_filters(&mut self) {
        self.clear_all_filters_except_group();
        self.clear_group_filter();
    }

    pub fn clear_all_filters_except_group(&mut self) {
        self.clear_source_filter();
        self.clear_target_filter();
        self.clear_search_expression_filter();
        self.stop_filter_learning();
    }

    pub fn clear_group_filter(&mut self) {
        self.group_filter.set(None);
    }

    pub fn clear_search_expression_filter(&mut self) {
        self.search_expression.set("".to_string());
    }

    pub fn clear_source_filter(&mut self) {
        self.source_filter.set(None)
    }

    pub fn clear_target_filter(&mut self) {
        self.target_filter.set(None)
    }

    pub fn filter_is_active(&self) -> bool {
        self.group_filter.get_ref().is_some()
            || self.source_filter.get_ref().is_some()
            || self.target_filter.get_ref().is_some()
            || !self.search_expression.get_ref().trim().is_empty()
    }

    pub fn stop_filter_learning(&mut self) {
        self.is_learning_source_filter.set(false);
        self.is_learning_target_filter.set(false);
    }
}
