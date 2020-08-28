use crate::core::{when, Prop};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::ui::constants::symbols;
use crate::infrastructure::ui::MainPanel;

use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    ControlValue, MidiClockTransportMessage, SourceCharacter, SymmetricUnitValue, Target, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use reaper_high::Reaper;
use reaper_low::raw;
use reaper_medium::{InitialAction, MessageBoxType, PromptForActionResult, SectionId};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::convert::TryInto;

use std::iter;

use std::ptr::null;
use std::rc::Rc;

use crate::application::{
    convert_factor_to_unit_value, convert_unit_value_to_factor, get_fx_label, get_fx_param_label,
    ActivationType, MappingModel, MidiSourceModel, MidiSourceType, ModeModel, ModeType,
    ModifierConditionModel, Session, SharedMapping, SharedSession, TargetModel,
    TargetModelWithContext, TargetType, VirtualTrack, WeakSession,
};
use crate::domain::{ActionInvocationType, ReaperTarget, TargetCharacter, PLUGIN_PARAMETER_COUNT};
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, WeakView, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct MappingPanel {
    view: ViewContext,
    session: WeakSession,
    mapping: RefCell<Option<SharedMapping>>,
    main_panel: WeakView<MainPanel>,
    is_invoked_programmatically: Cell<bool>,
    target_value_change_subscription: RefCell<SubscriptionGuard<Box<dyn SubscriptionLike>>>,
    sliders: RefCell<Option<Sliders>>,
    // Fires when a mapping is about to change or the panel is hidden.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
}

// TODO-low Is it enough to have a MutableMappingPanel?
struct ImmutableMappingPanel<'a> {
    session: &'a Session,
    mapping_ptr: *const MappingModel,
    mapping: &'a MappingModel,
    source: &'a MidiSourceModel,
    mode: &'a ModeModel,
    target: &'a TargetModel,
    view: &'a ViewContext,
    panel: &'a SharedView<MappingPanel>,
}

struct MutableMappingPanel<'a> {
    session: &'a mut Session,
    mapping: &'a mut MappingModel,
    shared_mapping: &'a SharedMapping,
    view: &'a ViewContext,
    panel: &'a SharedView<MappingPanel>,
}

#[derive(Debug)]
struct Sliders {
    mode_min_target_value: Window,
    mode_max_target_value: Window,
    mode_min_source_value: Window,
    mode_max_source_value: Window,
    mode_min_step_size: Window,
    mode_max_step_size: Window,
    mode_min_jump: Window,
    mode_max_jump: Window,
    target_value: Window,
}

impl MappingPanel {
    pub fn new(session: WeakSession, main_panel: WeakView<MainPanel>) -> MappingPanel {
        MappingPanel {
            view: Default::default(),
            session,
            mapping: None.into(),
            main_panel,
            is_invoked_programmatically: false.into(),
            target_value_change_subscription: RefCell::new(SubscriptionGuard::new(Box::new(
                LocalSubscription::default(),
            ))),
            sliders: None.into(),
            party_is_over_subject: Default::default(),
        }
    }

    pub fn is_free(&self) -> bool {
        self.mapping.borrow().is_none()
    }

    pub fn mapping_ptr(&self) -> *const MappingModel {
        match self.mapping.borrow().as_ref() {
            None => null(),
            Some(m) => m.as_ptr() as _,
        }
    }

    pub fn scroll_to_mapping_in_main_panel(&self) {
        self.main_panel
            .upgrade()
            .expect("main view gone")
            .scroll_to_mapping(self.mapping_ptr());
    }

    pub fn hide(&self) {
        self.stop_party();
        self.view.require_window().hide();
        self.mapping.replace(None);
    }

    pub fn show(self: SharedView<Self>, mapping: SharedMapping) {
        self.invoke_programmatically(|| {
            self.stop_party();
            self.mapping.replace(Some(mapping));
            self.clone().start_party();
            self.bring_to_foreground();
        });
    }

    pub fn bring_to_foreground(&self) {
        let window = self.view.require_window();
        window.hide();
        window.show();
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

    /// Unregisters listeners.
    fn stop_party(&self) {
        self.party_is_over_subject.borrow_mut().next(());
    }

    /// Invalidates everything and registers listeners.
    fn start_party(self: SharedView<Self>) {
        self.read(|p| {
            p.fill_all_controls();
            p.invalidate_all_controls();
            p.register_listeners();
        })
        .expect("mapping must be filled at this point");
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn read<R>(
        self: SharedView<Self>,
        op: impl Fn(&ImmutableMappingPanel) -> R,
    ) -> Result<R, &'static str> {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let shared_mapping = self.mapping.borrow();
        let shared_mapping = shared_mapping.as_ref().ok_or("mapping not filled")?;
        let mapping = shared_mapping.borrow();
        let p = ImmutableMappingPanel {
            session: &session,
            mapping_ptr: shared_mapping.as_ptr(),
            mapping: &mapping,
            source: &mapping.source_model,
            mode: &mapping.mode_model,
            target: &mapping.target_model,
            view: &self.view,
            panel: &self,
        };
        Ok(op(&p))
    }

    fn write<R>(self: SharedView<Self>, op: impl Fn(&mut MutableMappingPanel) -> R) -> R {
        let shared_session = self.session();
        let mut session = shared_session.borrow_mut();
        let mut shared_mapping = self.mapping.borrow_mut();
        let shared_mapping = shared_mapping.as_mut().expect("mapping not filled");
        let mut mapping = shared_mapping.borrow_mut();
        let mut p = MutableMappingPanel {
            session: &mut session,
            mapping: &mut mapping,
            shared_mapping: &shared_mapping,
            view: &self.view,
            panel: &self,
        };
        op(&mut p)
    }

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }

    fn memorize_all_slider_controls(&self) {
        let view = &self.view;
        let sliders = Sliders {
            mode_min_target_value: view
                .require_control(root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL),
            mode_max_target_value: view
                .require_control(root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL),
            mode_min_source_value: view
                .require_control(root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL),
            mode_max_source_value: view
                .require_control(root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL),
            mode_min_step_size: view
                .require_control(root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL),
            mode_max_step_size: view
                .require_control(root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL),
            mode_min_jump: view.require_control(root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL),
            mode_max_jump: view.require_control(root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL),
            target_value: view.require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL),
        };
        self.sliders.replace(Some(sliders));
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn when_do_sync(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(&ImmutableMappingPanel) + 'static + Copy,
    ) {
        when(event.take_until(self.party_is_over()))
            .with(Rc::downgrade(self))
            .do_sync(decorate_reaction(reaction));
    }

    fn when_do_async(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(&ImmutableMappingPanel) + 'static + Copy,
    ) -> SubscriptionWrapper<impl SubscriptionLike> {
        when(event.take_until(self.party_is_over()))
            .with(Rc::downgrade(self))
            .do_async(decorate_reaction(reaction))
    }
}

fn decorate_reaction(
    reaction: impl Fn(&ImmutableMappingPanel) + 'static + Copy,
) -> impl Fn(Rc<MappingPanel>, ()) + Copy {
    move |view, _| {
        let view_mirror = view.clone();
        view_mirror.is_invoked_programmatically.set(true);
        scopeguard::defer! { view_mirror.is_invoked_programmatically.set(false); }
        // If the reaction can't be displayed anymore because the mapping is not filled anymore,
        // so what.
        let _ = view.read(reaction);
    }
}

impl<'a> MutableMappingPanel<'a> {
    fn real_target(&self) -> Option<ReaperTarget> {
        self.mapping
            .target_model
            .with_context(self.session.context())
            .create_target()
            .ok()
    }

    fn open_target(&self) {
        if let Some(t) = self.real_target() {
            Reaper::get()
                .do_later_in_main_thread_asap(move || t.open())
                .unwrap();
        }
    }

    fn pick_action(&self) {
        let reaper = Reaper::get().medium_reaper();
        use InitialAction::*;
        let initial_action = match self.mapping.target_model.action.get_ref().as_ref() {
            None => NoneSelected,
            Some(a) => Selected(a.command_id()),
        };
        // TODO-low Add this to reaper-high with rxRust
        if reaper.low().pointers().PromptForAction.is_none() {
            reaper.show_message_box(
                "Please update to REAPER >= 6.12 in order to pick actions!",
                "ReaLearn",
                MessageBoxType::Okay,
            );
            return;
        }
        reaper.prompt_for_action_create(initial_action, SectionId::new(0));
        let shared_mapping = self.shared_mapping.clone();
        Reaper::get()
            .main_thread_idle()
            .take_until(self.panel.party_is_over())
            .map(|_| {
                Reaper::get()
                    .medium_reaper()
                    .prompt_for_action_poll(SectionId::new(0))
            })
            .filter(|r| *r != PromptForActionResult::NoneSelected)
            .take_while(|r| *r != PromptForActionResult::ActionWindowGone)
            .subscribe_complete(
                move |r| {
                    if let PromptForActionResult::Selected(command_id) = r {
                        let action = Reaper::get()
                            .main_section()
                            .action_by_command_id(command_id);
                        shared_mapping
                            .borrow_mut()
                            .target_model
                            .action
                            .set(Some(action));
                    }
                },
                || {
                    Reaper::get()
                        .medium_reaper()
                        .prompt_for_action_finish(SectionId::new(0));
                },
            );
    }

    fn toggle_learn_source(&mut self) {
        self.session.toggle_learn_source(&self.shared_mapping);
    }

    fn update_mapping_control_enabled(&mut self) {
        self.mapping.control_is_enabled.set(
            self.view
                .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_activation_setting_1_on(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX)
            .is_checked();
        self.mapping
            .modifier_condition_1
            .set_with(|prev| prev.with_is_on(checked));
    }

    fn update_mapping_activation_setting_2_on(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX)
            .is_checked();
        self.mapping
            .modifier_condition_2
            .set_with(|prev| prev.with_is_on(checked));
    }

    fn update_mapping_feedback_enabled(&mut self) {
        self.mapping.feedback_is_enabled.set(
            self.view
                .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_prevent_echo_feedback(&mut self) {
        self.mapping.prevent_echo_feedback.set(
            self.view
                .require_control(root::ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_name(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping.name.set(value);
    }

    fn update_mapping_activation_eel_condition(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping.eel_condition.set(value);
    }

    fn update_source_is_registered(&mut self) {
        self.mapping.source_model.is_registered.set(Some(
            self.view
                .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
                .is_checked(),
        ));
    }

    fn update_source_is_14_bit(&mut self) {
        self.mapping.source_model.is_14_bit.set(Some(
            self.view
                .require_control(root::ID_SOURCE_14_BIT_CHECK_BOX)
                .is_checked(),
        ));
    }

    fn update_mapping_activation_type(&mut self) {
        let b = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX);
        self.mapping.activation_type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid activation type"),
        );
    }

    fn update_source_channel(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(Channel::new(id as _)),
        };
        self.mapping.source_model.channel.set(value);
    }

    fn update_mapping_activation_setting_1_option(&mut self) {
        use ActivationType::*;
        match self.mapping.activation_type.get() {
            Modifiers => {
                self.update_mapping_activation_setting_option(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    |s| &mut s.mapping.modifier_condition_1,
                );
            }
            Program => {
                let b = self
                    .view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX);
                let value = b.selected_combo_box_item_index() as u32;
                self.mapping
                    .program_condition
                    .set_with(|prev| prev.with_param_index(value));
            }
            _ => {}
        };
    }

    fn update_mapping_activation_setting_2_option(&mut self) {
        use ActivationType::*;
        match self.mapping.activation_type.get() {
            Modifiers => {
                self.update_mapping_activation_setting_option(
                    root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
                    |s| &mut s.mapping.modifier_condition_2,
                );
            }
            Program => {
                let b = self
                    .view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX);
                let value = b.selected_combo_box_item_index() as u32;
                self.mapping
                    .program_condition
                    .set_with(|prev| prev.with_program_index(value));
            }
            _ => {}
        };
    }

    fn update_mapping_activation_setting_option(
        &mut self,
        combo_box_id: u32,
        prop: impl Fn(&mut Self) -> &mut Prop<ModifierConditionModel>,
    ) {
        let b = self.view.require_control(combo_box_id);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(id as u32),
        };
        prop(self).set_with(|prev| prev.with_param_index(value));
    }

    fn update_source_midi_message_number(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(U7::new(id as _)),
        };
        self.mapping.source_model.midi_message_number.set(value);
    }

    fn update_source_character(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        self.mapping.source_model.custom_character.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid source character"),
        );
    }

    fn update_source_type(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        self.mapping.source_model.r#type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid source type"),
        );
    }

    fn update_source_midi_clock_transport_message_type(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX);
        self.mapping.source_model.midi_clock_transport_message.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid MTC message type"),
        );
    }

    fn update_source_parameter_number_message_number(&mut self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        let value = c.text().ok().and_then(|t| t.parse::<U14>().ok());
        self.mapping
            .source_model
            .parameter_number_message_number
            .set(value);
    }

    fn update_mode_rotate(&mut self) {
        self.mapping.mode_model.rotate.set(
            self.view
                .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_ignore_out_of_range_values(&mut self) {
        self.mapping
            .mode_model
            .ignore_out_of_range_source_values
            .set(
                self.view
                    .require_control(root::ID_SETTINGS_IGNORE_OUT_OF_RANGE_CHECK_BOX)
                    .is_checked(),
            );
    }

    fn update_mode_round_target_value(&mut self) {
        self.mapping.mode_model.round_target_value.set(
            self.view
                .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_approach(&mut self) {
        self.mapping.mode_model.approach_target_value.set(
            self.view
                .require_control(root::ID_SETTINGS_SCALE_MODE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_reverse(&mut self) {
        self.mapping.mode_model.reverse.set(
            self.view
                .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn reset_mode(&mut self) {
        self.mapping.reset_mode(self.session.context());
    }

    fn update_mode_type(&mut self) {
        let b = self.view.require_control(root::ID_SETTINGS_MODE_COMBO_BOX);
        self.mapping.mode_model.r#type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid mode type"),
        );
        self.mapping
            .set_preferred_mode_values(self.session.context());
    }

    fn update_mode_min_target_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn get_value_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.real_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        target.parse_as_value(text.as_str()).ok()
    }

    fn get_step_size_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.real_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        target.parse_as_step_size(text.as_str()).ok()
    }

    fn update_mode_max_target_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_min_jump_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn update_mode_max_jump_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_min_source_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_source_edit_control(root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn get_value_from_source_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let text = self.view.require_control(edit_control_id).text().ok()?;
        self.mapping
            .source_model
            .parse_control_value(text.as_str())
            .ok()
    }

    fn update_mode_max_source_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_source_edit_control(root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_min_step_or_duration_from_edit_control(&mut self) {
        if self.mapping.mode_model.supports_press_duration() {
            let value = self
                .get_value_from_duration_edit_control(root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL)
                .unwrap_or_else(|| Duration::from_millis(0));
            self.mapping
                .mode_model
                .press_duration_interval
                .set_with(|prev| prev.with_min(value));
        } else {
            let value = self
                .get_value_from_step_edit_control(root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL)
                .unwrap_or_else(|| UnitValue::MIN.to_symmetric());
            self.mapping
                .mode_model
                .step_interval
                .set_with(|prev| prev.with_min(value));
        }
    }

    fn get_value_from_duration_edit_control(&self, edit_control_id: u32) -> Option<Duration> {
        let text = self.view.require_control(edit_control_id).text().ok()?;
        text.parse::<u64>().ok().map(Duration::from_millis)
    }

    fn get_value_from_step_edit_control(&self, edit_control_id: u32) -> Option<SymmetricUnitValue> {
        if self.mapping_uses_step_counts() {
            let text = self.view.require_control(edit_control_id).text().ok()?;
            convert_factor_to_unit_value(text.parse().ok()?).ok()
        } else {
            self.get_step_size_from_target_edit_control(edit_control_id)
                .map(|v| v.to_symmetric())
        }
    }

    fn update_mode_max_step_or_duration_from_edit_control(&mut self) {
        if self.mapping.mode_model.supports_press_duration() {
            let value = self
                .get_value_from_duration_edit_control(root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL)
                .unwrap_or_else(|| Duration::from_millis(0));
            self.mapping
                .mode_model
                .press_duration_interval
                .set_with(|prev| prev.with_max(value));
        } else {
            let value = self
                .get_value_from_step_edit_control(root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL)
                .unwrap_or(SymmetricUnitValue::MAX);
            self.mapping
                .mode_model
                .step_interval
                .set_with(|prev| prev.with_max(value));
        }
    }

    fn update_mode_eel_control_transformation(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping
            .mode_model
            .eel_control_transformation
            .set(value);
    }

    fn update_mode_eel_feedback_transformation(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping
            .mode_model
            .eel_feedback_transformation
            .set(value);
    }

    fn update_mode_min_target_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_target_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn update_mode_min_source_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_source_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn update_mode_min_step_or_duration_from_slider(&mut self, slider: Window) {
        if self.mapping.mode_model.supports_press_duration() {
            self.mapping
                .mode_model
                .press_duration_interval
                .set_with(|prev| prev.with_min(slider.slider_duration()));
        } else {
            let step_counts = self.mapping_uses_step_counts();
            let prop = &mut self.mapping.mode_model.step_interval;
            if step_counts {
                prop.set_with(|prev| prev.with_min(slider.slider_symmetric_unit_value()));
            } else {
                prop.set_with(|prev| prev.with_min(slider.slider_unit_value().to_symmetric()));
            }
        }
    }

    fn update_mode_max_step_or_duration_from_slider(&mut self, slider: Window) {
        if self.mapping.mode_model.supports_press_duration() {
            self.mapping
                .mode_model
                .press_duration_interval
                .set_with(|prev| prev.with_max(slider.slider_duration()));
        } else {
            let step_counts = self.mapping_uses_step_counts();
            let prop = &mut self.mapping.mode_model.step_interval;
            if step_counts {
                prop.set_with(|prev| prev.with_max(slider.slider_symmetric_unit_value()));
            } else {
                prop.set_with(|prev| prev.with_max(slider.slider_unit_value().to_symmetric()));
            }
        }
    }

    fn mapping_uses_step_counts(&self) -> bool {
        self.mapping
            .with_context(self.session.context())
            .uses_step_counts()
    }

    fn update_mode_min_jump_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_jump_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn update_target_is_input_fx(&mut self) {
        self.mapping.target_model.is_input_fx.set(
            self.view
                .require_control(root::ID_TARGET_INPUT_FX_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_target_only_if_fx_has_focus(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_FX_FOCUS_CHECK_BOX)
            .is_checked();
        let target = &mut self.mapping.target_model;
        if target.supports_fx() {
            target.enable_only_if_fx_has_focus.set(is_checked);
        } else if target.r#type.get() == TargetType::TrackSelection {
            target.select_exclusively.set(is_checked);
        }
    }

    fn update_target_only_if_track_is_selected(&mut self) {
        self.mapping.target_model.enable_only_if_track_selected.set(
            self.view
                .require_control(root::ID_TARGET_TRACK_SELECTED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn toggle_learn_target(&mut self) {
        self.session.toggle_learn_target(self.shared_mapping);
    }

    fn update_target_type(&mut self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        self.mapping.target_model.r#type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid target type"),
        );
    }

    fn update_target_track(&mut self) -> Result<(), &'static str> {
        let data = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX)
            .selected_combo_box_item_data();
        if self.mapping.target_model.supports_track() {
            use VirtualTrack::*;
            let project = self.target_with_context().project();
            let track = match data {
                -3 => This,
                -2 => Selected,
                -1 => Master,
                _ => Particular(
                    project
                        .track_by_index(data as u32)
                        .ok_or("track not existing")?,
                ),
            };
            self.mapping.target_model.track.set(track);
        }
        Ok(())
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.context())
    }

    fn update_target_from_combo_box_three(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_COMBO_BOX);
        let target = &mut self.mapping.target_model;
        if target.supports_fx() {
            let data = combo.selected_combo_box_item_data();
            let fx_index = if data == -1 { None } else { Some(data as u32) };
            target.set_fx_index_and_memorize_guid(self.session.context(), fx_index);
        } else if target.supports_send() {
            let data = combo.selected_combo_box_item_data();
            let send_index = if data == -1 { None } else { Some(data as u32) };
            target.send_index.set(send_index);
        } else if target.r#type.get() == TargetType::Action {
            let index = combo.selected_combo_box_item_index();
            target
                .action_invocation_type
                .set(index.try_into().expect("invalid action invocation type"));
        }
    }

    fn update_target_fx_parameter(&mut self) {
        let data = self
            .view
            .require_control(root::ID_TARGET_FX_PARAMETER_COMBO_BOX)
            .selected_combo_box_item_data();
        let target = &mut self.mapping.target_model;
        target.param_index.set(data as _);
    }
}

impl<'a> ImmutableMappingPanel<'a> {
    fn fill_all_controls(&self) {
        self.fill_mapping_activation_type_combo_box();
        self.fill_source_type_combo_box();
        self.fill_source_channel_combo_box();
        self.fill_source_midi_message_number_combo_box();
        self.fill_source_character_combo_box();
        self.fill_source_midi_clock_transport_message_type_combo_box();
        self.fill_settings_mode_combo_box();
        self.fill_target_type_combo_box();
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_window_title();
        self.invalidate_mapping_name_edit_control();
        self.invalidate_mapping_control_enabled_check_box();
        self.invalidate_mapping_feedback_enabled_check_box();
        self.invalidate_mapping_prevent_echo_feedback_check_box();
        self.invalidate_mapping_activation_controls();
        self.invalidate_source_controls();
        self.invalidate_target_controls();
        self.invalidate_mode_controls();
    }

    fn invalidate_window_title(&self) {
        self.view
            .require_window()
            .set_text(format!("Edit mapping {}", self.mapping.name.get_ref()));
    }

    fn invalidate_mapping_name_edit_control(&self) {
        let c = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL);
        c.set_text_if_not_focused(self.mapping.name.get_ref().as_str());
    }

    fn invalidate_mapping_activation_eel_condition_edit_control(&self) {
        let c = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL);
        c.set_text_if_not_focused(self.mapping.eel_condition.get_ref().as_str());
    }

    fn invalidate_mapping_control_enabled_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX);
        cb.set_checked(self.mapping.control_is_enabled.get());
        cb.set_text(format!("{} Control enabled", symbols::ARROW_RIGHT_SYMBOL));
    }

    fn invalidate_mapping_feedback_enabled_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX);
        cb.set_checked(self.mapping.feedback_is_enabled.get());
        cb.set_text(format!("{} Feedback enabled", symbols::ARROW_LEFT_SYMBOL));
    }

    fn invalidate_mapping_prevent_echo_feedback_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX);
        cb.set_checked(self.mapping.prevent_echo_feedback.get());
    }

    fn invalidate_mapping_activation_controls(&self) {
        self.invalidate_mapping_activation_control_appearance();
        self.invalidate_mapping_activation_type_combo_box();
        self.invalidate_mapping_activation_setting_1_controls();
        self.invalidate_mapping_activation_setting_2_controls();
        self.invalidate_mapping_activation_eel_condition_edit_control();
    }

    fn invalidate_mapping_activation_control_appearance(&self) {
        self.invalidate_mapping_activation_control_labels();
        self.fill_mapping_activation_combo_boxes();
        self.invalidate_mapping_activation_control_visibilities();
    }

    fn invalidate_mapping_activation_control_labels(&self) {
        use ActivationType::*;
        let label = match self.mapping.activation_type.get() {
            Always => None,
            Modifiers => Some(("Modifier A", "Modifier B")),
            Program => Some(("Bank", "Program")),
            Eel => None,
        };
        if let Some((first, second)) = label {
            self.view
                .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_LABEL_TEXT)
                .set_text(first);
            self.view
                .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_LABEL_TEXT)
                .set_text(second);
        }
    }

    fn invalidate_mapping_activation_control_visibilities(&self) {
        let activation_type = self.mapping.activation_type.get();
        self.show_if(
            activation_type == ActivationType::Modifiers
                || activation_type == ActivationType::Program,
            &[
                root::ID_MAPPING_ACTIVATION_SETTING_1_LABEL_TEXT,
                root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                root::ID_MAPPING_ACTIVATION_SETTING_2_LABEL_TEXT,
                root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
            ],
        );
        self.show_if(
            activation_type == ActivationType::Modifiers,
            &[
                root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX,
                root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX,
            ],
        );
        self.show_if(
            activation_type == ActivationType::Eel,
            &[
                root::ID_MAPPING_ACTIVATION_EEL_LABEL_TEXT,
                root::ID_MAPPING_ACTIVATION_EDIT_CONTROL,
            ],
        );
    }

    fn invalidate_mapping_activation_type_combo_box(&self) {
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX)
            .select_combo_box_item(self.mapping.activation_type.get().into());
    }

    fn fill_mapping_activation_combo_boxes(&self) {
        use ActivationType::*;
        match self.mapping.activation_type.get() {
            Modifiers => {
                self.fill_combo_box_with_realearn_params(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    true,
                );
                self.fill_combo_box_with_realearn_params(
                    root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
                    true,
                );
            }
            Program => {
                self.fill_combo_box_with_realearn_params(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    false,
                );
                self.view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX)
                    .fill_combo_box_with_data_vec(
                        (1..=100).map(|i| (i as isize, i.to_string())).collect(),
                    )
            }
            _ => {}
        };
    }

    fn invalidate_mapping_activation_setting_1_controls(&self) {
        use ActivationType::*;
        match self.mapping.activation_type.get() {
            Modifiers => {
                self.invalidate_mapping_activation_modifier_controls(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX,
                    self.mapping.modifier_condition_1.get(),
                );
            }
            Program => {
                let param_index = self.mapping.program_condition.get().param_index();
                self.view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX)
                    .select_combo_box_item(param_index as _);
            }
            _ => {}
        };
    }

    fn invalidate_mapping_activation_setting_2_controls(&self) {
        use ActivationType::*;
        match self.mapping.activation_type.get() {
            Modifiers => {
                self.invalidate_mapping_activation_modifier_controls(
                    root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
                    root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX,
                    self.mapping.modifier_condition_2.get(),
                );
            }
            Program => {
                let program_index = self.mapping.program_condition.get().program_index();
                self.view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX)
                    .select_combo_box_item(program_index as _);
            }
            _ => {}
        };
    }

    fn invalidate_mapping_activation_modifier_controls(
        &self,
        combo_box_id: u32,
        check_box_id: u32,
        modifier_condition: ModifierConditionModel,
    ) {
        let b = self.view.require_control(combo_box_id);
        match modifier_condition.param_index() {
            None => {
                b.select_combo_box_item_by_data(-1).unwrap();
            }
            Some(i) => {
                b.select_combo_box_item_by_data(i as _).unwrap();
            }
        };
        self.view
            .require_control(check_box_id)
            .set_checked(modifier_condition.is_on());
    }

    fn invalidate_source_controls(&self) {
        self.invalidate_source_control_appearance();
        self.invalidate_source_type_combo_box();
        self.invalidate_source_learn_button();
        self.invalidate_source_channel_combo_box();
        self.invalidate_source_14_bit_check_box();
        self.invalidate_source_is_registered_check_box();
        self.invalidate_source_midi_message_number_controls();
        self.invalidate_source_parameter_number_message_number_controls();
        self.invalidate_source_character_combo_box();
        self.invalidate_source_midi_clock_transport_message_type_combo_box();
    }

    fn invalidate_source_control_appearance(&self) {
        self.invalidate_source_control_labels();
        self.invalidate_source_control_visibilities();
    }

    fn invalidate_source_control_labels(&self) {
        self.view
            .require_control(root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT)
            .set_text(self.source.r#type.get().number_label())
    }

    fn invalidate_source_control_visibilities(&self) {
        let source = self.source;
        self.show_if(
            source.supports_channel(),
            &[
                root::ID_SOURCE_CHANNEL_COMBO_BOX,
                root::ID_SOURCE_CHANNEL_LABEL,
            ],
        );
        self.show_if(
            source.supports_midi_message_number(),
            &[root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT],
        );
        self.show_if(
            source.supports_is_registered(),
            &[root::ID_SOURCE_RPN_CHECK_BOX],
        );
        self.show_if(
            source.supports_14_bit(),
            &[root::ID_SOURCE_14_BIT_CHECK_BOX],
        );
        self.show_if(
            source.supports_midi_clock_transport_message_type(),
            &[
                root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX,
                root::ID_SOURCE_MIDI_MESSAGE_TYPE_LABEL_TEXT,
            ],
        );
        self.show_if(
            source.supports_custom_character(),
            &[
                root::ID_SOURCE_CHARACTER_COMBO_BOX,
                root::ID_SOURCE_CHARACTER_LABEL_TEXT,
            ],
        );
        self.show_if(
            source.supports_parameter_number_message_number(),
            &[root::ID_SOURCE_NUMBER_EDIT_CONTROL],
        );
        self.show_if(
            source.supports_midi_message_number(),
            &[root::ID_SOURCE_NUMBER_COMBO_BOX],
        );
    }

    fn show_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            self.view.require_control(*id).set_visible(condition);
        }
    }

    fn invalidate_source_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_TYPE_COMBO_BOX)
            .select_combo_box_item(self.source.r#type.get().into());
    }

    fn invalidate_source_learn_button(&self) {
        self.invalidate_learn_button(
            self.session.mapping_is_learning_source(self.mapping_ptr),
            root::ID_SOURCE_LEARN_BUTTON,
        );
    }

    fn invalidate_learn_button(&self, is_learning: bool, control_resource_id: u32) {
        let text = if is_learning {
            "Stop learning"
        } else {
            "Learn"
        };
        self.view
            .require_control(control_resource_id)
            .set_text(text);
    }

    fn invalidate_source_channel_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        match self.source.channel.get() {
            None => {
                b.select_combo_box_item_by_data(-1).unwrap();
            }
            Some(ch) => {
                b.select_combo_box_item_by_data(ch.get() as _).unwrap();
            }
        };
    }

    fn invalidate_source_14_bit_check_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_14_BIT_CHECK_BOX)
            .set_checked(
                self.source
                    .is_14_bit
                    .get()
                    .expect("14-bit == None not yet supported"),
            );
    }

    fn invalidate_source_is_registered_check_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
            .set_checked(
                self.source
                    .is_registered
                    .get()
                    .expect("registered == None not yet supported"),
            );
    }

    fn invalidate_source_midi_message_number_controls(&self) {
        let combo = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        let data = match self.source.midi_message_number.get() {
            None => -1,
            Some(n) => n.get() as _,
        };
        combo.select_combo_box_item_by_data(data).unwrap();
    }

    fn invalidate_source_parameter_number_message_number_controls(&self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        if c.has_focus() {
            return;
        }
        let text = match self.source.parameter_number_message_number.get() {
            None => "".to_string(),
            Some(n) => n.to_string(),
        };
        c.set_text_if_not_focused(text)
    }

    fn invalidate_source_character_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX)
            .select_combo_box_item(self.source.custom_character.get().into());
    }

    fn invalidate_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .select_combo_box_item(self.source.midi_clock_transport_message.get().into());
    }

    fn invalidate_target_controls(&self) {
        self.invalidate_target_type_combo_box();
        self.invalidate_target_line_two();
        self.invalidate_target_line_three();
        self.invalidate_target_only_if_fx_has_focus_check_box();
        self.invalidate_target_only_if_track_is_selected_check_box();
        self.invalidate_target_fx_param_combo_box();
        self.invalidate_target_value_controls();
        self.invalidate_target_learn_button();
    }

    fn invalidate_target_type_combo_box(&self) {
        self.view
            .require_control(root::ID_TARGET_TYPE_COMBO_BOX)
            .select_combo_box_item(self.target.r#type.get().into());
    }

    fn invalidate_target_line_two(&self) {
        let pick_button = self
            .view
            .require_control(root::ID_TARGET_PICK_ACTION_BUTTON);
        let action_label = self.view.require_control(root::ID_TARGET_ACTION_LABEL_TEXT);
        let combo = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_CMD_LABEL_TEXT);
        let target = self.target;
        if target.supports_track() {
            label.show();
            combo.show();
            action_label.hide();
            pick_button.hide();
            label.set_text("Track");
            self.fill_target_track_combo_box(label, combo);
            self.set_target_track_combo_box_value(combo);
        } else if self.target.r#type.get() == TargetType::Action {
            label.show();
            action_label.show();
            pick_button.show();
            combo.hide();
            label.set_text("Action");
            let action_name = self.target.action_name_label().to_string();
            action_label.set_text(action_name);
        } else {
            label.hide();
            combo.hide();
            action_label.hide();
            pick_button.hide();
        }
    }

    fn fill_target_track_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Track");
        let mut v = vec![
            (-3isize, VirtualTrack::This),
            (-2isize, VirtualTrack::Selected),
            (-1isize, VirtualTrack::Master),
        ];
        let project = self.target_with_context().project();
        v.extend(
            project
                .tracks()
                .enumerate()
                .map(|(i, track)| (i as isize, VirtualTrack::Particular(track))),
        );
        combo.fill_combo_box_with_data_vec(v);
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.context())
    }

    fn set_target_track_combo_box_value(&self, combo: Window) {
        use VirtualTrack::*;
        let data: isize = match self.target.track.get_ref() {
            This => -3,
            Selected => -2,
            Master => -1,
            Particular(t) => t.index().map(|i| i as isize).unwrap_or(-1),
        };
        combo.select_combo_box_item_by_data(data).unwrap();
    }

    fn invalidate_target_line_three(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_LABEL_TEXT);
        let input_fx_box = self
            .view
            .require_control(root::ID_TARGET_INPUT_FX_CHECK_BOX);
        let target = self.target;
        if target.supports_fx() {
            combo.show();
            label.show();
            input_fx_box.show();
            self.fill_target_fx_combo_box(label, combo);
            self.set_target_fx_combo_box_value(combo);
        } else if target.supports_send() {
            combo.show();
            label.show();
            input_fx_box.hide();
            self.fill_target_send_combo_box(label, combo);
            self.set_target_send_combo_box_value(combo);
        } else if target.r#type.get() == TargetType::Action {
            combo.show();
            label.show();
            input_fx_box.hide();
            self.fill_target_invocation_type_combo_box(label, combo);
            self.set_target_invocation_type_combo_box_value(combo);
        } else {
            label.hide();
            combo.hide();
            input_fx_box.hide();
        }
    }

    fn fill_target_send_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Send");
        let track = match self.target_with_context().effective_track().ok() {
            None => {
                combo.clear_combo_box();
                return;
            }
            Some(t) => t,
        };
        let sends = track
            .sends()
            .enumerate()
            .map(|(i, send)| (i as isize, send));
        combo.fill_combo_box_with_data_small(sends);
    }

    fn set_target_send_combo_box_value(&self, combo: Window) {
        let target = self.target;
        match target.send_index.get() {
            None => combo.select_new_combo_box_item("<None>"),
            Some(i) => combo
                .select_combo_box_item_by_data(i as isize)
                .unwrap_or_else(|_| {
                    combo.select_new_combo_box_item(format!("{}. <Not present>", i + 1).as_str());
                }),
        }
    }

    fn fill_target_invocation_type_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Invoke");
        combo.fill_combo_box(ActionInvocationType::into_enum_iter());
    }

    fn set_target_invocation_type_combo_box_value(&self, combo: Window) {
        combo.select_combo_box_item(self.target.action_invocation_type.get().into());
    }

    fn fill_target_fx_param_combo_box(&self, combo: Window) {
        let fx = match self.target_with_context().fx().ok() {
            None => {
                combo.clear_combo_box();
                return;
            }
            Some(fx) => fx,
        };
        let params: Vec<_> = fx
            .parameters()
            .map(|param| {
                (
                    param.index() as isize,
                    get_fx_param_label(Some(&param), param.index()),
                )
            })
            .collect();
        // TODO-low Just the index would be enough, don't need data.
        combo.fill_combo_box_with_data_vec(params);
    }

    fn set_target_fx_param_combo_box_value(&self, combo: Window) {
        let target = self.target;
        let param_index = target.param_index.get();
        combo
            .select_combo_box_item_by_data(param_index as isize)
            .unwrap_or_else(|_| {
                combo.select_new_combo_box_item(get_fx_param_label(None, param_index).as_ref());
            });
    }

    fn fill_target_fx_combo_box(&self, label: Window, combo: Window) {
        label.set_text("FX");
        let track = match self.target_with_context().effective_track().ok() {
            None => {
                combo.clear_combo_box();
                return;
            }
            Some(t) => t,
        };
        let fx_chain = if self.target.is_input_fx.get() {
            track.input_fx_chain()
        } else {
            track.normal_fx_chain()
        };
        let fxs = fx_chain
            .fxs()
            .enumerate()
            .map(|(i, fx)| (i as isize, get_fx_label(Some(&fx), Some(i as u32))));
        combo.fill_combo_box_with_data_small(fxs);
    }

    fn set_target_fx_combo_box_value(&self, combo: Window) {
        let target = self.target;
        match target.fx_index.get() {
            None => combo.select_new_combo_box_item("<None>"),
            Some(i) => combo
                .select_combo_box_item_by_data(i as isize)
                .unwrap_or_else(|_| {
                    combo.select_new_combo_box_item(get_fx_label(None, Some(i)).as_ref());
                }),
        }
    }

    fn invalidate_target_only_if_fx_has_focus_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_FX_FOCUS_CHECK_BOX);
        let target = self.target;
        if target.supports_fx() {
            b.show();
            b.set_text("FX must have focus");
            b.set_checked(target.enable_only_if_fx_has_focus.get());
        } else if target.r#type.get() == TargetType::TrackSelection {
            b.show();
            b.set_text("Select exclusively");
            b.set_checked(target.select_exclusively.get());
        } else {
            b.hide();
        }
    }

    fn invalidate_target_only_if_track_is_selected_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_TRACK_SELECTED_CHECK_BOX);
        let target = self.target;
        if target.supports_track() {
            b.show();
            b.set_checked(target.enable_only_if_track_selected.get());
        } else {
            b.hide();
        }
    }

    fn invalidate_target_fx_param_combo_box(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_FX_PARAMETER_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_FX_PARAMETER_LABEL_TEXT);
        let target = self.target;
        if target.r#type.get() == TargetType::FxParameter {
            combo.show();
            label.show();
            self.fill_target_fx_param_combo_box(combo);
            self.set_target_fx_param_combo_box_value(combo);
        } else {
            combo.hide();
            label.hide();
        }
    }

    fn invalidate_target_value_controls(&self) {
        if let Some(t) = self.real_target() {
            self.invalidate_target_controls_internal(
                root::ID_TARGET_VALUE_SLIDER_CONTROL,
                root::ID_TARGET_VALUE_EDIT_CONTROL,
                root::ID_TARGET_VALUE_TEXT,
                t.current_value(),
            )
        }
    }

    fn invalidate_target_learn_button(&self) {
        self.invalidate_learn_button(
            self.session.mapping_is_learning_target(self.mapping_ptr),
            root::ID_TARGET_LEARN_BUTTON,
        );
    }

    fn register_listeners(&self) {
        self.register_session_listeners();
        self.register_mapping_listeners();
        self.register_source_listeners();
        self.register_target_listeners();
        self.register_mode_listeners();
    }

    fn register_session_listeners(&self) {
        self.panel
            .when_do_sync(self.session.mapping_which_learns_source.changed(), |view| {
                view.invalidate_source_learn_button();
            });
        self.panel
            .when_do_sync(self.session.mapping_which_learns_target.changed(), |view| {
                view.invalidate_target_learn_button();
            });
        let reaper = Reaper::get();
        self.panel.when_do_sync(
            reaper
                // Because we want to display new tracks in combo box as soon as they appear.
                .track_added()
                .map_to(())
                // Because we want to display new FX in combo box as soon as they appear.
                .merge(reaper.fx_added().map_to(()))
                // Because we want a changed track name to be reflected immediately in the UI.
                .merge(reaper.track_name_changed().map_to(()))
                // Because we want to see any possible effective `ReaperTarget` change immediately.
                .merge(TargetModel::potential_static_change_events())
                .merge(TargetModel::potential_dynamic_change_events()),
            |view| {
                // TODO-medium The C++ code yields here (when FX changed):
                //  Yield. Because the model might also listen to such events and we want the model
                // to digest it *before* the  UI. It happened that this UI handler
                // is called *before* the model handler in some cases. Then it is super
                //  important - otherwise crash.
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
    }

    fn register_mapping_listeners(&self) {
        self.panel
            .when_do_sync(self.mapping.name.changed(), |view| {
                view.invalidate_window_title();
                view.invalidate_mapping_name_edit_control();
            });
        self.panel
            .when_do_sync(self.mapping.control_is_enabled.changed(), |view| {
                view.invalidate_mapping_control_enabled_check_box();
            });
        self.panel
            .when_do_sync(self.mapping.feedback_is_enabled.changed(), |view| {
                view.invalidate_mapping_feedback_enabled_check_box();
            });
        self.panel
            .when_do_sync(self.mapping.prevent_echo_feedback.changed(), |view| {
                view.invalidate_mapping_prevent_echo_feedback_check_box();
            });
        self.panel
            .when_do_sync(self.mapping.activation_type.changed(), |view| {
                view.invalidate_mapping_activation_controls();
            });
        self.panel
            .when_do_sync(self.mapping.modifier_condition_1.changed(), |view| {
                view.invalidate_mapping_activation_setting_1_controls();
            });
        self.panel
            .when_do_sync(self.mapping.modifier_condition_2.changed(), |view| {
                view.invalidate_mapping_activation_setting_2_controls();
            });
        self.panel
            .when_do_sync(self.mapping.program_condition.changed(), |view| {
                view.invalidate_mapping_activation_setting_1_controls();
                view.invalidate_mapping_activation_setting_2_controls();
            });
        self.panel
            .when_do_sync(self.mapping.eel_condition.changed(), |view| {
                view.invalidate_mapping_activation_eel_condition_edit_control();
            });
    }

    fn register_source_listeners(&self) {
        let source = self.source;
        self.panel.when_do_sync(source.r#type.changed(), |view| {
            view.invalidate_source_type_combo_box();
            view.invalidate_source_control_appearance();
            view.invalidate_mode_controls();
        });
        self.panel.when_do_sync(source.channel.changed(), |view| {
            view.invalidate_source_channel_combo_box();
        });
        self.panel.when_do_sync(source.is_14_bit.changed(), |view| {
            view.invalidate_source_14_bit_check_box();
            view.invalidate_mode_controls();
            view.invalidate_source_control_appearance();
        });
        self.panel
            .when_do_sync(source.midi_message_number.changed(), |view| {
                view.invalidate_source_midi_message_number_controls();
            });
        self.panel
            .when_do_sync(source.parameter_number_message_number.changed(), |view| {
                view.invalidate_source_parameter_number_message_number_controls();
            });
        self.panel
            .when_do_sync(source.is_registered.changed(), |view| {
                view.invalidate_source_is_registered_check_box();
            });
        self.panel
            .when_do_sync(source.custom_character.changed(), |view| {
                view.invalidate_source_character_combo_box();
            });
        self.panel
            .when_do_sync(source.midi_clock_transport_message.changed(), |view| {
                view.invalidate_source_midi_clock_transport_message_type_combo_box();
            });
    }

    fn invalidate_mode_controls(&self) {
        self.invalidate_mode_type_combo_box();
        self.invalidate_mode_control_appearance();
        self.invalidate_mode_source_value_controls();
        self.invalidate_mode_target_value_controls();
        self.invalidate_mode_step_or_duration_controls();
        self.invalidate_mode_rotate_check_box();
        self.invalidate_mode_ignore_out_of_range_check_box();
        self.invalidate_mode_round_target_value_check_box();
        self.invalidate_mode_approach_check_box();
        self.invalidate_mode_reverse_check_box();
        self.invalidate_mode_eel_control_transformation_edit_control();
        self.invalidate_mode_eel_feedback_transformation_edit_control();
    }

    fn invalidate_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .select_combo_box_item(self.mode.r#type.get().into());
    }

    fn invalidate_mode_control_appearance(&self) {
        self.invalidate_mode_control_labels();
        self.invalidate_mode_control_visibilities();
    }

    fn mapping_uses_step_counts(&self) -> bool {
        self.mapping
            .with_context(self.session.context())
            .uses_step_counts()
    }

    fn invalidate_mode_control_labels(&self) {
        let step_label = if self.mode.supports_press_duration() {
            "Length"
        } else if self.mapping_uses_step_counts() {
            "Speed"
        } else {
            "Step size"
        };
        self.view
            .require_control(root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT)
            .set_text(step_label);
    }

    fn invalidate_mode_control_visibilities(&self) {
        let mode = self.mode;
        self.show_if(
            mode.supports_round_target_value()
                && self.target_with_context().is_known_to_be_roundable(),
            &[root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_reverse(),
            &[root::ID_SETTINGS_REVERSE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_approach_target_value(),
            &[root::ID_SETTINGS_SCALE_MODE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_rotate(),
            &[root::ID_SETTINGS_ROTATE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_ignore_out_of_range_source_values(),
            &[root::ID_SETTINGS_IGNORE_OUT_OF_RANGE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_steps() || mode.supports_press_duration(),
            &[
                root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
                root::ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
            ],
        );
        self.show_if(
            mode.supports_jump(),
            &[
                root::ID_SETTINGS_TARGET_JUMP_LABEL_TEXT,
                root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
                root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
                root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
                root::ID_SETTINGS_MIN_TARGET_JUMP_LABEL_TEXT,
                root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
                root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
                root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
                root::ID_SETTINGS_MAX_TARGET_JUMP_LABEL_TEXT,
            ],
        );
        self.show_if(
            mode.supports_eel_control_transformation(),
            &[
                root::ID_MODE_EEL_CONTROL_TRANSFORMATION_LABEL,
                root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL,
            ],
        );
        self.show_if(
            mode.supports_eel_feedback_transformation(),
            &[
                root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_LABEL,
                root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL,
            ],
        );
    }

    fn invalidate_mode_source_value_controls(&self) {
        self.invalidate_mode_min_source_value_controls();
        self.invalidate_mode_max_source_value_controls();
    }

    fn invalidate_mode_target_value_controls(&self) {
        self.invalidate_mode_min_target_value_controls();
        self.invalidate_mode_max_target_value_controls();
        self.invalidate_mode_min_jump_controls();
        self.invalidate_mode_max_jump_controls();
    }

    fn invalidate_mode_min_source_value_controls(&self) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_source_value_controls(&self) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval.get_ref().max_val(),
        );
    }

    fn invalidate_mode_source_value_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value: UnitValue,
    ) {
        let formatted_value = self
            .source
            .format_control_value(ControlValue::Absolute(value))
            .unwrap_or_else(|_| "".to_string());
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(formatted_value);
        self.view
            .require_control(slider_control_id)
            .set_slider_unit_value(value);
    }

    fn invalidate_mode_min_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_TEXT,
            self.mode.target_value_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
            self.mode.target_value_interval.get_ref().max_val(),
        );
    }

    fn invalidate_target_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: UnitValue,
    ) {
        let (edit_text, value_text) = match &self.real_target() {
            Some(target) => {
                let edit_text = if target.character() == TargetCharacter::Discrete {
                    target
                        .convert_unit_value_to_discrete_value(value)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| "".to_string())
                } else {
                    target.format_value_without_unit(value)
                };
                let value_text = self.get_text_right_to_target_edit_control(&target, value);
                (edit_text, value_text)
            }
            None => ("".to_string(), "".to_string()),
        };
        self.view
            .require_control(slider_control_id)
            .set_slider_unit_value(value);
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(edit_text);
        self.view
            .require_control(value_text_control_id)
            .set_text(value_text);
    }

    fn get_text_right_to_step_size_edit_control(
        &self,
        t: &ReaperTarget,
        step_size: UnitValue,
    ) -> String {
        if t.hide_formatted_step_size() {
            t.step_size_unit().to_string()
        } else {
            format!(
                "{}  {}",
                t.step_size_unit(),
                t.format_step_size_without_unit(step_size)
            )
        }
    }

    fn get_text_right_to_target_edit_control(&self, t: &ReaperTarget, value: UnitValue) -> String {
        if t.hide_formatted_value() {
            t.value_unit().to_string()
        } else if t.character() == TargetCharacter::Discrete {
            // Please note that discrete FX parameters can only show their *current* value,
            // unless they implement the REAPER VST extension functions.
            t.format_value(value)
        } else {
            format!("{}  {}", t.value_unit(), t.format_value(value))
        }
    }

    fn invalidate_mode_min_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
            self.mode.jump_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
            self.mode.jump_interval.get_ref().max_val(),
        );
    }

    fn invalidate_mode_step_or_duration_controls(&self) {
        self.invalidate_mode_min_step_or_duration_controls();
        self.invalidate_mode_max_step_or_duration_controls();
    }

    fn invalidate_mode_min_step_or_duration_controls(&self) {
        if self.mode.supports_press_duration() {
            self.invalidate_mode_press_duration_controls_internal(
                root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
                self.mode.press_duration_interval.get_ref().min_val(),
            );
        } else if self.mode.supports_steps() {
            self.invalidate_mode_step_controls_internal(
                root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
                self.mode.step_interval.get_ref().min_val(),
            );
        }
    }

    fn invalidate_mode_max_step_or_duration_controls(&self) {
        if self.mode.supports_press_duration() {
            self.invalidate_mode_press_duration_controls_internal(
                root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
                self.mode.press_duration_interval.get_ref().max_val(),
            );
        } else if self.mode.supports_steps() {
            self.invalidate_mode_step_controls_internal(
                root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
                self.mode.step_interval.get_ref().max_val(),
            );
        }
    }

    fn invalidate_mode_step_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: SymmetricUnitValue,
    ) {
        let (val, edit_text, value_text) = match &self.real_target() {
            Some(target) => {
                if self.mapping_uses_step_counts() {
                    let edit_text = convert_unit_value_to_factor(value).to_string();
                    let val = PositiveOrSymmetricUnitValue::Symmetric(value);
                    // "count {x}"
                    (val, edit_text, "x".to_string())
                } else {
                    // "{size} {unit}"
                    let pos_value = value.clamp_to_positive_unit_interval();
                    let edit_text = target.format_step_size_without_unit(pos_value);
                    let value_text =
                        self.get_text_right_to_step_size_edit_control(target, pos_value);
                    (
                        PositiveOrSymmetricUnitValue::Positive(pos_value),
                        edit_text,
                        value_text,
                    )
                }
            }
            None => (
                PositiveOrSymmetricUnitValue::Positive(UnitValue::MIN),
                "".to_string(),
                "".to_string(),
            ),
        };
        match val {
            PositiveOrSymmetricUnitValue::Positive(v) => {
                self.view
                    .require_control(slider_control_id)
                    .set_slider_unit_value(v);
            }
            PositiveOrSymmetricUnitValue::Symmetric(v) => {
                self.view
                    .require_control(slider_control_id)
                    .set_slider_symmetric_unit_value(v);
            }
        }
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(edit_text);
        self.view
            .require_control(value_text_control_id)
            .set_text(value_text)
    }

    fn invalidate_mode_press_duration_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        duration: Duration,
    ) {
        self.view
            .require_control(slider_control_id)
            .set_slider_duration(duration);
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(duration.as_millis().to_string());
        self.view
            .require_control(value_text_control_id)
            .set_text("ms")
    }

    fn invalidate_mode_rotate_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
            .set_checked(self.mode.rotate.get());
    }

    fn invalidate_mode_ignore_out_of_range_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_IGNORE_OUT_OF_RANGE_CHECK_BOX)
            .set_checked(self.mode.ignore_out_of_range_source_values.get());
    }

    fn invalidate_mode_round_target_value_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
            .set_checked(self.mode.round_target_value.get());
    }

    fn invalidate_mode_approach_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_SCALE_MODE_CHECK_BOX)
            .set_checked(self.mode.approach_target_value.get());
    }

    fn invalidate_mode_reverse_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
            .set_checked(self.mode.reverse.get());
    }

    fn invalidate_mode_eel_control_transformation_edit_control(&self) {
        self.view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .set_text_if_not_focused(self.mode.eel_control_transformation.get_ref().as_str());
    }

    fn invalidate_mode_eel_feedback_transformation_edit_control(&self) {
        self.view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .set_text_if_not_focused(self.mode.eel_feedback_transformation.get_ref().as_str());
    }

    fn register_target_listeners(&self) {
        let target = self.target;
        // Display target value changes in real-time!
        self.panel.when_do_async(
            // We want to subscribe to target value changes when subscribed for the first time ...
            observable::of(())
                // ... and resubscribe whenever the target model changes
                .merge(target.changed())
                // ... and some other events occur that might change the target "value producer"
                // (e.g. volume of track 2) in some way.
                .merge(TargetModel::potential_static_change_events())
                .merge(TargetModel::potential_dynamic_change_events()),
            |view| {
                // Okay. Time to resubscribe.
                let mut existing_subscription =
                    view.panel.target_value_change_subscription.borrow_mut();
                // Resubscribe if information in model is enough to create actual target.
                if let Ok(t) = view.target_with_context().create_target() {
                    let new_subscription =
                        view.panel.when_do_async(t.value_changed(), |inner_view| {
                            inner_view.invalidate_target_value_controls();
                        });
                    *existing_subscription =
                        SubscriptionGuard::new(Box::new(new_subscription.into_inner()));
                };
            },
        );
        self.panel.when_do_sync(target.r#type.changed(), |view| {
            view.invalidate_target_controls();
            view.invalidate_mode_controls();
        });
        self.panel.when_do_sync(target.track.changed(), |view| {
            view.invalidate_target_controls();
            view.invalidate_mode_controls();
        });
        self.panel.when_do_sync(
            target
                .fx_index
                .changed()
                .merge(target.is_input_fx.changed()),
            |view| {
                view.invalidate_target_line_three();
                view.invalidate_target_fx_param_combo_box();
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel
            .when_do_sync(target.param_index.changed(), |view| {
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            });
        self.panel.when_do_sync(target.action.changed(), |view| {
            view.invalidate_target_line_two();
        });
        self.panel
            .when_do_sync(target.action_invocation_type.changed(), |view| {
                view.invalidate_target_line_three();
                view.invalidate_mode_controls();
            });
    }

    fn register_mode_listeners(&self) {
        let mode = self.mode;
        self.panel.when_do_sync(mode.r#type.changed(), |view| {
            view.invalidate_mode_controls();
        });
        self.panel
            .when_do_sync(mode.target_value_interval.changed(), |view| {
                view.invalidate_mode_min_target_value_controls();
                view.invalidate_mode_max_target_value_controls();
            });
        self.panel
            .when_do_sync(mode.source_value_interval.changed(), |view| {
                view.invalidate_mode_source_value_controls();
            });
        self.panel
            .when_do_sync(mode.jump_interval.changed(), |view| {
                view.invalidate_mode_min_jump_controls();
                view.invalidate_mode_max_jump_controls();
            });
        self.panel
            .when_do_sync(mode.step_interval.changed(), |view| {
                view.invalidate_mode_step_or_duration_controls();
            });
        self.panel
            .when_do_sync(mode.press_duration_interval.changed(), |view| {
                view.invalidate_mode_step_or_duration_controls();
            });
        self.panel
            .when_do_sync(mode.ignore_out_of_range_source_values.changed(), |view| {
                view.invalidate_mode_ignore_out_of_range_check_box();
            });
        self.panel
            .when_do_sync(mode.round_target_value.changed(), |view| {
                view.invalidate_mode_round_target_value_check_box();
            });
        self.panel
            .when_do_sync(mode.approach_target_value.changed(), |view| {
                view.invalidate_mode_approach_check_box();
            });
        self.panel.when_do_sync(mode.rotate.changed(), |view| {
            view.invalidate_mode_rotate_check_box();
        });
        self.panel.when_do_sync(mode.reverse.changed(), |view| {
            view.invalidate_mode_reverse_check_box();
        });
        self.panel
            .when_do_sync(mode.eel_control_transformation.changed(), |view| {
                view.invalidate_mode_eel_control_transformation_edit_control();
            });
        self.panel
            .when_do_sync(mode.eel_feedback_transformation.changed(), |view| {
                view.invalidate_mode_eel_feedback_transformation_edit_control();
            });
    }

    fn fill_mapping_activation_type_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX);
        b.fill_combo_box(ActivationType::into_enum_iter());
    }

    fn fill_combo_box_with_realearn_params(&self, control_id: u32, with_none: bool) {
        let b = self.view.require_control(control_id);
        let start = if with_none {
            vec![(-1isize, "<None>".to_string())]
        } else {
            vec![]
        };
        b.fill_combo_box_with_data_small(start.into_iter().chain((0..PLUGIN_PARAMETER_COUNT).map(
            |i| {
                (
                    i as isize,
                    format!("{}. {}", i + 1, self.session.get_parameter_name(i)),
                )
            },
        )));
    }

    fn fill_source_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        b.fill_combo_box(MidiSourceType::into_enum_iter());
    }

    fn fill_source_channel_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        b.fill_combo_box_with_data_small(
            iter::once((-1isize, "<Any> (no feedback)".to_string()))
                .chain((0..16).map(|i| (i as isize, (i + 1).to_string()))),
        )
    }

    fn fill_source_midi_message_number_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        b.fill_combo_box_with_data_vec(
            iter::once((-1isize, "<Any> (no feedback)".to_string()))
                .chain((0..128).map(|i| (i as isize, i.to_string())))
                .collect(),
        )
    }

    fn fill_source_character_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        b.fill_combo_box(SourceCharacter::into_enum_iter());
    }

    fn fill_source_midi_clock_transport_message_type_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX);
        b.fill_combo_box(MidiClockTransportMessage::into_enum_iter());
    }

    fn fill_settings_mode_combo_box(&self) {
        let b = self.view.require_control(root::ID_SETTINGS_MODE_COMBO_BOX);
        b.fill_combo_box(ModeType::into_enum_iter());
    }

    fn fill_target_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        b.fill_combo_box(TargetType::into_enum_iter());
    }

    fn real_target(&self) -> Option<ReaperTarget> {
        self.target_with_context().create_target().ok()
    }
}

impl View for MappingPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.memorize_all_slider_controls();
        true
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        self.hide();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.sliders.replace(None);
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            // Mapping
            ID_MAPPING_CONTROL_ENABLED_CHECK_BOX => {
                self.write(|p| p.update_mapping_control_enabled())
            }
            ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX => {
                self.write(|p| p.update_mapping_feedback_enabled())
            }
            ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX => {
                self.write(|p| p.update_mapping_prevent_echo_feedback())
            }
            ID_MAPPING_FIND_IN_LIST_BUTTON => {
                self.scroll_to_mapping_in_main_panel();
            }
            ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX => {
                self.write(|p| p.update_mapping_activation_setting_1_on())
            }
            ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX => {
                self.write(|p| p.update_mapping_activation_setting_2_on())
            }
            // IDCANCEL is escape button
            ID_OK | raw::IDCANCEL => {
                self.hide();
            }
            // Source
            ID_SOURCE_LEARN_BUTTON => self.write(|p| p.toggle_learn_source()),
            ID_SOURCE_RPN_CHECK_BOX => self.write(|p| p.update_source_is_registered()),
            ID_SOURCE_14_BIT_CHECK_BOX => self.write(|p| p.update_source_is_14_bit()),
            // Mode
            ID_SETTINGS_ROTATE_CHECK_BOX => self.write(|p| p.update_mode_rotate()),
            ID_SETTINGS_IGNORE_OUT_OF_RANGE_CHECK_BOX => {
                self.write(|p| p.update_mode_ignore_out_of_range_values())
            }
            ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX => {
                self.write(|p| p.update_mode_round_target_value())
            }
            ID_SETTINGS_SCALE_MODE_CHECK_BOX => self.write(|p| p.update_mode_approach()),
            ID_SETTINGS_REVERSE_CHECK_BOX => self.write(|p| p.update_mode_reverse()),
            ID_SETTINGS_RESET_BUTTON => self.write(|p| p.reset_mode()),
            // Target
            ID_TARGET_INPUT_FX_CHECK_BOX => self.write(|p| p.update_target_is_input_fx()),
            ID_TARGET_FX_FOCUS_CHECK_BOX => self.write(|p| p.update_target_only_if_fx_has_focus()),
            ID_TARGET_TRACK_SELECTED_CHECK_BOX => {
                self.write(|p| p.update_target_only_if_track_is_selected())
            }
            ID_TARGET_LEARN_BUTTON => self.write(|p| p.toggle_learn_target()),
            ID_TARGET_OPEN_BUTTON => self.write(|p| p.open_target()),
            ID_TARGET_PICK_ACTION_BUTTON => self.write(|p| p.pick_action()),
            _ => unreachable!(),
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            // Mapping
            ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX => {
                self.write(|p| p.update_mapping_activation_type())
            }
            ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX => {
                self.write(|p| p.update_mapping_activation_setting_1_option())
            }
            ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX => {
                self.write(|p| p.update_mapping_activation_setting_2_option())
            }
            // Source
            ID_SOURCE_CHANNEL_COMBO_BOX => self.write(|p| p.update_source_channel()),
            ID_SOURCE_NUMBER_COMBO_BOX => self.write(|p| p.update_source_midi_message_number()),
            ID_SOURCE_CHARACTER_COMBO_BOX => self.write(|p| p.update_source_character()),
            ID_SOURCE_TYPE_COMBO_BOX => self.write(|p| p.update_source_type()),
            ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX => {
                self.write(|p| p.update_source_midi_clock_transport_message_type())
            }
            // Mode
            ID_SETTINGS_MODE_COMBO_BOX => self.write(|p| p.update_mode_type()),
            // Target
            ID_TARGET_TYPE_COMBO_BOX => self.write(|p| p.update_target_type()),
            ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX => {
                self.write(|p| p.update_target_track()).unwrap();
            }
            ID_TARGET_FX_OR_SEND_COMBO_BOX => {
                self.write(|p| p.update_target_from_combo_box_three());
            }
            ID_TARGET_FX_PARAMETER_COMBO_BOX => self.write(|p| p.update_target_fx_parameter()),
            _ => unreachable!(),
        }
    }

    fn slider_moved(self: SharedView<Self>, slider: Window) {
        let cloned_self = self.clone();
        let sliders = cloned_self.sliders.borrow();
        let sliders = sliders.as_ref().expect("sliders not set");
        match slider {
            // Mode
            s if s == sliders.mode_min_target_value => {
                self.write(|p| p.update_mode_min_target_value_from_slider(s));
            }
            s if s == sliders.mode_max_target_value => {
                self.write(|p| p.update_mode_max_target_value_from_slider(s));
            }
            s if s == sliders.mode_min_source_value => {
                self.write(|p| p.update_mode_min_source_value_from_slider(s));
            }
            s if s == sliders.mode_max_source_value => {
                self.write(|p| p.update_mode_max_source_value_from_slider(s));
            }
            s if s == sliders.mode_min_step_size => {
                self.write(|p| p.update_mode_min_step_or_duration_from_slider(s));
            }
            s if s == sliders.mode_max_step_size => {
                self.write(|p| p.update_mode_max_step_or_duration_from_slider(s));
            }
            s if s == sliders.mode_min_jump => {
                self.write(|p| p.update_mode_min_jump_from_slider(s));
            }
            s if s == sliders.mode_max_jump => {
                self.write(|p| p.update_mode_max_jump_from_slider(s));
            }
            s if s == sliders.target_value => {
                if let Ok(Some(t)) = self.read(|p| p.real_target()) {
                    update_target_value(t, s.slider_unit_value());
                }
            }
            _ => unreachable!(),
        };
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
            // Mapping
            ID_MAPPING_NAME_EDIT_CONTROL => {
                self.write(|p| p.update_mapping_name());
            }
            ID_MAPPING_ACTIVATION_EDIT_CONTROL => {
                self.write(|p| p.update_mapping_activation_eel_condition());
            }
            // Source
            ID_SOURCE_NUMBER_EDIT_CONTROL => {
                self.write(|p| p.update_source_parameter_number_message_number());
            }
            // Mode
            ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_target_value_from_edit_control());
            }
            ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_target_value_from_edit_control());
            }
            ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_jump_from_edit_control());
            }
            ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_jump_from_edit_control());
            }
            ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_source_value_from_edit_control());
            }
            ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_source_value_from_edit_control());
            }
            ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_step_or_duration_from_edit_control());
            }
            ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_step_or_duration_from_edit_control());
            }
            ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL => {
                self.write(|p| p.update_mode_eel_control_transformation());
            }
            ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL => {
                self.write(|p| p.update_mode_eel_feedback_transformation());
            }
            // Target
            ID_TARGET_VALUE_EDIT_CONTROL => {
                let (target, value) = self.write(|p| {
                    let value = p
                        .get_value_from_target_edit_control(root::ID_TARGET_VALUE_EDIT_CONTROL)
                        .unwrap_or(UnitValue::MIN);
                    (p.real_target(), value)
                });
                if let Some(t) = target {
                    update_target_value(t, value);
                }
            }
            _ => return false,
        };
        true
    }

    fn edit_control_focus_killed(self: SharedView<Self>, _resource_id: u32) -> bool {
        // This is also called when the window is hidden.
        // The edit control which is currently edited by the user doesn't get invalidated during
        // `edit_control_changed()`, for good reasons. But as soon as the edit control loses
        // focus, we should invalidate it. This is especially important if the user
        // entered an invalid value. Because we are lazy and edit controls are not
        // manipulated very frequently, we just invalidate all controls.
        // If this fails (because the mapping is not filled anymore), it's not a problem.
        let _ = self.read(|p| {
            p.invalidate_all_controls();
        });
        false
    }
}

trait WindowExt {
    fn slider_unit_value(&self) -> UnitValue;
    fn slider_symmetric_unit_value(&self) -> SymmetricUnitValue;
    fn slider_duration(&self) -> Duration;
    fn set_slider_unit_value(&self, value: UnitValue);
    fn set_slider_symmetric_unit_value(&self, value: SymmetricUnitValue);
    fn set_slider_duration(&self, value: Duration);
}

impl WindowExt for Window {
    fn slider_unit_value(&self) -> UnitValue {
        let discrete_value = self.slider_value();
        UnitValue::new(discrete_value as f64 / 100.0)
    }

    fn slider_symmetric_unit_value(&self) -> SymmetricUnitValue {
        self.slider_unit_value().map_to_symmetric_unit_interval()
    }

    fn slider_duration(&self) -> Duration {
        let discrete_value = self.slider_value();
        Duration::from_millis((discrete_value * 50) as _)
    }

    fn set_slider_unit_value(&self, value: UnitValue) {
        // TODO-low Refactor that map_to_interval stuff to be more generic and less boilerplate
        self.set_slider_range(0, 100);
        let val = (value.get() * 100.0).round() as u32;
        self.set_slider_value(val);
    }

    fn set_slider_symmetric_unit_value(&self, value: SymmetricUnitValue) {
        self.set_slider_unit_value(value.map_to_positive_unit_interval());
    }

    fn set_slider_duration(&self, value: Duration) {
        // 0 = 0ms, 1 = 50ms, ..., 100 = 5s
        self.set_slider_range(0, 100);
        let val = (value.as_millis() / 50) as u32;
        self.set_slider_value(val);
    }
}

enum PositiveOrSymmetricUnitValue {
    Positive(UnitValue),
    Symmetric(SymmetricUnitValue),
}

fn update_target_value(target: ReaperTarget, value: UnitValue) {
    // If it doesn't work in some cases, so what.
    let _ = target.control(ControlValue::Absolute(value));
}
