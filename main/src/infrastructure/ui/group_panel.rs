use crate::application::{WeakGroup, WeakSession};
use crate::core::when;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::{ItemProp, MappingHeaderPanel};
use reaper_low::raw;
use rx_util::UnitEvent;
use std::rc::Rc;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

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
        self.when(
            group.activation_condition_model.activation_type.changed(),
            |view| {
                view.mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ActivationType);
            },
        );
        self.when(
            group
                .activation_condition_model
                .modifier_condition_1
                .changed(),
            |view| {
                view.mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ModifierCondition1);
            },
        );
        self.when(
            group
                .activation_condition_model
                .modifier_condition_2
                .changed(),
            |view| {
                view.mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ModifierCondition2);
            },
        );
        self.when(
            group.activation_condition_model.bank_condition.changed(),
            |view| {
                view.mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::BankCondition);
            },
        );
        self.when(
            group.activation_condition_model.eel_condition.changed(),
            |view| {
                view.mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::EelCondition);
            },
        );
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
