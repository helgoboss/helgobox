use crate::domain::{MidiControlInput, Session};
use crate::infrastructure::common::bindings::root;
use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use reaper_medium::ReaperFunctions;
use rxrust::prelude::*;
use std::cell::{Cell, Ref, RefCell};
use std::rc::{Rc, Weak};
use swell_ui::{View, ViewContext, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    view_context: ViewContext,
    session: Rc<RefCell<Session<'static>>>,
}

impl HeaderPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> HeaderPanel {
        HeaderPanel {
            view_context: Default::default(),
            session,
        }
    }
}

impl HeaderPanel {
    fn session(&self) -> Ref<Session<'static>> {
        self.session.borrow()
    }

    fn learn_source_filter(&self) {
        // TODO
    }

    fn learn_target_filter(&self) {
        // TODO
    }

    fn clear_source_filter(&self) {
        // TODO
    }

    fn clear_target_filter(&self) {
        // TODO
    }

    fn update_let_matched_events_through(&self) {
        self.session.borrow_mut().let_matched_events_through.set(
            self.view_context()
                .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_let_unmatched_events_through(&self) {
        self.session.borrow_mut().let_unmatched_events_through.set(
            self.view_context()
                .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_send_feedback_only_if_armed(&self) {
        self.session.borrow_mut().send_feedback_only_if_armed.set(
            self.view_context()
                .require_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_always_auto_detect(&self) {
        self.session.borrow_mut().always_auto_detect.set(
            self.view_context()
                .require_control(root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_control_device(&self) {
        // TODO
    }

    fn update_feedback_device(&self) {
        // TODO
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_control_device_combo_box();
        self.invalidate_feedback_device_combo_box();
        self.invalidate_let_matched_events_through_check_box();
        self.invalidate_let_unmatched_events_through_check_box();
        self.invalidate_send_feedback_only_if_armed_check_box();
        self.invalidate_always_auto_detect_check_box();
        self.invalidate_source_filter_buttons();
        self.invalidate_target_filter_buttons();
    }

    fn invalidate_control_device_combo_box(&self) {
        todo!()
    }

    fn invalidate_feedback_device_combo_box(&self) {
        todo!()
    }

    fn invalidate_let_matched_events_through_check_box(&self) {
        let check_box = self
            .view_context()
            .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX);
        if *self.session().midi_control_input.get() == MidiControlInput::FxInput {
            check_box.enable();
            check_box.set_checked(*self.session().let_matched_events_through.get());
        } else {
            check_box.disable();
            check_box.uncheck();
        }
    }

    fn invalidate_let_unmatched_events_through_check_box(&self) {
        let check_box = self
            .view_context()
            .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX);
        if *self.session().midi_control_input.get() == MidiControlInput::FxInput {
            check_box.enable();
            check_box.set_checked(*self.session().let_unmatched_events_through.get());
        } else {
            check_box.disable();
            check_box.uncheck();
        }
    }

    fn invalidate_send_feedback_only_if_armed_check_box(&self) {
        let check_box = self
            .view_context()
            .require_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX);
        if self.session().is_in_input_fx_chain() {
            check_box.disable();
            check_box.check();
        } else {
            check_box.enable();
            check_box.set_checked(*self.session().send_feedback_only_if_armed.get());
        }
    }

    fn invalidate_always_auto_detect_check_box(&self) {
        self.view_context()
            .require_control(root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX)
            .set_checked(*self.session().always_auto_detect.get());
    }

    fn invalidate_source_filter_buttons(&self) {
        // TODO
    }

    fn invalidate_target_filter_buttons(&self) {
        // TODO
    }

    fn register_listeners(self: Rc<Self>) {
        let weak = self.weak();
        self.session()
            .let_matched_events_through
            .changed()
            .take_until(self.view_context.closed())
            .subscribe(move |_| {
                weak.upgrade()
                    .expect("panel gone")
                    .invalidate_let_matched_events_through_check_box()
            });
    }

    fn weak(self: &Rc<Self>) -> Weak<Self> {
        Rc::downgrade(self)
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPINGS_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view_context
    }

    fn opened(self: Rc<Self>, window: Window) -> bool {
        self.invalidate_all_controls();
        self.register_listeners();
        true
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
            ID_EXPORT_BUTTON => self.session().export_to_clipboard(),
            ID_SEND_FEEDBACK_BUTTON => self.session().send_feedback(),
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
