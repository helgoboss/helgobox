use crate::base::{prop, Prop};
use crate::domain::{
    Compartment, CompoundMappingSource, GroupId, IncomingCompoundSourceValue, MessageCaptureResult,
    ReaperTarget, Tag, VirtualSourceValue,
};

use crate::application::{MappingModel, Session};
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
    pub source_filter: Prop<Option<SourceFilter>>,
    pub is_learning_source_filter: Prop<bool>,
    pub active_compartment: Prop<Compartment>,
    pub displayed_group: EnumMap<Compartment, Prop<Option<GroupFilter>>>,
    pub search_expression: Prop<SearchExpression>,
    pub scroll_status: Prop<ScrollStatus>,
}

#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct ScrollStatus {
    pub from_pos: usize,
    pub to_pos: usize,
    pub item_count: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SourceFilter {
    /// Only can match mappings with real sources.
    pub message_capture_result: MessageCaptureResult,
    /// If the incoming message was successfully virtualized
    /// (can match main mappings with virtual sources).
    pub virtual_source_value: Option<VirtualSourceValue>,
}

impl SourceFilter {
    pub fn matches(&self, source: &CompoundMappingSource) -> bool {
        // First try real source matching.
        if source
            .reacts_to_source_value_with(self.message_capture_result.message())
            .is_some()
        {
            return true;
        }
        // Then try virtual source matching (if the message was virtualized before).
        if let Some(v) = self.virtual_source_value {
            if source
                .reacts_to_source_value_with(IncomingCompoundSourceValue::Virtual(&v))
                .is_some()
            {
                return true;
            }
        }
        false
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct GroupFilter(pub GroupId);

impl GroupFilter {
    pub fn matches(&self, mapping: &MappingModel) -> bool {
        mapping.group_id() == self.0
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
            active_compartment: prop(Compartment::Main),
            displayed_group: enum_map! {
                Compartment::Controller => prop(Some(GroupFilter::default())),
                Compartment::Main => prop(Some(GroupFilter::default())),
            },
            search_expression: Default::default(),
            scroll_status: Default::default(),
        }
    }
}

impl MainState {
    pub fn clear_all_filters_and_displayed_group(&mut self) {
        self.clear_all_filters();
        for c in Compartment::enum_iter() {
            self.clear_displayed_group(c);
        }
    }

    pub fn displayed_group_for_any_compartment_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.displayed_group[Compartment::Controller]
            .changed()
            .merge(self.displayed_group[Compartment::Main].changed())
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

    pub fn clear_displayed_group(&mut self, compartment: Compartment) {
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
pub struct SearchExpression {
    wild_match: WildMatch,
    tag: Option<Tag>,
}

impl SearchExpression {
    pub fn new(text: &str) -> SearchExpression {
        let wild_match = if text.is_empty() {
            WildMatch::default()
        } else {
            let modified_text = format!("*{}*", text.to_lowercase());
            WildMatch::new(&modified_text)
        };
        fn extract_tag(text: &str) -> Option<Tag> {
            let tag_name = text.strip_prefix('#')?;
            tag_name.parse().ok()
        }
        Self {
            wild_match,
            tag: extract_tag(text),
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        self.wild_match.matches(&text.to_lowercase())
    }

    pub fn matches_any_tag_in_group(&self, mapping: &MappingModel, session: &Session) -> bool {
        if self.tag.is_none() {
            return false;
        }
        if let Some(group) = session
            .find_group_by_id_including_default_group(mapping.compartment(), mapping.group_id())
        {
            self.matches_any_tag(group.borrow().tags())
        } else {
            false
        }
    }

    pub fn matches_any_tag(&self, tags: &[Tag]) -> bool {
        if let Some(tag) = &self.tag {
            tags.iter().any(|t| t == tag)
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.to_string().is_empty()
    }
}

impl fmt::Display for SearchExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.wild_match.to_string();
        let s = s.strip_prefix('*').unwrap_or(&s);
        let s = s.strip_suffix('*').unwrap_or(s);
        f.write_str(s)
    }
}
