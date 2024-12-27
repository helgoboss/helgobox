use crate::application::{LearnManySubState, SharedUnitModel, WeakUnitModel};
use crate::base::when;
use crate::domain::CompartmentKind;
use crate::infrastructure::ui::bindings::root;
use helgobox_api::persistence::VirtualControlElementCharacter;
use reaper_low::raw;
use rxrust::prelude::*;
use std::rc::Rc;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct SessionMessagePanel {
    view: ViewContext,
    session: WeakUnitModel,
}

impl SessionMessagePanel {
    pub fn new(session: WeakUnitModel) -> SessionMessagePanel {
        SessionMessagePanel {
            view: Default::default(),
            session,
        }
    }

    fn invalidate_controls(&self) {
        self.invalidate_message_and_title();
    }

    fn invalidate_message_and_title(&self) {
        let session = self.session();
        let session = session.borrow();
        let (title_addition, msg) = if let Some(state) = session.learn_many_state() {
            if let Some(mapping) =
                session.find_mapping_by_id(state.compartment, state.current_mapping_id)
            {
                let mapping = mapping.borrow();
                let mapping_label = format!("mapping {}", mapping.effective_name());
                match state.sub_state {
                    LearnManySubState::LearningSource {
                        control_element_character,
                    } => {
                        let msg = match state.compartment {
                            CompartmentKind::Controller => match control_element_character {
                                VirtualControlElementCharacter::Multi => {
                                    "Move a multi-like control element!"
                                }
                                VirtualControlElementCharacter::Button => {
                                    "Press a button-like control element!"
                                }
                            },
                            CompartmentKind::Main => "Touch a control element!",
                        };
                        (
                            format!("Learning source for {mapping_label}"),
                            msg.to_string(),
                        )
                    }
                    LearnManySubState::LearningTarget => (
                        format!("Learning target for {mapping_label}"),
                        "Now touch the target which you want to control!".to_string(),
                    ),
                }
            } else {
                ("".to_string(), "".to_string())
            }
        } else {
            ("".to_string(), "".to_string())
        };
        self.view
            .require_window()
            .set_text(format!("ReaLearn - {title_addition}"));
        self.view
            .require_control(root::ID_MESSAGE_TEXT)
            .set_text(msg);
    }

    fn register_listeners(self: &SharedView<Self>) {
        let session = self.session();
        let session = session.borrow();
        when(
            session
                .learn_many_state_changed()
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(self))
        .do_async(|view, _| {
            view.invalidate_message_and_title();
        });
    }

    fn session(&self) -> SharedUnitModel {
        self.session.upgrade().expect("session gone")
    }
}

impl View for SessionMessagePanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MESSAGE_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.invalidate_controls();
        self.register_listeners();
        true
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        if let Some(session) = self.session.upgrade() {
            session.borrow_mut().stop_learning_many_mappings();
        }
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }
}
