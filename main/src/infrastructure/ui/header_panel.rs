use std::convert::TryInto;
use std::ops::Deref;

use std::rc::Rc;

use std::iter;

use clipboard::{ClipboardContext, ClipboardProvider};
use enum_iterator::IntoEnumIterator;

use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};

use reaper_medium::{
    MessageBoxResult, MessageBoxType, MidiInputDeviceId, MidiOutputDeviceId, ReaperString,
};
use slog::debug;

use rx_util::UnitEvent;
use swell_ui::{MenuBar, Pixels, Point, SharedView, View, ViewContext, Window};

use crate::application::{Controller, SharedSession, WeakSession};
use crate::core::{toast, when};
use crate::domain::{MappingCompartment, ReaperTarget};
use crate::domain::{MidiControlInput, MidiFeedbackOutput};
use crate::infrastructure::data::SessionData;
use crate::infrastructure::plugin::{warn_about_failed_server_start, App};

use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::dialog_util::alert;
use crate::infrastructure::ui::{add_firewall_rule, SharedMainState};
use crate::infrastructure::ui::{dialog_util, CompanionAppPresenter};

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    companion_app_presenter: Rc<CompanionAppPresenter>,
}

impl HeaderPanel {
    pub fn new(session: WeakSession, main_state: SharedMainState) -> HeaderPanel {
        HeaderPanel {
            view: Default::default(),
            session: session.clone(),
            main_state,
            companion_app_presenter: CompanionAppPresenter::new(session),
        }
    }
}

impl HeaderPanel {
    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn active_compartment(&self) -> MappingCompartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn toggle_learn_source_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        let compartment = main_state.active_compartment.get();
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
                    .source_touched(compartment)
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

    fn fill_all_controls(&self) {
        self.fill_compartment_combo_box();
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_midi_control_input_combo_box();
        self.invalidate_midi_feedback_output_combo_box();
        self.invalidate_compartment_combo_box();
        self.invalidate_preset_controls();
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

    fn invalidate_compartment_combo_box(&self) {
        self.view
            .require_control(root::ID_COMPARTMENT_COMBO_BOX)
            .select_combo_box_item(self.active_compartment().into());
    }

    fn invalidate_preset_controls(&self) {
        let label = self.view.require_control(root::ID_PRESET_LABEL_TEXT);
        let combo = self.view.require_control(root::ID_PRESET_COMBO_BOX);
        let delete_button = self.view.require_control(root::ID_PRESET_DELETE_BUTTON);
        let save_button = self.view.require_control(root::ID_PRESET_SAVE_BUTTON);
        let save_as_button = self.view.require_control(root::ID_PRESET_SAVE_AS_BUTTON);
        if self.main_state.borrow().active_compartment.get()
            == MappingCompartment::ControllerMappings
        {
            label.show();
            combo.show();
            delete_button.show();
            save_button.show();
            save_as_button.show();
            self.invalidate_preset_combo_box();
            self.invalidate_preset_buttons();
        } else {
            label.hide();
            combo.hide();
            delete_button.hide();
            save_button.hide();
            save_as_button.hide();
        }
    }

    fn invalidate_preset_combo_box(&self) {
        self.fill_preset_combo_box();
        self.invalidate_preset_combo_box_value();
    }

    fn invalidate_preset_buttons(&self) {
        let delete_button = self.view.require_control(root::ID_PRESET_DELETE_BUTTON);
        let save_button = self.view.require_control(root::ID_PRESET_SAVE_BUTTON);
        let session = self.session();
        let session = session.borrow();
        let controller_is_active = session.active_controller_id().is_some();
        delete_button.set_enabled(controller_is_active);
        let controller_mappings_are_dirty = session.controller_mappings_are_dirty();
        save_button.set_enabled(controller_is_active && controller_mappings_are_dirty);
    }

    fn fill_preset_combo_box(&self) {
        self.view
            .require_control(root::ID_PRESET_COMBO_BOX)
            .fill_combo_box_with_data_small(
                vec![(-1isize, "<None>".to_string())].into_iter().chain(
                    App::get()
                        .controller_manager()
                        .borrow()
                        .controllers()
                        .enumerate()
                        .map(|(i, c)| (i as isize, c.to_string())),
                ),
            );
    }

    fn invalidate_preset_combo_box_value(&self) {
        let combo = self.view.require_control(root::ID_PRESET_COMBO_BOX);
        let index = match self.session().borrow().active_controller_id() {
            None => -1isize,
            Some(id) => {
                let index_option = App::get()
                    .controller_manager()
                    .borrow()
                    .find_index_by_id(id);
                match index_option {
                    None => {
                        combo.select_new_combo_box_item(format!("<Not present> ({})", id));
                        return;
                    }
                    Some(i) => i as isize,
                }
            }
        };
        combo.select_combo_box_item_by_data(index).unwrap();
    }

    fn fill_compartment_combo_box(&self) {
        self.view
            .require_control(root::ID_COMPARTMENT_COMBO_BOX)
            .fill_combo_box(MappingCompartment::into_enum_iter());
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

    fn update_compartment(&self) {
        self.main_state.borrow_mut().active_compartment.set(
            self.view
                .require_control(root::ID_COMPARTMENT_COMBO_BOX)
                .selected_combo_box_item_index()
                .try_into()
                .expect("invalid compartment"),
        );
    }

    fn update_preset(&self) {
        if self.session().borrow().controller_mappings_are_dirty() {
            let result = Reaper::get().medium_reaper().show_message_box(
                "Your changes of the current controller mappings will be lost. Consider to save them first. Do you really want to continue?",
                "ReaLearn",
                MessageBoxType::YesNo,
            );
            if result == MessageBoxResult::No {
                self.invalidate_preset_combo_box_value();
                return;
            }
        }
        let controller_manager = App::get().controller_manager();
        let controller_manager = controller_manager.borrow();
        let controller = match self
            .view
            .require_control(root::ID_PRESET_COMBO_BOX)
            .selected_combo_box_item_data()
        {
            -1 => None,
            i if i >= 0 => controller_manager.find_by_index(i as usize),
            _ => unreachable!(),
        };
        self.session()
            .borrow_mut()
            .activate_controller(controller.map(|c| c.id().to_string()), self.session.clone())
            .unwrap();
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

    fn delete_active_preset(&self) -> Result<(), &'static str> {
        let result = Reaper::get().medium_reaper().show_message_box(
            "Do you really want to remove this controller?",
            "ReaLearn",
            MessageBoxType::YesNo,
        );
        if result == MessageBoxResult::No {
            return Ok(());
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        let active_controller_id = session
            .active_controller_id()
            .ok_or("no controller selected")?
            .to_string();
        session.activate_controller(None, self.session.clone())?;
        App::get()
            .controller_manager()
            .borrow_mut()
            .remove_controller(&active_controller_id)?;
        Ok(())
    }

    fn save_active_preset(&self) -> Result<(), &'static str> {
        let session = self.session();
        let session = session.borrow();
        match session.active_controller() {
            None => Err("no active preset"),
            Some(mut controller) => {
                let mappings = session
                    .mappings(MappingCompartment::ControllerMappings)
                    .map(|ptr| ptr.borrow().clone())
                    .collect();
                controller.update_mappings(mappings);
                App::get()
                    .controller_manager()
                    .borrow_mut()
                    .update_controller(controller)?;
                Ok(())
            }
        }
    }

    fn change_session_id(&self) {
        let current_session_id = { self.session().borrow().id.get_ref().clone() };
        let new_session_id = match dialog_util::prompt_for("Session ID", &current_session_id) {
            None => return,
            Some(n) => n.trim().to_string(),
        };
        if new_session_id == current_session_id {
            return;
        }
        if crate::application::App::get().has_session(&new_session_id) {
            alert("There's another open ReaLearn session which already has this session ID!");
            return;
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        if new_session_id.is_empty() {
            session.reset_id();
        } else {
            session.id.set(new_session_id);
        }
    }

    fn save_as_preset(&self) -> Result<(), &'static str> {
        let controller_name = match dialog_util::prompt_for("Controller name", "") {
            None => return Ok(()),
            Some(n) => n,
        };
        let controller_id = slug::slugify(&controller_name);
        let session = self.session();
        let mut session = session.borrow_mut();
        let custom_data = session
            .active_controller()
            .map(|c| c.custom_data().clone())
            .unwrap_or_default();
        let mappings = session
            .mappings(MappingCompartment::ControllerMappings)
            .map(|ptr| ptr.borrow().clone())
            .collect();
        let controller = Controller::new(
            controller_id.clone(),
            controller_name,
            mappings,
            custom_data,
        );
        App::get()
            .controller_manager()
            .borrow_mut()
            .add_controller(controller)?;
        session.activate_controller(Some(controller_id), self.session.clone())?;
        Ok(())
    }

    fn log_debug_info(&self) {
        let session = self.session();
        let session = session.borrow();
        session.log_debug_info();
        App::get().log_debug_info(session.id());
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
        self.when(main_state.active_compartment.changed(), |view| {
            view.invalidate_compartment_combo_box();
            view.invalidate_preset_controls();
        });
        when(
            App::get()
                .controller_manager()
                .borrow()
                .changed()
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_async(move |view, _| {
            view.invalidate_preset_controls();
        });
        when(
            session
                .mapping_list_changed()
                .merge(session.mapping_changed())
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_sync(move |view, compartment| {
            if compartment == MappingCompartment::ControllerMappings {
                view.invalidate_preset_buttons();
            }
        });
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
        self.fill_all_controls();
        self.invalidate_all_controls();
        self.invalidate_search_expression();
        self.register_listeners();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_ADD_MAPPING_BUTTON => {
                self.session()
                    .borrow_mut()
                    .add_default_mapping(self.active_compartment());
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
            ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX => self.update_let_matched_events_through(),
            ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX => self.update_let_unmatched_events_through(),
            ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX => self.update_send_feedback_only_if_armed(),
            ID_ALWAYS_AUTO_DETECT_MODE_CHECK_BOX => self.update_always_auto_detect(),
            ID_PRESET_DELETE_BUTTON => {
                self.delete_active_preset().unwrap();
            }
            ID_PRESET_SAVE_AS_BUTTON => {
                self.save_as_preset().unwrap();
            }
            ID_PRESET_SAVE_BUTTON => {
                self.save_active_preset().unwrap();
            }
            ID_PROJECTION_BUTTON => {
                self.companion_app_presenter.show_app_info();
            }
            _ => {}
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_CONTROL_DEVICE_COMBO_BOX => self.update_midi_control_input(),
            ID_FEEDBACK_DEVICE_COMBO_BOX => self.update_midi_feedback_output(),
            ID_COMPARTMENT_COMBO_BOX => self.update_compartment(),
            ID_PRESET_COMBO_BOX => self.update_preset(),
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

    fn context_menu_wanted(self: SharedView<Self>, location: Point<Pixels>) {
        let menu_bar = MenuBar::load(root::IDR_HEADER_PANEL_CONTEXT_MENU)
            .expect("menu bar couldn't be loaded");
        let menu = menu_bar.get_menu(0).expect("menu bar didn't have 1st menu");
        let app = App::get();
        enum ServerAction {
            Start,
            Disable,
            Enable,
        }
        let (next_server_action, http_port, https_port) = {
            let server = app.server().borrow();
            let server_is_enabled = app.config().server_is_enabled();
            let next_server_action = {
                use ServerAction::*;
                if server.is_running() {
                    if server_is_enabled { Disable } else { Enable }
                } else {
                    Start
                }
            };
            menu.set_item_checked(root::IDM_SERVER_START, server_is_enabled);
            (next_server_action, server.http_port(), server.https_port())
        };
        let result = match self.view.require_window().open_popup_menu(menu, location) {
            None => return,
            Some(r) => r,
        };
        match result {
            root::IDM_LOG_DEBUG_INFO => self.log_debug_info(),
            root::IDM_CHANGE_SESSION_ID => {
                self.change_session_id();
            }
            root::IDM_SERVER_START => {
                use ServerAction::*;
                match next_server_action {
                    Start => {
                        match App::start_server_persistently(app) {
                            Ok(_) => {
                                Reaper::get().medium_reaper().show_message_box(
                                    "Successfully started projection server.",
                                    "ReaLearn",
                                    MessageBoxType::Okay,
                                );
                            }
                            Err(info) => {
                                warn_about_failed_server_start(info);
                            }
                        };
                    }
                    Disable => {
                        app.disable_server_persistently();
                        Reaper::get().medium_reaper().show_message_box(
                            "Disabled projection server. This will take effect on the next start of REAPER.",
                            "ReaLearn",
                            MessageBoxType::Okay,
                        );
                    }
                    Enable => {
                        app.enable_server_persistently();
                        Reaper::get().medium_reaper().show_message_box(
                            "Enabled projection server again.",
                            "ReaLearn",
                            MessageBoxType::Okay,
                        );
                    }
                }
            }
            root::IDM_SERVER_ADD_FIREWALL_RULE => {
                let msg = match add_firewall_rule(http_port, https_port) {
                    Ok(_) => "Successfully added firewall rule.",
                    Err(_) => "Couldn't add firewall rule. Please try to do it manually!",
                };
                Reaper::get().medium_reaper().show_message_box(
                    msg,
                    "ReaLearn",
                    MessageBoxType::Okay,
                );
            }
            _ => unreachable!(),
        };
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
