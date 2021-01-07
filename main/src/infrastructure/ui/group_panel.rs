use crate::core::{when, Prop};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::constants::symbols;
use crate::infrastructure::ui::{ItemProp, MainPanel, MappingHeaderPanel};

use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    AbsoluteMode, ControlValue, MidiClockTransportMessage, OutOfRangeBehavior,
    SoftSymmetricUnitValue, SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use reaper_high::Reaper;
use reaper_low::raw;
use reaper_medium::{InitialAction, PromptForActionResult, SectionId};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::convert::TryInto;

use std::iter;

use std::ptr::null;
use std::rc::Rc;

use crate::application::{
    convert_factor_to_unit_value, convert_unit_value_to_factor, get_fx_label, get_fx_param_label,
    get_guid_based_fx_at_index, get_optional_fx_label, ActivationType, FxAnchorType, MappingModel,
    MidiSourceType, ModeModel, ModifierConditionModel, ReaperTargetType, Session, SharedMapping,
    SharedSession, SourceCategory, SourceModel, TargetCategory, TargetModel,
    TargetModelWithContext, TrackAnchorType, VirtualControlElementType, WeakGroup, WeakSession,
};
use crate::core::Global;
use crate::domain::{
    ActionInvocationType, CompoundMappingTarget, FxAnchor, MappingCompartment, MappingId,
    ProcessorContext, RealearnTarget, ReaperTarget, TargetCharacter, TrackAnchor, TransportAction,
    VirtualControlElement, VirtualFx, VirtualTrack, PLUGIN_PARAMETER_COUNT,
};
use itertools::Itertools;
use std::collections::HashMap;
use std::time::Duration;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, WeakView, Window};

#[derive(Debug)]
pub struct GroupPanel {
    view: ViewContext,
    session: WeakSession,
    group: WeakGroup,
    mapping_header_panel: SharedView<MappingHeaderPanel>,
}

impl GroupPanel {
    pub fn new(session: WeakSession, group: WeakGroup) -> GroupPanel {
        GroupPanel {
            view: Default::default(),
            session: session.clone(),
            group: group.clone(),
            mapping_header_panel: SharedView::new(MappingHeaderPanel::new(
                session,
                Point::new(DialogUnits(2), DialogUnits(2)),
                Some(group),
            )),
        }
    }

    fn register_listeners(self: Rc<Self>) {
        let group = self.group.upgrade().expect("group gone");
        let group = group.borrow();
        self.when(group.name.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::Name);
        });
        self.when(group.control_is_enabled.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::ControlEnabled);
        });
        self.when(group.feedback_is_enabled.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::FeedbackEnabled);
        });
        self.when(group.activation_type.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::ActivationType);
        });
        self.when(group.modifier_condition_1.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::ModifierCondition1);
        });
        self.when(group.modifier_condition_2.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::ModifierCondition2);
        });
        self.when(group.program_condition.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::ProgramCondition);
        });
        self.when(group.eel_condition.changed(), |view| {
            view.mapping_header_panel
                .invalidate_due_to_changed_prop(ItemProp::EelCondition);
        });
    }

    fn when(self: &Rc<Self>, event: impl UnitEvent, reaction: impl Fn(Rc<Self>) + 'static + Copy) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, _| reaction(panel));
    }
}

impl View for GroupPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_GROUP_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        self.mapping_header_panel.clone().open(window);
        self.register_listeners();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            // IDCANCEL is escape button
            ID_GROUP_PANEL_OK | raw::IDCANCEL => {
                self.close();
            }
            _ => unreachable!(),
        }
    }
}
