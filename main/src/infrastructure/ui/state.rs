use crate::core::{prop, Prop};
use crate::domain::{CompoundMappingSource, MappingCompartment, ReaperTarget};

use crate::application::{GroupId, MappingModel};
use enum_map::{enum_map, EnumMap};
use rxrust::prelude::*;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use wildmatch::WildMatch;

pub type SharedMainState = Rc<RefCell<MainState>>;

#[derive(Debug)]
pub struct MainState {
    pub target_filter: Prop<Option<ReaperTarget>>,
    pub is_learning_target_filter: Prop<bool>,
    pub source_filter: Prop<Option<CompoundMappingSource>>,
    pub is_learning_source_filter: Prop<bool>,
    pub active_compartment: Prop<MappingCompartment>,
    pub displayed_group: EnumMap<MappingCompartment, Prop<Option<GroupFilter>>>,
    pub search_expression: Prop<SearchExpression>,
    pub status_msg: Prop<String>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
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
            displayed_group: enum_map! {
                MappingCompartment::ControllerMappings => prop(Some(GroupFilter::default())),
                MappingCompartment::MainMappings => prop(Some(GroupFilter::default())),
            },
            search_expression: Default::default(),
            status_msg: Default::default(),
        }
    }
}

impl MainState {
    pub fn clear_all_filters_and_displayed_group(&mut self) {
        self.clear_all_filters();
        for c in MappingCompartment::enum_iter() {
            self.clear_displayed_group(c);
        }
    }

    pub fn displayed_group_for_any_compartment_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.displayed_group[MappingCompartment::ControllerMappings]
            .changed()
            .merge(self.displayed_group[MappingCompartment::MainMappings].changed())
    }

    pub fn displayed_group_for_active_compartment(&self) -> Option<GroupFilter> {
        self.displayed_group[self.active_compartment.get()].get()
    }

    pub fn set_displayed_group_for_active_compartment(&mut self, filter: Option<GroupFilter>) {
        self.displayed_group[self.active_compartment.get()].set(filter);
    }

    pub fn clear_all_filters(&mut self) {
        self.clear_source_filter();
        self.clear_target_filter();
        self.clear_search_expression_filter();
        self.stop_filter_learning();
    }

    pub fn clear_displayed_group(&mut self, compartment: MappingCompartment) {
        self.displayed_group[compartment].set(None);
    }

    pub fn clear_displayed_group_for_active_compartment(&mut self) {
        self.clear_displayed_group(self.active_compartment.get());
    }

    pub fn clear_search_expression_filter(&mut self) {
        self.search_expression.set(Default::default());
    }

    pub fn clear_source_filter(&mut self) {
        self.source_filter.set(None)
    }

    pub fn clear_target_filter(&mut self) {
        self.target_filter.set(None)
    }

    pub fn filter_and_displayed_group_is_active(&self) -> bool {
        self.displayed_group[self.active_compartment.get()]
            .get_ref()
            .is_some()
            || self.filter_is_active()
    }

    pub fn filter_is_active(&self) -> bool {
        self.source_filter.get_ref().is_some()
            || self.target_filter.get_ref().is_some()
            || !self.search_expression.get_ref().is_empty()
    }

    pub fn stop_filter_learning(&mut self) {
        self.is_learning_source_filter.set(false);
        self.is_learning_target_filter.set(false);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchExpression(WildMatch);

impl SearchExpression {
    pub fn new(text: &str) -> SearchExpression {
        let wild_match = if text.is_empty() {
            WildMatch::default()
        } else {
            let modified_text = format!("*{}*", text.to_lowercase());
            WildMatch::new(&modified_text)
        };
        Self(wild_match)
    }

    pub fn matches(&self, text: &str) -> bool {
        self.0.matches(&text.to_lowercase())
    }

    pub fn is_empty(&self) -> bool {
        self.to_string().is_empty()
    }
}

impl fmt::Display for SearchExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0.to_string();
        let s = s.strip_prefix('*').unwrap_or(&s);
        let s = s.strip_suffix('*').unwrap_or(&s);
        f.write_str(s)
    }
}
