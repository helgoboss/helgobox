use crate::application::{CompartmentProp, GroupProp, SessionProp, WeakGroup, WeakSession};
use crate::base::when;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::{ItemProp, MappingHeaderPanel};
use reaper_low::raw;
use rxrust::prelude::*;
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

    pub fn handle_session_prop_change(
        self: SharedView<Self>,
        prop: SessionProp,
        initiator: Option<u32>,
    ) {
        // If the reaction can't be displayed anymore because the mapping is not filled anymore,
        // so what.
        match prop {
            SessionProp::CompartmentProp(_, prop) => match prop {
                CompartmentProp::GroupProp(_, prop) => {
                    use GroupProp as P;
                    match prop {
                        P::Name => {
                            self.mapping_header_panel
                                .invalidate_due_to_changed_prop(ItemProp::Name, initiator);
                        }
                        P::Tags => {
                            self.mapping_header_panel
                                .invalidate_due_to_changed_prop(ItemProp::Tags, initiator);
                        }
                        P::ControlIsEnabled => {
                            self.mapping_header_panel.invalidate_due_to_changed_prop(
                                ItemProp::ControlEnabled,
                                initiator,
                            );
                        }
                        P::FeedbackIsEnabled => {
                            self.mapping_header_panel.invalidate_due_to_changed_prop(
                                ItemProp::FeedbackEnabled,
                                initiator,
                            );
                        }
                        P::ActivationConditionProp(p) => {
                            let item_prop = ItemProp::from_activation_condition_prop(p);
                            self.mapping_header_panel
                                .invalidate_due_to_changed_prop(item_prop, initiator);
                        }
                    }
                }
                CompartmentProp::MappingProp(_, _) => {}
            },
        }
    }

    fn when<I: Send + Sync + Clone + 'static>(
        self: &Rc<Self>,
        event: impl LocalObservable<'static, Item = I, Err = ()> + 'static,
        reaction: impl Fn(Rc<Self>, I) + 'static + Copy,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, item| reaction(panel, item));
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
