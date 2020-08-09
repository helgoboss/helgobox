use crate::application::SessionData;
use crate::core::{toast, when};
use crate::domain::{MidiControlInput, MidiFeedbackOutput, WeakSession};
use crate::domain::{ReaperTarget, SharedSession};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::ui::SharedMainState;

use clipboard::{ClipboardContext, ClipboardProvider};

use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};

use reaper_medium::{MessageBoxType, MidiInputDeviceId, MidiOutputDeviceId, ReaperString};
use rx_util::UnitEvent;

use slog::debug;

use std::iter;
use std::ops::Deref;
use std::rc::Rc;

use swell_ui::{SharedView, View, ViewContext, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
}

impl HeaderPanel {
    pub fn new(session: WeakSession, main_state: SharedMainState) -> HeaderPanel {
        HeaderPanel {
            view: Default::default(),
            session,
            main_state,
        }
    }
}

impl HeaderPanel {
    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn toggle_learn_source_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        let learning = &mut main_state.is_learning_source_filter;
        if learning.get() {
            // Stop learning
            learning.set(false);
        } else {
            // Start learning
            learning.set(true);
            let main_state_1 = self.main_state.clone();
            let main_state_2 = self.main_state.clone();
            when(
                self.session()
                    .borrow()
                    .midi_source_touched()
                    .take_until(learning.changed_to(false))
                    .take_until(self.view.closed()),
            )
            .with(self.session.clone())
            .finally(move |_| {
                main_state_1
                    .borrow_mut()
                    .is_learning_source_filter
                    .set(false);
            })
            .do_async(move |_session, source| {
                main_state_2.borrow_mut().source_filter.set(Some(source));
            });
        }
    }

    fn toggle_learn_target_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        let learning = &mut main_state.is_learning_target_filter;
        if learning.get() {
            // Stop learning
            learning.set(false);
        } else {
            // Start learning
            learning.set(true);
            when(
                ReaperTarget::touched()
                    .take_until(learning.changed_to(false))
                    .take_until(self.view.closed())
                    .take(1),
            )
            .with(Rc::downgrade(&self.main_state))
            .finally(|main_state| {
                main_state.borrow_mut().is_learning_target_filter.set(false);
            })
            .do_sync(|main_state, target| {
                main_state
                    .borrow_mut()
                    .target_filter
                    .set(Some((*target).clone()));
            });
        }
    }

    fn update_let_matched_events_through(&self) {
        self.session().borrow_mut().let_matched_events_through.set(
            self.view
                .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_let_unmatched_events_through(&self) {
        self.session()
            .borrow_mut()
            .let_unmatched_events_through
            .set(
                self.view
                    .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX)
                    .is_checked(),
            );
    }

    fn update_send_feedback_only_if_armed(&self) {
        self.session().borrow_mut().send_feedback_only_if_armed.set(
            self.view
                .require_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_always_auto_detect(&self) {
        self.session().borrow_mut().always_auto_detect.set(
            self.view
                .require_control(root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX)
                .is_checked(),
        );
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
        b.fill_combo_box_with_data_small(
            iter::once((
                -1isize,
                "<FX input> (no support for MIDI clock sources)".to_string(),
            ))
            .chain(
                Reaper::get()
                    .midi_input_devices()
                    .map(|dev| (dev.id().get() as isize, get_midi_input_device_label(dev))),
            ),
        )
    }

    fn invalidate_midi_control_input_combo_box_value(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        use MidiControlInput::*;
        match self.session().borrow().midi_control_input.get() {
            FxInput => {
                b.select_combo_box_item_by_data(-1).unwrap();
            }
            Device(dev) => b
                .select_combo_box_item_by_data(dev.id().get() as _)
                .unwrap_or_else(|_| {
                    b.select_new_combo_box_item(format!("{}. <Unknown>", dev.id().get()));
                }),
        };
    }

    fn invalidate_midi_feedback_output_combo_box(&self) {
        self.invalidate_midi_feedback_output_combo_box_options();
        self.invalidate_midi_feedback_output_combo_box_value();
    }

    fn invalidate_midi_feedback_output_combo_box_options(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        b.fill_combo_box_with_data_small(
            vec![
                (-1isize, "<None>".to_string()),
                (-2isize, "<FX output>".to_string()),
            ]
            .into_iter()
            .chain(
                Reaper::get()
                    .midi_output_devices()
                    .map(|dev| (dev.id().get() as isize, get_midi_output_device_label(dev))),
            ),
        )
    }

    fn invalidate_midi_feedback_output_combo_box_value(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        use MidiFeedbackOutput::*;
        match self.session().borrow().midi_feedback_output.get() {
            None => {
                b.select_combo_box_item_by_data(-1).unwrap();
            }
            Some(o) => match o {
                FxOutput => {
                    b.select_combo_box_item_by_data(-2).unwrap();
                }
                Device(dev) => b
                    .select_combo_box_item_by_data(dev.id().get() as _)
                    .unwrap_or_else(|_| {
                        b.select_new_combo_box_item(format!("{}. <Unknown>", dev.id().get()));
                    }),
            },
        };
    }

    fn update_search_expression(&self) {
        let ec = self
            .view
            .require_control(root::ID_HEADER_SEARCH_EDIT_CONTROL);
        let text = ec.text().unwrap_or_else(|_| "".to_string());
        self.main_state.borrow_mut().search_expression.set(text);
    }

    fn invalidate_search_expression(&self) {
        let main_state = self.main_state.borrow();
        self.view
            .require_control(root::ID_HEADER_SEARCH_EDIT_CONTROL)
            .set_text(main_state.search_expression.get_ref().as_str())
    }

    fn update_midi_control_input(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => MidiControlInput::FxInput,
            id if id >= 0 => {
                let dev = Reaper::get().midi_input_device_by_id(MidiInputDeviceId::new(id as _));
                MidiControlInput::Device(dev)
            }
            _ => unreachable!(),
        };
        self.session().borrow_mut().midi_control_input.set(value);
    }

    fn update_midi_feedback_output(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id if id >= 0 => {
                let dev = Reaper::get().midi_output_device_by_id(MidiOutputDeviceId::new(id as _));
                Some(MidiFeedbackOutput::Device(dev))
            }
            -2 => Some(MidiFeedbackOutput::FxOutput),
            _ => unreachable!(),
        };
        self.session().borrow_mut().midi_feedback_output.set(value);
    }

    fn invalidate_let_matched_events_through_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX);
        if self.session().borrow().midi_control_input.get() == MidiControlInput::FxInput {
            b.enable();
            b.set_checked(self.session().borrow().let_matched_events_through.get());
        } else {
            b.disable();
            b.uncheck();
        }
    }

    fn invalidate_let_unmatched_events_through_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX);
        if self.session().borrow().midi_control_input.get() == MidiControlInput::FxInput {
            b.enable();
            b.set_checked(self.session().borrow().let_unmatched_events_through.get());
        } else {
            b.disable();
            b.uncheck();
        }
    }

    fn invalidate_send_feedback_only_if_armed_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX);
        if self.session().borrow().containing_fx_is_in_input_fx_chain() {
            b.disable();
            b.check();
        } else {
            b.enable();
            b.set_checked(self.session().borrow().send_feedback_only_if_armed.get());
        }
    }

    fn invalidate_always_auto_detect_check_box(&self) {
        self.view
            .require_control(root::ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX)
            .set_checked(self.session().borrow().always_auto_detect.get());
    }

    fn invalidate_source_filter_buttons(&self) {
        let main_state = self.main_state.borrow();
        self.invalidate_filter_buttons(
            main_state.is_learning_source_filter.get(),
            main_state.source_filter.get_ref().is_some(),
            "Learn source filter",
            root::ID_FILTER_BY_SOURCE_BUTTON,
            root::ID_CLEAR_SOURCE_FILTER_BUTTON,
        );
    }

    fn invalidate_target_filter_buttons(&self) {
        let main_state = self.main_state.borrow();
        self.invalidate_filter_buttons(
            main_state.is_learning_target_filter.get(),
            main_state.target_filter.get_ref().is_some(),
            "Learn target filter",
            root::ID_FILTER_BY_TARGET_BUTTON,
            root::ID_CLEAR_TARGET_FILTER_BUTTON,
        );
    }

    fn invalidate_filter_buttons(
        &self,
        is_learning: bool,
        is_set: bool,
        learn_text: &str,
        learn_button_id: u32,
        clear_button_id: u32,
    ) {
        let learn_button_text = if is_learning { "Stop" } else { learn_text };
        self.view
            .require_control(learn_button_id)
            .set_text(learn_button_text);
        self.view
            .require_control(clear_button_id)
            .set_enabled(is_set);
    }

    pub fn import_from_clipboard(&self) -> Result<(), String> {
        let mut clipboard: ClipboardContext =
            ClipboardProvider::new().map_err(|_| "Couldn't obtain clipboard.".to_string())?;
        let json = clipboard
            .get_contents()
            .map_err(|_| "Couldn't read from clipboard.".to_string())?;
        let session_data: SessionData = serde_json::from_str(json.as_str()).map_err(|e| {
            format!(
                "Clipboard content doesn't look like a proper ReaLearn export. Details:\n\n{}",
                e
            )
        })?;
        let shared_session = self.session();
        let mut session = shared_session.borrow_mut();
        if let Err(e) = session_data.apply_to_model(&mut session) {
            toast::warn(e)
        }
        session.notify_everything_has_changed(self.session.clone());
        session.mark_project_as_dirty();
        Ok(())
    }

    pub fn export_to_clipboard(&self) {
        let session_data = SessionData::from_model(self.session().borrow().deref());
        let json =
            serde_json::to_string_pretty(&session_data).expect("couldn't serialize session data");
        let mut clipboard: ClipboardContext =
            ClipboardProvider::new().expect("couldn't create clipboard");
        clipboard
            .set_contents(json)
            .expect("couldn't set clipboard contents");
    }

    fn register_listeners(self: SharedView<Self>) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        self.when(session.everything_changed(), |view| {
            view.invalidate_all_controls();
        });
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
            let shared_session = view.session();
            let mut session = shared_session.borrow_mut();
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
        let main_state = self.main_state.borrow();
        self.when(
            main_state
                .is_learning_target_filter
                .changed()
                .merge(main_state.target_filter.changed()),
            |view| {
                view.invalidate_target_filter_buttons();
            },
        );
        self.when(
            main_state
                .is_learning_source_filter
                .changed()
                .merge(main_state.source_filter.changed()),
            |view| {
                view.invalidate_source_filter_buttons();
            },
        );
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, _| reaction(panel));
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPINGS_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.invalidate_all_controls();
        self.invalidate_search_expression();
        self.register_listeners();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_ADD_MAPPING_BUTTON => {
                self.session().borrow_mut().add_default_mapping();
            }
            ID_FILTER_BY_SOURCE_BUTTON => self.toggle_learn_source_filter(),
            ID_FILTER_BY_TARGET_BUTTON => self.toggle_learn_target_filter(),
            ID_CLEAR_SOURCE_FILTER_BUTTON => self.main_state.borrow_mut().clear_source_filter(),
            ID_CLEAR_TARGET_FILTER_BUTTON => self.main_state.borrow_mut().clear_target_filter(),
            ID_IMPORT_BUTTON => {
                if let Err(msg) = self.import_from_clipboard() {
                    Reaper::get().medium_reaper().show_message_box(
                        msg,
                        "ReaLearn",
                        MessageBoxType::Okay,
                    );
                }
            }
            ID_EXPORT_BUTTON => self.export_to_clipboard(),
            ID_SEND_FEEDBACK_BUTTON => self.session().borrow().send_feedback(),
            ID_LOG_BUTTON => self.session().borrow().log_debug_info(),
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
            _ => unreachable!(),
        }
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        use root::*;
        match resource_id {
            ID_HEADER_SEARCH_EDIT_CONTROL => self.update_search_expression(),
            _ => unreachable!(),
        }
        true
    }
}

fn get_midi_input_device_label(dev: MidiInputDevice) -> String {
    get_midi_device_label(dev.name(), dev.id().get(), dev.is_connected())
}

fn get_midi_output_device_label(dev: MidiOutputDevice) -> String {
    get_midi_device_label(dev.name(), dev.id().get(), dev.is_connected())
}

fn get_midi_device_label(name: ReaperString, raw_id: u8, connected: bool) -> String {
    format!(
        "{}. {}{}",
        raw_id,
        name.to_str(),
        if connected { "" } else { " <not present>" }
    )
}

impl Drop for HeaderPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping header panel...");
    }
}
