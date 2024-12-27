use crate::application::{
    Affected, CompartmentProp, GroupProp, SessionProp, WeakGroup, WeakUnitModel,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::MAPPING_PANEL_SCALING;
use crate::infrastructure::ui::{ItemProp, MappingHeaderPanel};
use reaper_low::raw;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct GroupPanel {
    view: ViewContext,
    mapping_header_panel: SharedView<MappingHeaderPanel>,
}

impl GroupPanel {
    pub fn new(session: WeakUnitModel, group: WeakGroup) -> GroupPanel {
        GroupPanel {
            view: Default::default(),
            mapping_header_panel: SharedView::new(MappingHeaderPanel::new(
                session,
                Point::new(DialogUnits(7), DialogUnits(5)).scale(&MAPPING_PANEL_SCALING),
                Some(group),
            )),
        }
    }

    pub fn handle_affected(
        self: &SharedView<Self>,
        affected: &Affected<SessionProp>,
        initiator: Option<u32>,
    ) {
        use Affected::*;
        use CompartmentProp::*;
        use SessionProp::*;
        match affected {
            One(InCompartment(_, One(InGroup(_, affected)))) => match affected {
                Multiple => {
                    self.mapping_header_panel.invalidate_controls();
                }
                One(prop) => {
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
                        P::InActivationCondition(p) => match p {
                            Multiple => {
                                self.mapping_header_panel.invalidate_controls();
                            }
                            One(p) => {
                                let item_prop = ItemProp::from_activation_condition_prop(p);
                                self.mapping_header_panel
                                    .invalidate_due_to_changed_prop(item_prop, initiator);
                            }
                        },
                    }
                }
            },
            _ => {}
        }
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
            _ => {}
        }
    }
}
