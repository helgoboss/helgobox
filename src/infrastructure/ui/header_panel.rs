use crate::domain::Session;
use crate::infrastructure::common::bindings::root;
use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use reaper_medium::ReaperFunctions;
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use swell_ui::{define_control_methods, View, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    session: Rc<RefCell<Session<'static>>>,
    window: Cell<Option<Window>>,
}

impl HeaderPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> HeaderPanel {
        HeaderPanel {
            session,
            window: None.into(),
        }
    }

    // TODO Remove
    fn change_text(&self, text: &str) {
        self.window
            .get()
            .expect("header panel doesn't have a window")
            .find_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
            .expect("send-feedback-only-if-armed check box not found")
            .set_text(text);
    }
}

define_control_methods!(HeaderPanel, [
    let_matched_events_through_check_box => root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX,
    let_unmatched_events_through_check_box => root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX,
    send_feedback_only_if_armed_check_box => root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX,
    always_auto_detect_check_box => root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX,
]);

impl HeaderPanel {
    // // TODO Remove
    // fn setup_change_listener(self: Rc<Self>) {
    //     Reaper::get().show_console_msg(c_str!("Opened header ui\n"));
    //     let weak_self = Rc::downgrade(&self);
    //     self.session
    //         .borrow_mut()
    //         .get_dummy_source_model()
    //         .changed()
    //         .subscribe(move |_| {
    //             println!("Dummy source model changed");
    //             weak_self
    //                 .upgrade()
    //                 .expect("header panel not existing anymore")
    //                 .change_text("test");
    //         });
    // }

    fn learn_source_filter(&self) {
        todo!()
    }

    fn learn_target_filter(&self) {
        todo!()
    }

    fn clear_source_filter(&self) {
        todo!()
    }

    fn clear_target_filter(&self) {
        todo!()
    }

    fn update_let_matched_events_through(&self) {
        self.session.borrow_mut().let_matched_events_through.set(
            self.require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_let_unmatched_events_through(&self) {
        self.session
            .borrow_mut()
            .let_unmatched_events_through
            .set(self.let_unmatched_events_through_check_box().is_checked());
    }

    fn update_send_feedback_only_if_armed(&self) {
        self.session
            .borrow_mut()
            .send_feedback_only_if_armed
            .set(self.send_feedback_only_if_armed_check_box().is_checked());
    }

    fn update_always_auto_detect(&self) {
        self.session
            .borrow_mut()
            .always_auto_detect
            .set(self.always_auto_detect_check_box().is_checked());
    }

    fn update_control_device(&self) {
        todo!()
    }

    fn update_feedback_device(&self) {
        todo!()
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPINGS_DIALOG
    }

    fn window(&self) -> &Cell<Option<Window>> {
        &self.window
    }

    fn opened(self: Rc<Self>, window: Window) -> bool {
        false
    }

    fn button_clicked(self: Rc<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_ADD_MAPPING_BUTTON => self.session.borrow_mut().add_default_mapping(),
            ID_FILTER_BY_SOURCE_BUTTON => self.learn_source_filter(),
            ID_FILTER_BY_TARGET_BUTTON => self.learn_target_filter(),
            ID_CLEAR_SOURCE_FILTER_BUTTON => self.clear_source_filter(),
            ID_CLEAR_TARGET_FILTER_BUTTON => self.clear_target_filter(),
            ID_IMPORT_BUTTON => self.session.borrow_mut().import_from_clipboard(),
            ID_EXPORT_BUTTON => self.session.borrow().export_to_clipboard(),
            ID_SEND_FEEDBACK_BUTTON => self.session.borrow().send_feedback(),
            ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX => self.update_let_matched_events_through(),
            ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX => self.update_let_unmatched_events_through(),
            ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX => self.update_send_feedback_only_if_armed(),
            ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX => self.update_always_auto_detect(),
            _ => {}
        }
    }

    fn option_selected(self: Rc<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_CONTROL_DEVICE_COMBO_BOX => self.update_control_device(),
            ID_FEEDBACK_DEVICE_COMBO_BOX => self.update_feedback_device(),
            _ => {}
        }
    }
}
