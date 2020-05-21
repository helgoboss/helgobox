use crate::domain::{MidiControlInput, Session};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_low::Swell;
use rx_util::{LocalProp, SharedItemEvent};
use rxrust::prelude::*;
use std::cell::{Cell, Ref, RefCell};
use std::ffi::CString;
use std::rc::{Rc, Weak};
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
pub struct HeaderPanel {
    view: ViewContext,
    session: SharedSession,
}

impl HeaderPanel {
    pub fn new(session: SharedSession) -> HeaderPanel {
        HeaderPanel {
            view: Default::default(),
            session,
        }
    }
}

impl HeaderPanel {
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
            self.view
                .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_let_unmatched_events_through(&self) {
        self.session.borrow_mut().let_unmatched_events_through.set(
            self.view
                .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_send_feedback_only_if_armed(&self) {
        self.session.borrow_mut().send_feedback_only_if_armed.set(
            self.view
                .require_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_always_auto_detect(&self) {
        self.session.borrow_mut().always_auto_detect.set(
            self.view
                .require_control(root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_midi_control_input(&self) {
        // TODO
    }

    fn update_midi_feedback_output(&self) {
        // TODO
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_midi_control_input_combo_box();
        self.invalidate_midi_feedback_output_combo_box();
        self.invalidate_let_matched_events_through_check_box();
        self.invalidate_let_unmatched_events_through_check_box();
        self.invalidate_send_feedback_only_if_armed_check_box();
        self.invalidate_always_auto_detect_check_box();
        self.invalidate_source_filter_buttons();
        self.invalidate_target_filter_buttons();
    }

    fn invalidate_midi_control_input_combo_box(&self) {
        self.invalidate_midi_control_input_combo_box_options();
        self.invalidate_midi_control_input_combo_box_value();
    }

    fn invalidate_midi_control_input_combo_box_options(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        b.clear_combo_box();
        std::iter::once((
            -1isize,
            "<FX input> (no support for MIDI clock sources)".to_string(),
        ))
        .chain(Reaper::get().get_midi_input_devices().map(|dev| {
            (
                dev.get_id().get() as isize,
                get_midi_input_device_label(dev),
            )
        }))
        .enumerate()
        .for_each(|(i, (data, label))| {
            b.insert_combo_box_item_with_data(i, data, label);
        });
    }

    fn invalidate_midi_control_input_combo_box_value(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        use MidiControlInput::*;
        match self.session.borrow().midi_control_input.get() {
            FxInput => {
                b.select_combo_box_item_by_data(-1);
            }
            Device(dev) => b
                .select_combo_box_item_by_data(dev.get_id().get() as _)
                .unwrap_or_else(|_| {
                    b.select_new_combo_box_item(format!("{}. <Unknown>", dev.get_id().get()));
                }),
        };
    }

    fn invalidate_midi_feedback_output_combo_box(&self) {
        // TODO
    }

    fn invalidate_let_matched_events_through_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX);
        if self.session.borrow().midi_control_input.get() == MidiControlInput::FxInput {
            b.enable();
            b.set_checked(self.session.borrow().let_matched_events_through.get());
        } else {
            b.disable();
            b.uncheck();
        }
    }

    fn invalidate_let_unmatched_events_through_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX);
        if self.session.borrow().midi_control_input.get() == MidiControlInput::FxInput {
            b.enable();
            b.set_checked(self.session.borrow().let_unmatched_events_through.get());
        } else {
            b.disable();
            b.uncheck();
        }
    }

    fn invalidate_send_feedback_only_if_armed_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX);
        if self.session.borrow().is_in_input_fx_chain() {
            b.disable();
            b.check();
        } else {
            b.enable();
            b.set_checked(self.session.borrow().send_feedback_only_if_armed.get());
        }
    }

    fn invalidate_always_auto_detect_check_box(&self) {
        self.view
            .require_control(root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX)
            .set_checked(self.session.borrow().always_auto_detect.get());
    }

    fn invalidate_source_filter_buttons(&self) {
        // TODO
    }

    fn invalidate_target_filter_buttons(&self) {
        // TODO
    }

    fn register_listeners(self: SharedView<Self>) {
        let session = self.session.borrow();
        self.when(session.let_matched_events_through.changed(), |view| {
            view.invalidate_let_matched_events_through_check_box()
        });
        self.when(session.let_unmatched_events_through.changed(), |view| {
            view.invalidate_let_unmatched_events_through_check_box()
        });
        self.when(session.send_feedback_only_if_armed.changed(), |view| {
            view.invalidate_send_feedback_only_if_armed_check_box()
        });
        self.when(session.always_auto_detect.changed(), |view| {
            view.invalidate_always_auto_detect_check_box()
        });
        self.when(session.midi_control_input.changed(), |view| {
            view.invalidate_midi_control_input_combo_box();
            view.invalidate_let_matched_events_through_check_box();
            view.invalidate_let_unmatched_events_through_check_box();
            let mut session = view.session.borrow_mut();
            if session.always_auto_detect.get() {
                let control_input = session.midi_control_input.get();
                session
                    .send_feedback_only_if_armed
                    .set(control_input != MidiControlInput::FxInput)
            }
        });
        self.when(session.midi_feedback_output.changed(), |view| {
            view.invalidate_midi_feedback_output_combo_box()
        });
        // TODO sourceFilterListening, targetFilterListening,
    }

    // fn when(
    //     self: &SharedView<Self>,
    //     event: impl ReactiveEvent,
    //     reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    // ) {
    //     self.view.when(&self, event, move |view| {
    //         Reaper::get().do_later_in_main_thread_asap(move || reaction(view));
    //     });
    // }

    fn when(
        self: &SharedView<Self>,
        event: impl SharedItemEvent<()>,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when_async(event, reaction, &self, self.view.closed());
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPINGS_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        self.invalidate_all_controls();
        self.register_listeners();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
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

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_CONTROL_DEVICE_COMBO_BOX => self.update_midi_control_input(),
            ID_FEEDBACK_DEVICE_COMBO_BOX => self.update_midi_feedback_output(),
            _ => {}
        }
    }
}

fn get_midi_input_device_label(dev: MidiInputDevice) -> String {
    get_midi_device_label(dev.get_name(), dev.get_id().get(), dev.is_connected())
}

fn get_midi_output_device_label(dev: MidiOutputDevice) -> String {
    get_midi_device_label(dev.get_name(), dev.get_id().get(), dev.is_connected())
}

fn get_midi_device_label(name: CString, raw_id: u8, connected: bool) -> String {
    format!(
        "{}. {}{}",
        raw_id,
        name.to_str().expect("not UTF-8"),
        if connected { "" } else { " <not present>" }
    )
}
