use std::cell::RefCell;
use std::convert::TryInto;
use std::ops::Deref;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::thread::JoinHandle;
use std::{io, iter};

use clipboard::{ClipboardContext, ClipboardProvider};
use enum_iterator::IntoEnumIterator;
use image::Luma;
use once_cell::unsync::Lazy;
use qrcode::QrCode;
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_low::{raw, Swell};
use reaper_medium::{
    MessageBoxResult, MessageBoxType, MidiInputDeviceId, MidiOutputDeviceId, ReaperString,
};
use slog::debug;
use web_view::{Content, Error, WebView};
use wrap_debug::WrapDebug;

use rx_util::UnitEvent;
use swell_ui::{MenuBar, Pixels, Point, SharedView, View, ViewContext, Window};

use crate::application::{Controller, SharedSession, WeakSession};
use crate::core::{toast, when};
use crate::domain::{MappingCompartment, ReaperTarget};
use crate::domain::{MidiControlInput, MidiFeedbackOutput};
use crate::infrastructure::data::SessionData;
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::dialog_util;
use crate::infrastructure::ui::SharedMainState;

type WebViewState = ();

#[derive(Debug)]
struct WebViewConnection {
    sender: crossbeam_channel::Sender<WebViewTask>,
    join_handle: JoinHandle<()>,
}

type WebViewTask = Box<dyn FnOnce(&mut WebView<WebViewState>) + Send + 'static>;

impl WebViewConnection {
    pub fn send(&self, task: impl FnOnce(&mut WebView<WebViewState>) + Send + 'static) {
        let _ = self.sender.send(Box::new(task));
    }

    pub fn blocking_exit(mut self) {
        self.send(|wv| wv.exit());
        self.join_handle
            .join()
            .expect("couldn't join with web view thread");
    }
}

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    web_view_connection: RefCell<Option<WebViewConnection>>,
    qrcode_file_temp_path: Lazy<Option<tempfile::TempPath>>,
}

impl HeaderPanel {
    pub fn new(session: WeakSession, main_state: SharedMainState) -> HeaderPanel {
        HeaderPanel {
            view: Default::default(),
            session,
            main_state,
            web_view_connection: Default::default(),
            qrcode_file_temp_path: Lazy::new(|| create_qrcode_file().ok()),
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
                    .source_touched()
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

    fn show_projection_info(&self) {
        self.exit_web_view_blocking();
        let mut server = App::get().server().borrow_mut();
        if !server.is_running() {
            let result = Reaper::get().medium_reaper().show_message_box(
                "In order to use the projection feature, ReaLearn must start a web server to which \
            mobile devices can connect. If you choose yes, the server will be started right now \
            and in future whenever ReaLearn is loaded the first time. You can disable the server \
            at any time in the context menu. \n\
            \n\
            Do you want to continue?",
                "ReaLearn",
                MessageBoxType::YesNo,
            );
            if result != MessageBoxResult::Yes {
                return;
            }
            server.start();
        }
        let session = self.session();
        let session = session.borrow();
        let url_to_encode = server.generate_realearn_app_url(session.id());
        let (file, width, height) = {
            let file = self
                .qrcode_file_temp_path
                .as_ref()
                .expect("couldn't create temp file for QR code");
            let code = QrCode::new(url_to_encode).unwrap();
            let image = code.render::<image::Luma<u8>>().build();
            image
                .save(file)
                .expect("couldn't save QR code image to temporary file");
            (file.to_string_lossy(), image.width(), image.height())
        };
        // let html_content = format!(
        //     r#"
        //     <html>
        //     <body>
        //     <h1>ReaLearn</h1>
        //     <img src="{}"/>
        //     Or manually enter the following data:
        //     <table>
        //     <tr>
        //     <td>Host:</td>
        //     <td>{}</td>
        //     </tr>
        //     <tr>
        //     <td>Port:</td>
        //     <td>{}</td>
        //     </tr>
        //     <tr>
        //     <td>Session:</td>
        //     <td>{}</td>
        //     </tr>
        //     </table>
        //     </body>
        //     </html>
        //     "#,
        //     file,
        //     server
        //         .local_ip()
        //         .map(|ip| ip.to_string())
        //         .unwrap_or("<could not be determined>".to_string()),
        //     server.port(),
        //     session.id()
        // );
        let html_content = include_str!("web_view_content.html");
        let (sender, receiver): (
            crossbeam_channel::Sender<WebViewTask>,
            crossbeam_channel::Receiver<WebViewTask>,
        ) = crossbeam_channel::unbounded();
        let join_handle = std::thread::spawn(move || {
            let mut wv = web_view::builder()
                .title("ReaLearn")
                .content(Content::Html(html_content))
                .size(800, 600)
                .resizable(false)
                .user_data(())
                .invoke_handler(|_webview, _arg| Ok(()))
                .build()
                .expect("couldn't build WebView");
            loop {
                for task in receiver.try_iter() {
                    (task)(&mut wv);
                }
                match wv.step() {
                    // WebView closed
                    None => break,
                    // WebView still running or error
                    Some(res) => res.expect("error in projection web view"),
                }
            }
        });
        *self.web_view_connection.borrow_mut() = Some(WebViewConnection {
            sender,
            join_handle,
        });
        self.update_web_view();
    }

    fn update_web_view(&self) {
        if let Some(c) = self.web_view_connection.borrow().as_ref() {
            c.send(|wv| {
                wv.eval("document.body.innerHTML = '<b>Test</b>';");
            })
        }
    }

    fn toggle_server(&self) {
        let mut server = App::get().server().borrow_mut();
        if server.is_running() {
            server.stop();
        } else {
            server.start();
        }
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

    fn exit_web_view_blocking(&self) {
        if let Some(con) = self.web_view_connection.replace(None) {
            con.blocking_exit();
        }
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
                self.show_projection_info();
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
        let server_is_running = App::get().server().borrow().is_running();
        menu.set_item_checked(root::IDM_PROJECTION_SERVER, server_is_running);

        let result = match self.view.require_window().open_popup_menu(menu, location) {
            None => return,
            Some(r) => r,
        };
        match result {
            root::IDM_PROJECTION_SERVER => self.toggle_server(),
            root::IDM_LOG_DEBUG_INFO => self.log_debug_info(),
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
        self.exit_web_view_blocking();
    }
}

fn create_qrcode_file() -> io::Result<tempfile::TempPath> {
    let mut file = tempfile::Builder::new().suffix(".png").tempfile()?;
    Ok(file.into_temp_path())
}
