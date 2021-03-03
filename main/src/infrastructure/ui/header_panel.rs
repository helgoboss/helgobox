use std::convert::TryInto;

use derive_more::Display;
use std::rc::{Rc, Weak};

use std::{iter, sync};

use clipboard::{ClipboardContext, ClipboardProvider};
use enum_iterator::IntoEnumIterator;

use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};

use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId, ReaperString};
use slog::debug;

use rx_util::UnitEvent;
use swell_ui::{MenuBar, Pixels, Point, SharedView, View, ViewContext, Window};

use crate::application::{
    make_mappings_project_independent, mappings_have_project_references, ControllerPreset, FxId,
    GroupId, MainPreset, MainPresetAutoLoadMode, MappingModel, PresetManager, Session,
    SharedSession, VirtualControlElementType, WeakSession,
};
use crate::core::when;
use crate::domain::{MappingCompartment, OscDeviceId, ProcessorContext, ReaperTarget};
use crate::domain::{MidiControlInput, MidiFeedbackOutput};
use crate::infrastructure::data::{ExtendedPresetManager, OscDevice, SessionData};
use crate::infrastructure::plugin::{
    warn_about_failed_server_start, App, RealearnPluginParameters,
};

use crate::infrastructure::ui::bindings::root;

use crate::infrastructure::ui::{
    add_firewall_rule, GroupFilter, GroupPanel, IndependentPanelManager,
    SharedIndependentPanelManager, SharedMainState,
};
use crate::infrastructure::ui::{dialog_util, CompanionAppPresenter};
use std::cell::{Cell, RefCell};
use std::net::Ipv4Addr;

const OSC_INDEX_OFFSET: isize = 1000;

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    companion_app_presenter: Rc<CompanionAppPresenter>,
    plugin_parameters: sync::Weak<RealearnPluginParameters>,
    panel_manager: Weak<RefCell<IndependentPanelManager>>,
    group_panel: RefCell<Option<SharedView<GroupPanel>>>,
    is_invoked_programmatically: Cell<bool>,
}

impl HeaderPanel {
    pub fn new(
        session: WeakSession,
        main_state: SharedMainState,
        plugin_parameters: sync::Weak<RealearnPluginParameters>,
        panel_manager: Weak<RefCell<IndependentPanelManager>>,
    ) -> HeaderPanel {
        HeaderPanel {
            view: Default::default(),
            session: session.clone(),
            main_state,
            companion_app_presenter: CompanionAppPresenter::new(session),
            plugin_parameters,
            panel_manager,
            group_panel: Default::default(),
            is_invoked_programmatically: false.into(),
        }
    }
}

impl HeaderPanel {
    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    /// If you know a function in this view can be invoked by something else than the dialog
    /// process, wrap your function body with this. Basically all pub functions!
    ///
    /// This prevents edit control text change events fired by windows to be processed.
    fn invoke_programmatically(&self, f: impl FnOnce()) {
        self.is_invoked_programmatically.set(true);
        scopeguard::defer! { self.is_invoked_programmatically.set(false); }
        f();
    }

    fn active_compartment(&self) -> MappingCompartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn active_group_id(&self) -> Option<GroupId> {
        let group_filter = self.main_state.borrow().group_filter.get()?;
        Some(group_filter.group_id())
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn toggle_learn_many_mappings(&self) {
        self.main_state.borrow_mut().stop_filter_learning();
        let session = self.session();
        if session.borrow().is_learning_many_mappings() {
            session.borrow_mut().stop_learning_many_mappings();
            self.panel_manager().borrow().close_message_panel();
        } else {
            let compartment = self.active_compartment();
            let control_element_type = match compartment {
                MappingCompartment::ControllerMappings => {
                    match self.prompt_for_control_element_type() {
                        None => return,
                        Some(t) => t,
                    }
                }
                MappingCompartment::MainMappings => {
                    // Doesn't matter
                    VirtualControlElementType::Multi
                }
            };
            session.borrow_mut().start_learning_many_mappings(
                &session,
                compartment,
                self.active_group_id().unwrap_or_default(),
                control_element_type,
            );
            self.panel_manager().borrow().open_message_panel();
        }
    }

    fn prompt_for_control_element_type(&self) -> Option<VirtualControlElementType> {
        let menu_bar = MenuBar::load(root::IDR_HEADER_PANEL_ADD_MANY_CONTROLLER_MAPPINGS_MENU)
            .expect("menu bar couldn't be loaded");
        let menu = menu_bar.get_menu(0).expect("menu bar didn't have 1st menu");
        let location = Window::cursor_pos();
        let result = self.view.require_window().open_popup_menu(menu, location)?;
        let control_element_type = match result {
            root::IDM_MULTIS => VirtualControlElementType::Multi,
            root::IDM_BUTTONS => VirtualControlElementType::Button,
            _ => unreachable!(),
        };
        Some(control_element_type)
    }

    fn add_group(&self) {
        if let Some(name) = dialog_util::prompt_for("Group name", "") {
            if name.is_empty() {
                return;
            }
            let id = self.session().borrow_mut().add_default_group(name);
            self.main_state
                .borrow_mut()
                .group_filter
                .set(Some(GroupFilter(id)));
        }
    }

    fn add_mapping(&self) {
        self.main_state
            .borrow_mut()
            .clear_all_filters_except_group();
        self.session().borrow_mut().add_default_mapping(
            self.active_compartment(),
            self.active_group_id().unwrap_or_default(),
            VirtualControlElementType::Multi,
        );
    }

    fn toggle_learn_source_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        let active_compartment = main_state.active_compartment.get();
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
                    .source_touched(
                        true,
                        active_compartment != MappingCompartment::ControllerMappings,
                        None,
                    )
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

    fn clear_source_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        main_state.clear_source_filter();
        // When clearing the source filter while still learning, we want the learning to stop, too.
        let learning = &mut main_state.is_learning_source_filter;
        if learning.get() {
            // Stop learning
            learning.set(false);
        }
    }

    fn clear_target_filter(&self) {
        self.main_state.borrow_mut().clear_target_filter();
    }

    fn clear_search_expression(&self) {
        self.main_state
            .borrow_mut()
            .clear_search_expression_filter();
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

    fn toggle_send_feedback_only_if_armed(&self) {
        self.session()
            .borrow_mut()
            .send_feedback_only_if_armed
            .set_with(|prev| !*prev);
    }

    fn toggle_always_auto_detect(&self) {
        self.session()
            .borrow_mut()
            .auto_correct_settings
            .set_with(|prev| !*prev);
    }

    fn fill_all_controls(&self) {
        self.fill_compartment_combo_box();
        self.fill_preset_auto_load_mode_combo_box();
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_control_input_combo_box();
        self.invalidate_feedback_output_combo_box();
        self.invalidate_compartment_combo_box();
        self.invalidate_preset_controls();
        self.invalidate_group_controls();
        self.invalidate_let_matched_events_through_check_box();
        self.invalidate_let_unmatched_events_through_check_box();
        self.invalidate_source_filter_buttons();
        self.invalidate_target_filter_buttons();
        self.invalidate_add_one_button();
        self.invalidate_learn_many_button();
    }

    fn invalidate_control_input_combo_box(&self) {
        self.invalidate_control_input_combo_box_options();
        self.invalidate_control_input_combo_box_value();
    }

    fn invalidate_compartment_combo_box(&self) {
        self.view
            .require_control(root::ID_COMPARTMENT_COMBO_BOX)
            .select_combo_box_item(self.active_compartment().into());
    }

    fn invalidate_preset_auto_load_mode_combo_box(&self) {
        let label = self.view.require_control(root::ID_AUTO_LOAD_LABEL_TEXT);
        let combo = self.view.require_control(root::ID_AUTO_LOAD_COMBO_BOX);
        if self.active_compartment() == MappingCompartment::MainMappings {
            label.show();
            combo.show();
            combo.select_combo_box_item(
                self.session()
                    .borrow()
                    .main_preset_auto_load_mode
                    .get()
                    .into(),
            );
        } else {
            label.hide();
            combo.hide();
        }
    }

    fn invalidate_group_controls(&self) {
        self.invalidate_group_control_appearance();
        if self.active_compartment() != MappingCompartment::MainMappings {
            return;
        }
        self.invalidate_group_combo_box();
        self.invalidate_group_buttons();
    }

    fn invalidate_group_control_appearance(&self) {
        self.show_if(
            self.active_compartment() == MappingCompartment::MainMappings,
            &[
                root::ID_GROUP_LABEL_TEXT,
                root::ID_GROUP_COMBO_BOX,
                root::ID_GROUP_ADD_BUTTON,
                root::ID_GROUP_DELETE_BUTTON,
                root::ID_GROUP_EDIT_BUTTON,
            ],
        );
    }

    fn show_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            self.view.require_control(*id).set_visible(condition);
        }
    }

    fn invalidate_group_combo_box(&self) {
        self.fill_group_combo_box();
        self.invalidate_group_combo_box_value();
    }

    fn fill_group_combo_box(&self) {
        let combo = self.view.require_control(root::ID_GROUP_COMBO_BOX);
        let vec = vec![
            (-2isize, "<All>".to_string()),
            (-1isize, "<Default>".to_string()),
        ];
        combo.fill_combo_box_with_data_small(
            vec.into_iter().chain(
                self.session()
                    .borrow()
                    .groups_sorted()
                    .enumerate()
                    .map(|(i, g)| (i as isize, g.borrow().to_string())),
            ),
        );
    }

    fn invalidate_group_combo_box_value(&self) {
        let combo = self.view.require_control(root::ID_GROUP_COMBO_BOX);
        let data = match self.main_state.borrow().group_filter.get() {
            None => -2isize,
            Some(GroupFilter(id)) => {
                if id.is_default() {
                    -1isize
                } else {
                    match self.session().borrow().find_group_index_by_id_sorted(id) {
                        None => {
                            combo.select_new_combo_box_item(format!("<Not present> ({})", id));
                            return;
                        }
                        Some(i) => i as isize,
                    }
                }
            }
        };
        combo.select_combo_box_item_by_data(data).unwrap();
    }

    fn invalidate_group_buttons(&self) {
        let remove_button = self.view.require_control(root::ID_GROUP_DELETE_BUTTON);
        let edit_button = self.view.require_control(root::ID_GROUP_EDIT_BUTTON);
        let (remove_enabled, edit_enabled) = match self.main_state.borrow().group_filter.get() {
            None => (false, false),
            Some(GroupFilter(id)) if id.is_default() => (false, true),
            _ => (true, true),
        };
        remove_button.set_enabled(remove_enabled);
        edit_button.set_enabled(edit_enabled);
    }

    fn invalidate_preset_controls(&self) {
        self.invalidate_preset_combo_box();
        self.invalidate_preset_buttons();
        self.invalidate_preset_auto_load_mode_combo_box();
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
        let (preset_is_active, is_dirty) = match self.active_compartment() {
            MappingCompartment::ControllerMappings => (
                session.active_controller_id().is_some(),
                session.controller_preset_is_out_of_date(),
            ),
            MappingCompartment::MainMappings => (
                session.active_main_preset().is_some(),
                session.main_preset_is_out_of_date(),
            ),
        };
        delete_button.set_enabled(preset_is_active);
        save_button.set_enabled(preset_is_active && is_dirty);
    }

    fn fill_preset_combo_box(&self) {
        let combo = self.view.require_control(root::ID_PRESET_COMBO_BOX);
        let vec = vec![(-1isize, "<None>".to_string())];
        match self.active_compartment() {
            MappingCompartment::ControllerMappings => combo.fill_combo_box_with_data_small(
                vec.into_iter().chain(
                    App::get()
                        .controller_preset_manager()
                        .borrow()
                        .presets()
                        .enumerate()
                        .map(|(i, c)| (i as isize, c.to_string())),
                ),
            ),
            MappingCompartment::MainMappings => combo.fill_combo_box_with_data_small(
                vec.into_iter().chain(
                    App::get()
                        .main_preset_manager()
                        .borrow()
                        .presets()
                        .enumerate()
                        .map(|(i, c)| (i as isize, c.to_string())),
                ),
            ),
        };
    }

    fn invalidate_preset_combo_box_value(&self) {
        let combo = self.view.require_control(root::ID_PRESET_COMBO_BOX);
        let enabled = !self.mappings_are_read_only();
        let session = self.session();
        let session = session.borrow();
        let (preset_manager, active_preset_id): (Box<dyn ExtendedPresetManager>, _) =
            match self.active_compartment() {
                MappingCompartment::ControllerMappings => (
                    Box::new(App::get().controller_preset_manager()),
                    session.active_controller_id(),
                ),
                MappingCompartment::MainMappings => (
                    Box::new(App::get().main_preset_manager()),
                    session.active_main_preset_id(),
                ),
            };
        let data = match active_preset_id {
            None => -1isize,
            Some(id) => match preset_manager.find_index_by_id(id) {
                None => {
                    combo.select_new_combo_box_item(format!("<Not present> ({})", id));
                    return;
                }
                Some(i) => i as isize,
            },
        };
        combo.select_combo_box_item_by_data(data).unwrap();
        combo.set_enabled(enabled);
    }

    fn fill_compartment_combo_box(&self) {
        self.view
            .require_control(root::ID_COMPARTMENT_COMBO_BOX)
            .fill_combo_box(MappingCompartment::into_enum_iter());
    }

    fn fill_preset_auto_load_mode_combo_box(&self) {
        self.view
            .require_control(root::ID_AUTO_LOAD_COMBO_BOX)
            .fill_combo_box(MainPresetAutoLoadMode::into_enum_iter());
    }

    fn invalidate_control_input_combo_box_options(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        let osc_device_manager = App::get().osc_device_manager();
        let osc_device_manager = osc_device_manager.borrow();
        let osc_devices = osc_device_manager.devices();
        b.fill_combo_box_with_data_small(
            vec![
                (-100isize, generate_midi_device_heading()),
                (
                    -1isize,
                    "<FX input> (no support for MIDI clock sources)".to_string(),
                ),
            ]
            .into_iter()
            .chain(
                Reaper::get()
                    .midi_input_devices()
                    .filter(|d| d.is_available())
                    .map(|dev| (dev.id().get() as isize, get_midi_input_device_label(dev))),
            )
            .chain(iter::once((
                -100isize,
                generate_osc_device_heading(osc_devices.len()),
            )))
            .chain(osc_devices.enumerate().map(|(i, dev)| {
                (
                    OSC_INDEX_OFFSET + i as isize,
                    get_osc_device_label(dev, false),
                )
            })),
        )
    }

    fn invalidate_control_input_combo_box_value(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        use MidiControlInput::*;
        if let Some(osc_device_id) = self.session().borrow().osc_input_device_id.get_ref() {
            // We currently don't let the UI set both a MIDI and OSC device. Although internally
            // this would be perfectly possible, it could be confusing.
            match App::get()
                .osc_device_manager()
                .borrow()
                .find_index_by_id(osc_device_id)
            {
                None => {
                    b.select_new_combo_box_item(format!("<Not present> ({})", osc_device_id));
                }
                Some(i) => b
                    .select_combo_box_item_by_data(OSC_INDEX_OFFSET + i as isize)
                    .unwrap(),
            };
            return;
        }
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

    fn invalidate_feedback_output_combo_box(&self) {
        self.invalidate_feedback_output_combo_box_options();
        self.invalidate_feedback_output_combo_box_value();
    }

    fn invalidate_feedback_output_combo_box_options(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        let osc_device_manager = App::get().osc_device_manager();
        let osc_device_manager = osc_device_manager.borrow();
        let osc_devices = osc_device_manager.devices();
        b.fill_combo_box_with_data_small(
            vec![
                (-1isize, "<None>".to_string()),
                (-100isize, generate_midi_device_heading()),
                (-2isize, "<FX output>".to_string()),
            ]
            .into_iter()
            .chain(
                Reaper::get()
                    .midi_output_devices()
                    .filter(|d| d.is_available())
                    .map(|dev| (dev.id().get() as isize, get_midi_output_device_label(dev))),
            )
            .chain(iter::once((
                -100isize,
                generate_osc_device_heading(osc_devices.len()),
            )))
            .chain(osc_devices.enumerate().map(|(i, dev)| {
                (
                    OSC_INDEX_OFFSET + i as isize,
                    get_osc_device_label(dev, true),
                )
            })),
        )
    }

    fn invalidate_feedback_output_combo_box_value(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        use MidiFeedbackOutput::*;
        if let Some(osc_device_id) = self.session().borrow().osc_output_device_id.get_ref() {
            // We currently don't let the UI set both a MIDI and OSC device. Although internally
            // this would be perfectly possible, it could be confusing.
            match App::get()
                .osc_device_manager()
                .borrow()
                .find_index_by_id(osc_device_id)
            {
                None => {
                    b.select_new_combo_box_item(format!("<Not present> ({})", osc_device_id));
                }
                Some(i) => b
                    .select_combo_box_item_by_data(OSC_INDEX_OFFSET + i as isize)
                    .unwrap(),
            };
            return;
        }
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
            .set_text_if_not_focused(main_state.search_expression.get_ref().as_str())
    }

    fn update_control_input(&self) {
        let selection_was_valid = {
            let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
            let session = self.session();
            let mut session = session.borrow_mut();
            match b.selected_combo_box_item_data() {
                -1 => {
                    session.osc_input_device_id.set(None);
                    session.midi_control_input.set(MidiControlInput::FxInput);
                    true
                }
                osc_dev_index if osc_dev_index >= OSC_INDEX_OFFSET => {
                    if let Some(dev) = App::get()
                        .osc_device_manager()
                        .borrow()
                        .find_device_by_index((osc_dev_index - OSC_INDEX_OFFSET) as usize)
                    {
                        // TODO-medium We should set this to None as soon as available.
                        session.midi_control_input.set(MidiControlInput::FxInput);
                        session.osc_input_device_id.set(Some(*dev.id()));
                        true
                    } else {
                        false
                    }
                }
                midi_dev_id if midi_dev_id >= 0 => {
                    let dev = Reaper::get()
                        .midi_input_device_by_id(MidiInputDeviceId::new(midi_dev_id as _));
                    session.osc_input_device_id.set(None);
                    session
                        .midi_control_input
                        .set(MidiControlInput::Device(dev));
                    true
                }
                _ => false,
            }
        };
        if !selection_was_valid {
            // This is most likely a section entry. Selection is not allowed.
            self.invalidate_control_input_combo_box_value();
        }
    }

    fn update_feedback_output(&self) {
        let selection_was_valid = {
            let b = self
                .view
                .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
            let session = self.session();
            let mut session = session.borrow_mut();
            match b.selected_combo_box_item_data() {
                -2 => {
                    session.osc_output_device_id.set(None);
                    session
                        .midi_feedback_output
                        .set(Some(MidiFeedbackOutput::FxOutput));
                    true
                }
                -1 => {
                    session.osc_output_device_id.set(None);
                    session.midi_feedback_output.set(None);
                    true
                }
                osc_dev_index if osc_dev_index >= OSC_INDEX_OFFSET => {
                    if let Some(dev) = App::get()
                        .osc_device_manager()
                        .borrow()
                        .find_device_by_index((osc_dev_index - OSC_INDEX_OFFSET) as usize)
                    {
                        session.midi_feedback_output.set(None);
                        session.osc_output_device_id.set(Some(*dev.id()));
                        true
                    } else {
                        false
                    }
                }
                midi_dev_id if midi_dev_id >= 0 => {
                    let dev = Reaper::get()
                        .midi_output_device_by_id(MidiOutputDeviceId::new(midi_dev_id as _));
                    session.osc_output_device_id.set(None);
                    session
                        .midi_feedback_output
                        .set(Some(MidiFeedbackOutput::Device(dev)));
                    true
                }
                _ => false,
            }
        };
        if !selection_was_valid {
            // This is most likely a section entry. Selection is not allowed.
            self.invalidate_feedback_output_combo_box_value();
        }
    }

    fn update_compartment(&self) {
        let mut main_state = self.main_state.borrow_mut();
        main_state.stop_filter_learning();
        main_state.active_compartment.set(
            self.view
                .require_control(root::ID_COMPARTMENT_COMBO_BOX)
                .selected_combo_box_item_index()
                .try_into()
                .expect("invalid compartment"),
        );
    }

    fn remove_group(&self) {
        let id = match self.main_state.borrow().group_filter.get() {
            Some(GroupFilter(id)) if !id.is_default() => id,
            _ => return,
        };
        let delete_mappings_result = if self.session().borrow().group_contains_mappings(id) {
            let msg = "Do you also want to delete all mappings in that group? If you choose no, they will be automatically moved to the default group.";
            self.view
                .require_window()
                .ask_yes_no_or_cancel("ReaLearn", msg)
        } else {
            Some(false)
        };
        if let Some(delete_mappings) = delete_mappings_result {
            self.main_state
                .borrow_mut()
                .group_filter
                .set(Some(GroupFilter(GroupId::default())));
            self.session()
                .borrow_mut()
                .remove_group(id, delete_mappings);
        }
    }

    fn edit_group(&self) {
        let weak_group = match self.main_state.borrow().group_filter.get() {
            Some(GroupFilter(id)) => {
                if id.is_default() {
                    Rc::downgrade(self.session().borrow().default_group())
                } else {
                    let session = self.session();
                    let session = session.borrow();
                    let group = session.find_group_by_id(id).expect("group not existing");
                    Rc::downgrade(group)
                }
            }
            _ => return,
        };
        let panel = GroupPanel::new(self.session.clone(), weak_group);
        let shared_panel = Rc::new(panel);
        if let Some(already_open_panel) =
            self.group_panel.borrow_mut().replace(shared_panel.clone())
        {
            already_open_panel.close();
        }
        shared_panel.open(self.view.require_window());
    }

    fn update_group(&self) {
        let group_filter = match self
            .view
            .require_control(root::ID_GROUP_COMBO_BOX)
            .selected_combo_box_item_data()
        {
            -2 => None,
            -1 => Some(GroupFilter(GroupId::default())),
            i if i >= 0 => {
                let session = self.session();
                let session = session.borrow();
                let group = session
                    .find_group_by_index_sorted(i as usize)
                    .expect("group not existing")
                    .borrow();
                Some(GroupFilter(group.id()))
            }
            _ => unreachable!(),
        };
        self.main_state.borrow_mut().group_filter.set(group_filter);
    }

    fn update_preset_auto_load_mode(&self) {
        self.main_state.borrow_mut().stop_filter_learning();
        let mode = self
            .view
            .require_control(root::ID_AUTO_LOAD_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid preset auto-load mode");
        let session = self.session();
        if mode != MainPresetAutoLoadMode::Off {
            if session.borrow().main_preset_is_out_of_date() {
                let msg = "Your mapping changes will be lost. Consider to save them first. Do you really want to continue?";
                if !self.view.require_window().confirm("ReaLearn", msg) {
                    self.invalidate_preset_auto_load_mode_combo_box();
                    return;
                }
            }
            self.panel_manager()
                .borrow_mut()
                .hide_all_with_compartment(MappingCompartment::MainMappings);
        }
        self.session()
            .borrow_mut()
            .activate_main_preset_auto_load_mode(mode, self.session.clone());
    }

    fn update_preset(&self) {
        self.main_state.borrow_mut().stop_filter_learning();
        let session = self.session();
        let compartment = self.active_compartment();
        let (preset_manager, mappings_are_dirty): (Box<dyn ExtendedPresetManager>, _) =
            match compartment {
                MappingCompartment::ControllerMappings => (
                    Box::new(App::get().controller_preset_manager()),
                    session.borrow().controller_preset_is_out_of_date(),
                ),
                MappingCompartment::MainMappings => (
                    Box::new(App::get().main_preset_manager()),
                    session.borrow().main_preset_is_out_of_date(),
                ),
            };
        if mappings_are_dirty {
            let msg = "Your mapping changes will be lost. Consider to save them first. Do you really want to continue?";
            if !self.view.require_window().confirm("ReaLearn", msg) {
                self.invalidate_preset_combo_box_value();
                return;
            }
        }
        let preset_id = match self
            .view
            .require_control(root::ID_PRESET_COMBO_BOX)
            .selected_combo_box_item_data()
        {
            -1 => None,
            i if i >= 0 => preset_manager.find_id_by_index(i as usize),
            _ => unreachable!(),
        };
        let mut session = session.borrow_mut();
        match compartment {
            MappingCompartment::ControllerMappings => {
                session
                    .activate_controller(preset_id, self.session.clone())
                    .unwrap();
            }
            MappingCompartment::MainMappings => session
                .activate_main_preset(preset_id, self.session.clone())
                .unwrap(),
        };
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
        self.view
            .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX)
            .set_checked(self.session().borrow().let_unmatched_events_through.get());
    }

    fn mappings_are_read_only(&self) -> bool {
        let session = self.session();
        let session = session.borrow();
        session.is_learning_many_mappings()
            || (self.active_compartment() == MappingCompartment::MainMappings
                && session.main_preset_auto_load_is_active())
    }

    fn invalidate_learn_many_button(&self) {
        let is_learning = self.session().borrow().is_learning_many_mappings();
        let learn_button_text = if is_learning { "Stop" } else { "Learn many" };
        let button = self
            .view
            .require_control(root::ID_LEARN_MANY_MAPPINGS_BUTTON);
        button.set_text(learn_button_text);
        let enabled = !(self.active_compartment() == MappingCompartment::MainMappings
            && self.session().borrow().main_preset_auto_load_is_active());
        button.set_enabled(enabled);
    }

    fn invalidate_add_one_button(&self) {
        self.view
            .require_control(root::ID_ADD_MAPPING_BUTTON)
            .set_enabled(!self.mappings_are_read_only());
    }

    fn invalidate_source_filter_buttons(&self) {
        let main_state = self.main_state.borrow();
        self.invalidate_filter_buttons(
            main_state.is_learning_source_filter.get(),
            main_state.source_filter.get_ref().is_some(),
            "Filter source",
            root::ID_FILTER_BY_SOURCE_BUTTON,
            root::ID_CLEAR_SOURCE_FILTER_BUTTON,
        );
    }

    fn invalidate_target_filter_buttons(&self) {
        let main_state = self.main_state.borrow();
        self.invalidate_filter_buttons(
            main_state.is_learning_target_filter.get(),
            main_state.target_filter.get_ref().is_some(),
            "Filter target",
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
        let plugin_parameters = self
            .plugin_parameters
            .upgrade()
            .expect("plugin params gone");
        plugin_parameters.apply_session_data(&session_data);
        Ok(())
    }

    pub fn export_to_clipboard(&self) {
        let plugin_parameters = self
            .plugin_parameters
            .upgrade()
            .expect("plugin params gone");
        let session_data = plugin_parameters.create_session_data();
        let json =
            serde_json::to_string_pretty(&session_data).expect("couldn't serialize session data");
        let mut clipboard: ClipboardContext =
            ClipboardProvider::new().expect("couldn't create clipboard");
        clipboard
            .set_contents(json)
            .expect("couldn't set clipboard contents");
    }

    fn delete_active_preset(&self) -> Result<(), &'static str> {
        if !self
            .view
            .require_window()
            .confirm("ReaLearn", "Do you really want to remove this preset?")
        {
            return Ok(());
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        let compartment = self.active_compartment();
        let (mut preset_manager, active_preset_id): (Box<dyn ExtendedPresetManager>, _) =
            match compartment {
                MappingCompartment::ControllerMappings => (
                    Box::new(App::get().controller_preset_manager()),
                    session.active_controller_id(),
                ),
                MappingCompartment::MainMappings => (
                    Box::new(App::get().main_preset_manager()),
                    session.active_main_preset_id(),
                ),
            };
        let active_preset_id = active_preset_id.ok_or("no preset selected")?.to_string();
        match compartment {
            MappingCompartment::ControllerMappings => {
                session.activate_controller(None, self.session.clone())?
            }
            MappingCompartment::MainMappings => {
                session.activate_main_preset(None, self.session.clone())?
            }
        };
        preset_manager.remove_preset(&active_preset_id)?;
        Ok(())
    }

    fn reload_all_presets(&self) {
        let _ = App::get()
            .controller_preset_manager()
            .borrow_mut()
            .load_presets();
        let _ = App::get().main_preset_manager().borrow_mut().load_presets();
    }

    fn save_active_preset(&self) -> Result<(), &'static str> {
        let session = self.session();
        let (context, mut mappings, preset_id, compartment) = {
            let session = session.borrow();
            let compartment = self.active_compartment();
            let preset_id = match compartment {
                MappingCompartment::ControllerMappings => session.active_controller_id(),
                MappingCompartment::MainMappings => session.active_main_preset_id(),
            };
            let preset_id = match preset_id {
                None => return Err("no active preset"),
                Some(id) => id,
            };
            let mappings: Vec<_> = session
                .mappings(compartment)
                .map(|ptr| ptr.borrow().clone())
                .collect();
            (
                session.context().clone(),
                mappings,
                preset_id.to_owned(),
                compartment,
            )
        };
        self.make_mappings_project_independent_if_desired(context, &mut mappings);
        let session = session.borrow();
        match compartment {
            MappingCompartment::ControllerMappings => {
                let preset_manager = App::get().controller_preset_manager();
                let mut controller = preset_manager
                    .find_by_id(&preset_id)
                    .ok_or("controller not found")?;
                controller.update_realearn_data(mappings);
                preset_manager.borrow_mut().update_preset(controller)?;
            }
            MappingCompartment::MainMappings => {
                let preset_manager = App::get().main_preset_manager();
                let mut main_preset = preset_manager
                    .find_by_id(&preset_id)
                    .ok_or("main preset not found")?;
                let default_group = session.default_group().borrow().clone();
                let groups = session.groups().map(|ptr| ptr.borrow().clone()).collect();
                main_preset.update_data(default_group, groups, mappings);
                preset_manager.borrow_mut().update_preset(main_preset)?;
            }
        };
        Ok(())
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
        if App::get().has_session(&new_session_id) {
            self.view.require_window().alert(
                "ReaLearn",
                "There's another open ReaLearn session which already has this session ID!",
            );
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

    /// Don't borrow the session while calling this!
    fn make_mappings_project_independent_if_desired(
        &self,
        context: ProcessorContext,
        mut mappings: &mut [MappingModel],
    ) {
        let msg = "Some of the mappings have references to this particular project. This usually doesn't make too much sense for a preset that's supposed to be reusable among different projects. Do you want ReaLearn to automatically adjust the mappings so that track targets refer to tracks by their position and FX targets relate to whatever FX is currently focused?";
        if mappings_have_project_references(&mappings)
            && self.view.require_window().confirm("ReaLearn", msg)
        {
            make_mappings_project_independent(&mut mappings, &context);
        }
    }

    fn save_as_preset(&self) -> Result<(), &'static str> {
        let session = self.session();
        let (context, mut mappings, compartment) = {
            let session = session.borrow_mut();
            let compartment = self.active_compartment();
            let mappings: Vec<_> = session
                .mappings(compartment)
                .map(|ptr| ptr.borrow().clone())
                .collect();
            (session.context().clone(), mappings, compartment)
        };
        self.make_mappings_project_independent_if_desired(context, &mut mappings);
        let preset_name = match dialog_util::prompt_for("Preset name", "") {
            None => return Ok(()),
            Some(n) => n,
        };
        let preset_id = slug::slugify(&preset_name);
        let mut session = session.borrow_mut();
        match compartment {
            MappingCompartment::ControllerMappings => {
                let custom_data = session
                    .active_controller()
                    .map(|c| c.custom_data().clone())
                    .unwrap_or_default();
                let controller =
                    ControllerPreset::new(preset_id.clone(), preset_name, mappings, custom_data);
                App::get()
                    .controller_preset_manager()
                    .borrow_mut()
                    .add_preset(controller)?;
                session.activate_controller(Some(preset_id), self.session.clone())?;
            }
            MappingCompartment::MainMappings => {
                let default_group = session.default_group().borrow().clone();
                let groups = session.groups().map(|ptr| ptr.borrow().clone()).collect();
                let main_preset = MainPreset::new(
                    preset_id.clone(),
                    preset_name,
                    default_group,
                    groups,
                    mappings,
                );
                App::get()
                    .main_preset_manager()
                    .borrow_mut()
                    .add_preset(main_preset)?;
                session.activate_main_preset(Some(preset_id), self.session.clone())?;
            }
        };
        Ok(())
    }

    fn reset(&self) {
        self.main_state
            .borrow_mut()
            .group_filter
            .set(Some(GroupFilter(GroupId::default())));
        if let Some(already_open_panel) = self.group_panel.borrow().as_ref() {
            already_open_panel.close();
        }
        self.group_panel.replace(None);
        self.invalidate_all_controls();
    }

    fn log_debug_info(&self) {
        let session = self.session();
        let session = session.borrow();
        session.log_debug_info();
        App::get().log_debug_info(session.id());
    }

    fn open_user_guide_offline(&self) {
        let user_guide_pdf = App::realearn_data_dir_path().join("doc/realearn-user-guide.pdf");
        if open::that(user_guide_pdf).is_err() {
            self.view.require_window().alert(
                "ReaLearn",
                "Couldn't open offline user guide. Please try the online version!",
            )
        }
    }

    fn open_user_guide_online(&self) {
        self.open_in_browser("https://github.com/helgoboss/realearn/blob/master/doc/user-guide.md");
    }

    fn donate(&self) {
        self.open_in_browser("https://paypal.me/helgoboss");
    }

    fn open_forum(&self) {
        self.open_in_browser("https://forum.cockos.com/showthread.php?t=178015");
    }

    fn contact_developer(&self) {
        self.open_in_browser("mailto:info@helgoboss.org");
    }

    fn open_website(&self) {
        self.open_in_browser("https://www.helgoboss.org/projects/realearn/");
    }

    fn open_in_browser(&self, url: &str) {
        if webbrowser::open(url).is_err() {
            Reaper::get().show_console_msg(
                format!("Couldn't open browser. Please open the following address in your browser manually:\n\n{}\n\n", url)
            );
        }
    }

    fn register_listeners(self: SharedView<Self>) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        self.when(session.everything_changed(), |view| {
            view.reset();
        });
        self.when(session.let_matched_events_through.changed(), |view| {
            view.invalidate_let_matched_events_through_check_box();
        });
        self.when(session.let_unmatched_events_through.changed(), |view| {
            view.invalidate_let_unmatched_events_through_check_box();
        });
        self.when(session.learn_many_state_changed(), |view| {
            view.invalidate_all_controls();
        });
        self.when(
            session
                .midi_control_input
                .changed()
                .merge(session.osc_input_device_id.changed()),
            |view| {
                view.invalidate_control_input_combo_box();
                view.invalidate_let_matched_events_through_check_box();
                view.invalidate_let_unmatched_events_through_check_box();
                let shared_session = view.session();
                let mut session = shared_session.borrow_mut();
                if session.auto_correct_settings.get() {
                    let osc_control_input = session.osc_input_device_id.get();
                    let midi_control_input = session.midi_control_input.get();
                    session.send_feedback_only_if_armed.set(
                        osc_control_input.is_none()
                            && midi_control_input == MidiControlInput::FxInput,
                    );
                }
            },
        );
        self.when(
            session
                .midi_feedback_output
                .changed()
                .merge(session.osc_output_device_id.changed()),
            |view| view.invalidate_feedback_output_combo_box(),
        );
        self.when(session.group_changed(), |view| {
            view.invalidate_group_controls();
        });
        let main_state = self.main_state.borrow();
        self.when(main_state.group_filter.changed(), |view| {
            view.invalidate_group_controls();
        });
        self.when(main_state.search_expression.changed(), |view| {
            view.invoke_programmatically(|| {
                view.invalidate_search_expression();
            });
        });
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
            view.invalidate_all_controls();
        });
        self.when(session.main_preset_auto_load_mode.changed(), |view| {
            view.invalidate_all_controls();
        });
        self.when(session.group_list_changed(), |view| {
            view.invalidate_group_controls();
        });
        when(
            App::get()
                .controller_preset_manager()
                .borrow()
                .changed()
                .merge(App::get().main_preset_manager().borrow().changed())
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_async(move |view, _| {
            view.invalidate_preset_controls();
        });
        when(
            App::get()
                .osc_device_manager()
                .borrow()
                .changed()
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_async(move |view, _| {
            view.invalidate_control_input_combo_box();
            view.invalidate_feedback_output_combo_box();
        });
        // TODO-medium This is lots of stuff done whenever changing just something small in a
        // mapping  or group. Maybe micro optimization, I don't know. Alternatively we could
        // just set a  dirty flag once something changed and reset it after saving!
        // Mainly enables/disables save button depending on dirty state.
        when(
            session
                .mapping_list_changed()
                .map_to(())
                .merge(session.mapping_changed().map_to(()))
                .merge(session.group_list_changed())
                .merge(session.group_changed())
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_sync(move |view, _| {
            view.invalidate_preset_buttons();
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

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_HEADER_PANEL
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

    fn closed(self: SharedView<Self>, _window: Window) {
        self.main_state.borrow_mut().stop_filter_learning();
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_GROUP_ADD_BUTTON => self.add_group(),
            ID_GROUP_DELETE_BUTTON => self.remove_group(),
            ID_GROUP_EDIT_BUTTON => self.edit_group(),
            ID_ADD_MAPPING_BUTTON => self.add_mapping(),
            ID_LEARN_MANY_MAPPINGS_BUTTON => {
                self.toggle_learn_many_mappings();
            }
            ID_FILTER_BY_SOURCE_BUTTON => self.toggle_learn_source_filter(),
            ID_FILTER_BY_TARGET_BUTTON => self.toggle_learn_target_filter(),
            ID_CLEAR_SOURCE_FILTER_BUTTON => self.clear_source_filter(),
            ID_CLEAR_TARGET_FILTER_BUTTON => self.clear_target_filter(),
            ID_CLEAR_SEARCH_BUTTON => self.clear_search_expression(),
            ID_IMPORT_BUTTON => {
                if let Err(msg) = self.import_from_clipboard() {
                    self.view.require_window().alert("ReaLearn", msg);
                }
            }
            ID_EXPORT_BUTTON => self.export_to_clipboard(),
            ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX => self.update_let_matched_events_through(),
            ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX => self.update_let_unmatched_events_through(),
            ID_PRESET_DELETE_BUTTON => {
                self.delete_active_preset().unwrap();
            }
            ID_PRESET_SAVE_AS_BUTTON => {
                self.save_as_preset().unwrap();
            }
            ID_PRESET_SAVE_BUTTON => {
                self.save_active_preset().unwrap();
            }
            ID_PRESET_RELOAD_ALL_BUTTON => {
                self.reload_all_presets();
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
            ID_CONTROL_DEVICE_COMBO_BOX => self.update_control_input(),
            ID_FEEDBACK_DEVICE_COMBO_BOX => self.update_feedback_output(),
            ID_COMPARTMENT_COMBO_BOX => self.update_compartment(),
            ID_GROUP_COMBO_BOX => self.update_group(),
            ID_AUTO_LOAD_COMBO_BOX => self.update_preset_auto_load_mode(),
            ID_PRESET_COMBO_BOX => self.update_preset(),
            _ => unreachable!(),
        }
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        if self.is_invoked_programmatically() {
            // We don't want to continue if the edit control change was not caused by the user.
            // Although the edit control text is changed programmatically, it also triggers the
            // change handler. Ignore it! Most of those events are filtered out already
            // by the dialog proc reentrancy check, but this one is not because the
            // dialog proc is not reentered - we are just reacting (async) to a change.
            return false;
        }
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
        let ctx_menu = menu_bar.get_menu(0).expect("menu bar didn't have 1st menu");
        let app = App::get();
        // Invalidate some menu items without result
        {
            let session = self.session();
            let session = session.borrow();
            // Invalidate "Send feedback only if track armed"
            {
                let item_id = root::IDM_SEND_FEEDBACK_ONLY_IF_TRACK_ARMED;
                let (enabled, checked) = if session.containing_fx_is_in_input_fx_chain() {
                    (false, true)
                } else {
                    (true, session.send_feedback_only_if_armed.get())
                };
                ctx_menu.set_item_enabled(item_id, enabled);
                ctx_menu.set_item_checked(item_id, checked);
            }
            // Invalidate "Auto-correct settings"
            {
                ctx_menu.set_item_checked(
                    root::IDM_AUTO_CORRECT_SETTINGS,
                    session.auto_correct_settings.get(),
                );
            }
        }
        // Invalidate "Link current preset to FX"
        let next_preset_fx_link_action = {
            let item_id = root::IDM_LINK_TO_FX;
            let action = determine_next_preset_fx_link_action(
                app,
                &self.session().borrow(),
                self.active_compartment(),
            );
            ctx_menu.set_item_enabled(item_id, action.is_some());
            ctx_menu.set_item_checked(
                item_id,
                matches!(action, Some(PresetFxLinkAction::UnlinkFrom { .. })),
            );
            let label = match &action {
                None => "Link current preset fo FX...".to_string(),
                Some(PresetFxLinkAction::LinkTo { fx_id: fx_name, .. }) => {
                    format!("Link current preset to FX \"{}\"", fx_name)
                }
                Some(PresetFxLinkAction::UnlinkFrom { fx_id: fx_name, .. }) => {
                    format!("Unlink current preset from FX \"{}\"", fx_name)
                }
            };
            ctx_menu.set_item_text(item_id, label);
            action
        };
        // Invalidate "Server enabled"
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
            ctx_menu.set_item_checked(root::IDM_SERVER_START, server_is_enabled);
            (next_server_action, server.http_port(), server.https_port())
        };
        // Invalidate "OSC devices"
        let devs_menu = {
            use std::iter::once;
            use swell_ui::menu_tree::*;
            let dev_manager = App::get().osc_device_manager();
            let dev_manager = dev_manager.borrow();
            let parent_window = self.view.require_window();
            let entries =
                once(item("<New>", edit_new_osc_device)).chain(dev_manager.devices().map(|dev| {
                    let dev_id = *dev.id();
                    menu(
                        dev.name(),
                        vec![
                            item("Edit...", move || edit_existing_osc_device(dev_id)),
                            item("Remove", move || remove_osc_device(parent_window, dev_id)),
                            item_with_opts(
                                "Enabled for control",
                                ItemOpts {
                                    enabled: true,
                                    checked: dev.is_enabled_for_control(),
                                },
                                move || {
                                    App::get().do_with_osc_device(dev_id, |d| d.toggle_control())
                                },
                            ),
                            item_with_opts(
                                "Enabled for feedback",
                                ItemOpts {
                                    enabled: true,
                                    checked: dev.is_enabled_for_feedback(),
                                },
                                move || {
                                    App::get().do_with_osc_device(dev_id, |d| d.toggle_feedback())
                                },
                            ),
                            item_with_opts(
                                "Can deal with OSC bundles",
                                ItemOpts {
                                    enabled: true,
                                    checked: dev.can_deal_with_bundles(),
                                },
                                move || {
                                    App::get().do_with_osc_device(dev_id, |d| {
                                        d.toggle_can_deal_with_bundles()
                                    })
                                },
                            ),
                        ],
                    )
                }));
            let mut m = root_menu(entries.collect());
            let swell_menu = ctx_menu.turn_into_submenu(root::IDM_OSC_DEVICES);
            m.index(50_000);
            fill_menu(swell_menu, &m);
            m
        };
        // Open menu
        let result = match self
            .view
            .require_window()
            .open_popup_menu(ctx_menu, location)
        {
            None => return,
            Some(r) => r,
        };
        // Execute action
        match result {
            root::IDM_DONATE => self.donate(),
            root::IDM_USER_GUIDE_OFFLINE => self.open_user_guide_offline(),
            root::IDM_USER_GUIDE_ONLINE => self.open_user_guide_online(),
            root::IDM_FORUM => self.open_forum(),
            root::IDM_CONTACT_DEVELOPER => self.contact_developer(),
            root::IDM_WEBSITE => self.open_website(),
            root::IDM_LOG_DEBUG_INFO => self.log_debug_info(),
            root::IDM_SEND_FEEDBACK_NOW => self.session().borrow().send_all_feedback(),
            root::IDM_AUTO_CORRECT_SETTINGS => self.toggle_always_auto_detect(),
            root::IDM_LINK_TO_FX => {
                use PresetFxLinkAction::*;
                let manager = app.preset_link_manager();
                match next_preset_fx_link_action.expect("impossible") {
                    LinkTo { preset_id, fx_id } => {
                        manager.borrow_mut().link_preset_to_fx(preset_id, fx_id)
                    }
                    UnlinkFrom { preset_id, .. } => {
                        manager.borrow_mut().unlink_preset_from_fx(&preset_id)
                    }
                }
            }
            root::IDM_SEND_FEEDBACK_ONLY_IF_TRACK_ARMED => {
                self.toggle_send_feedback_only_if_armed()
            }
            root::IDM_CHANGE_SESSION_ID => {
                self.change_session_id();
            }
            root::IDM_SERVER_START => {
                use ServerAction::*;
                match next_server_action {
                    Start => {
                        match App::start_server_persistently(app) {
                            Ok(_) => {
                                self.view
                                    .require_window()
                                    .alert("ReaLearn", "Successfully started projection server.");
                            }
                            Err(info) => {
                                warn_about_failed_server_start(info);
                            }
                        };
                    }
                    Disable => {
                        app.disable_server_persistently();
                        self.view.require_window().alert(
                            "ReaLearn",
                            "Disabled projection server. This will take effect on the next start of REAPER.",
                        );
                    }
                    Enable => {
                        app.enable_server_persistently();
                        self.view
                            .require_window()
                            .alert("ReaLearn", "Enabled projection server again.");
                    }
                }
            }
            root::IDM_SERVER_ADD_FIREWALL_RULE => {
                let msg = match add_firewall_rule(http_port, https_port) {
                    Ok(_) => "Successfully added firewall rule.".to_string(),
                    Err(reason) => format!(
                        "Couldn't add firewall rule because {}. Please try to do it manually!",
                        reason
                    ),
                };
                self.view.require_window().alert("ReaLearn", msg);
            }
            _ => {
                devs_menu
                    .find_item_by_id(result)
                    .expect("selected menu item not found")
                    .invoke_handler();
            }
        };
    }
}

fn get_midi_input_device_label(dev: MidiInputDevice) -> String {
    get_midi_device_label(
        dev.name(),
        dev.id().get(),
        MidiDeviceStatus::from_flags(dev.is_open(), dev.is_connected()),
    )
}

fn get_midi_output_device_label(dev: MidiOutputDevice) -> String {
    get_midi_device_label(
        dev.name(),
        dev.id().get(),
        MidiDeviceStatus::from_flags(dev.is_open(), dev.is_connected()),
    )
}

#[derive(Display)]
enum MidiDeviceStatus {
    #[display(fmt = " <disconnected>")]
    Disconnected,
    #[display(fmt = " <connected but disabled>")]
    ConnectedButDisabled,
    #[display(fmt = "")]
    Connected,
}

impl MidiDeviceStatus {
    fn from_flags(open: bool, connected: bool) -> MidiDeviceStatus {
        use MidiDeviceStatus::*;
        match (open, connected) {
            (false, false) => Disconnected,
            (false, true) => ConnectedButDisabled,
            // Shouldn't happen but cope with it.
            (true, false) => Disconnected,
            (true, true) => Connected,
        }
    }
}

fn get_midi_device_label(name: ReaperString, raw_id: u8, status: MidiDeviceStatus) -> String {
    format!(
        "{}. {}{}",
        raw_id,
        // Here we don't rely on the string to be UTF-8 because REAPER doesn't have influence on
        // how MIDI devices encode their name. Indeed a user reported an error related to that:
        // https://github.com/helgoboss/realearn/issues/78
        name.into_inner().to_string_lossy(),
        status
    )
}

fn get_osc_device_label(dev: &OscDevice, is_output: bool) -> String {
    format!(
        "{}{}",
        dev.name(),
        if is_output {
            dev.output_status()
        } else {
            dev.input_status()
        }
    )
}

impl Drop for HeaderPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping header panel...");
    }
}

enum PresetFxLinkAction {
    LinkTo { preset_id: String, fx_id: FxId },
    UnlinkFrom { preset_id: String, fx_id: FxId },
}

fn determine_next_preset_fx_link_action(
    app: &App,
    session: &Session,
    compartment: MappingCompartment,
) -> Option<PresetFxLinkAction> {
    if compartment != MappingCompartment::MainMappings {
        // Not relevant for controller mappings.
        return None;
    }
    // No preset means nothing can be linked.
    let preset_id = session.active_main_preset_id()?;
    if let Some(fx_id) = app
        .preset_link_manager()
        .borrow()
        .find_fx_that_preset_is_linked_to(preset_id)
    {
        Some(PresetFxLinkAction::UnlinkFrom {
            preset_id: preset_id.to_string(),
            fx_id,
        })
    } else {
        let last_fx = app.previously_focused_fx()?;
        if !last_fx.is_available() {
            return None;
        }
        Some(PresetFxLinkAction::LinkTo {
            preset_id: preset_id.to_string(),
            fx_id: FxId::from_fx(&last_fx),
        })
    }
}

fn generate_midi_device_heading() -> String {
    "----  MIDI  ----".to_owned()
}

fn generate_osc_device_heading(device_count: usize) -> String {
    format!(
        "----  OSC  ----{}",
        if device_count == 0 {
            " (add devices via right-click menu)"
        } else {
            ""
        }
    )
}

fn edit_new_osc_device() {
    let dev = match edit_osc_device(OscDevice::default()) {
        Ok(d) => d,
        Err(EditOscDevError::Cancelled) => return,
        res => res.unwrap(),
    };
    App::get()
        .osc_device_manager()
        .borrow_mut()
        .add_device(dev)
        .unwrap();
}

fn edit_existing_osc_device(dev_id: OscDeviceId) {
    let dev = App::get()
        .osc_device_manager()
        .borrow()
        .find_device_by_id(&dev_id)
        .unwrap()
        .clone();
    let dev = match edit_osc_device(dev) {
        Ok(d) => d,
        Err(EditOscDevError::Cancelled) => return,
        res => res.unwrap(),
    };
    App::get()
        .osc_device_manager()
        .borrow_mut()
        .update_device(dev)
        .unwrap();
}

fn remove_osc_device(parent_window: Window, dev_id: OscDeviceId) {
    if !parent_window.confirm(
        "ReaLearn",
        "Do you really want to remove this OSC device? This is a global action. As a consequence, all existing ReaLearn instances which use this device will point to a device that doesn't exist anymore.",
    ) {
        return;
    }
    App::get()
        .osc_device_manager()
        .borrow_mut()
        .remove_device_by_id(dev_id)
        .unwrap();
}

#[derive(Debug)]
enum EditOscDevError {
    Cancelled,
    Unexpected(&'static str),
}

fn edit_osc_device(mut dev: OscDevice) -> Result<OscDevice, EditOscDevError> {
    let csv = Reaper::get()
        .medium_reaper()
        .get_user_inputs(
            "ReaLearn",
            4,
            "Name,Local port (e.g. 7878),Device host (e.g. 192.168.x.y),Device port (e.g. 7878),separator=;,extrawidth=80",
            format!(
                "{};{};{};{}",
                dev.name(),
                dev.local_port().map(|p| p.to_string()).unwrap_or_default(),
                dev.device_host().map(|a| a.to_string()).unwrap_or_default(),
                dev.device_port().map(|p| p.to_string()).unwrap_or_default(),
            ),
            512,
        )
        .ok_or(EditOscDevError::Cancelled)?;
    let splitted: Vec<_> = csv.to_str().split(';').collect();
    if let [name, local_port, device_host, device_port] = splitted.as_slice() {
        dev.set_name(name.to_string());
        dev.set_local_port(local_port.parse::<u16>().ok());
        dev.set_device_host(device_host.parse::<Ipv4Addr>().ok());
        dev.set_device_port(device_port.parse::<u16>().ok());
        Ok(dev)
    } else {
        Err(EditOscDevError::Unexpected("couldn't split"))
    }
}
