use crate::core::{prop, Prop};
use crate::domain::{CompoundMappingSource, MappingCompartment, ReaperTarget};

use crate::application::{GroupId, MappingModel};
use enum_map::{enum_map, EnumMap};
use rx_util::UnitEvent;
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
    pub group_filter: EnumMap<MappingCompartment, Prop<Option<GroupFilter>>>,
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
            group_filter: enum_map! {
                MappingCompartment::ControllerMappings => prop(Some(GroupFilter::default())),
                MappingCompartment::MainMappings => prop(Some(GroupFilter::default())),
            },
            search_expression: Default::default(),
            status_msg: Default::default(),
        }
    }
}

impl MainState {
    pub fn clear_all_filters(&mut self) {
        self.clear_all_filters_except_group();
        for c in MappingCompartment::enum_iter() {
            self.clear_group_filter(c);
        }
    }

    pub fn group_filter_for_any_compartment_changed(&self) -> impl UnitEvent {
        self.group_filter[MappingCompartment::ControllerMappings]
            .changed()
            .merge(self.group_filter[MappingCompartment::MainMappings].changed())
    }

    pub fn group_filter_for_active_compartment(&self) -> Option<GroupFilter> {
        self.group_filter[self.active_compartment.get()].get()
    }

    pub fn set_group_filter_for_active_compartment(&mut self, filter: Option<GroupFilter>) {
        self.group_filter[self.active_compartment.get()].set(filter);
    }

    pub fn clear_all_filters_except_group(&mut self) {
        self.clear_source_filter();
        self.clear_target_filter();
        self.clear_search_expression_filter();
        self.stop_filter_learning();
    }

    pub fn clear_group_filter(&mut self, compartment: MappingCompartment) {
        self.group_filter[compartment].set(None);
    }

    pub fn clear_group_filter_for_active_compartment(&mut self) {
        self.clear_group_filter(self.active_compartment.get());
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

    pub fn filter_is_active(&self) -> bool {
        self.group_filter[self.active_compartment.get()]
            .get_ref()
            .is_some()
            || self.filter_is_active_except_group()
    }

    pub fn filter_is_active_except_group(&self) -> bool {
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
