use crate::core::when_async;
use crate::domain::SharedSession;
use crate::domain::{
    get_fx_label, get_fx_param_label, share_mapping, ActionInvocationType, MappingModel,
    MidiControlInput, MidiFeedbackOutput, MidiSourceModel, MidiSourceType, ModeModel, ModeType,
    ReaperTarget, Session, SharedMapping, TargetCharacter, TargetModel, TargetModelWithContext,
    TargetType, VirtualTrack,
};
use crate::infrastructure::common::bindings::root;
use c_str_macro::c_str;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    ControlValue, DiscreteValue, Interval, MidiClockTransportMessage, SourceCharacter, Target,
    UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper, Track};
use reaper_low::{raw, Swell};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId, ReaperString};
use rx_util::{LocalProp, UnitEvent};
use rxrust::prelude::*;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::iter;
use std::ops::Deref;
use std::ptr::null;
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
pub struct MappingPanel {
    view: ViewContext,
    session: SharedSession,
    mapping: RefCell<Option<SharedMapping>>,
    is_in_reaction: Cell<bool>,
    sliders: RefCell<Option<Sliders>>,
    // Fires when a mapping is about to change or the panel is hidden.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
}

// TODO-low Is it enough to have a MutableMappingPanel?
struct ImmutableMappingPanel<'a> {
    session: &'a Session,
    mapping_ptr: *const MappingModel,
    shared_mapping: &'a SharedMapping,
    mapping: &'a MappingModel,
    source: &'a MidiSourceModel,
    mode: &'a ModeModel,
    target: &'a TargetModel,
    view: &'a ViewContext,
    is_in_reaction: &'a Cell<bool>,
    panel: &'a SharedView<MappingPanel>,
}

struct MutableMappingPanel<'a> {
    session: &'a mut Session,
    mapping: &'a mut MappingModel,
    shared_mapping: &'a SharedMapping,
    view: &'a ViewContext,
    panel: &'a SharedView<MappingPanel>,
}

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
    pub fn new(session: SharedSession) -> MappingPanel {
        MappingPanel {
            view: Default::default(),
            session,
            mapping: None.into(),
            is_in_reaction: false.into(),
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

    pub fn hide(&self) {
        self.stop_party();
        self.view.require_window().hide();
        self.mapping.replace(None);
    }

    pub fn show(self: SharedView<Self>, mapping: SharedMapping) {
        self.stop_party();
        self.mapping.replace(Some(mapping));
        self.clone().start_party();
        self.bring_to_foreground();
    }

    pub fn bring_to_foreground(&self) {
        let window = self.view.require_window();
        window.hide();
        window.show();
    }

    /// Unregisters listeners.
    fn stop_party(&self) {
        self.party_is_over_subject.borrow_mut().next(());
    }

    /// Invalidates everything and registers listeners.
    fn start_party(self: SharedView<Self>) {
        self.with_immutable(|p| {
            p.fill_all_controls();
            p.invalidate_all_controls();
            p.register_listeners();
        });
    }

    fn with_immutable<R>(self: SharedView<Self>, op: impl Fn(&ImmutableMappingPanel) -> R) -> R {
        let session = self.session.borrow();
        let shared_mapping = self.mapping.borrow();
        let shared_mapping = shared_mapping.as_ref().expect("mapping not filled");
        let mapping = shared_mapping.borrow();
        let p = ImmutableMappingPanel {
            session: &session,
            mapping_ptr: shared_mapping.as_ptr(),
            shared_mapping: &shared_mapping,
            mapping: &mapping,
            source: &mapping.source_model,
            mode: &mapping.mode_model,
            target: &mapping.target_model,
            view: &self.view,
            is_in_reaction: &self.is_in_reaction,
            panel: &self,
        };
        op(&p)
    }

    fn with_mutable<R>(self: SharedView<Self>, op: impl Fn(&mut MutableMappingPanel) -> R) -> R {
        let mut session = self.session.borrow_mut();
        let mut shared_mapping = self.mapping.borrow_mut();
        let mut shared_mapping = shared_mapping.as_mut().expect("mapping not filled");
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

    fn is_in_reaction(&self) -> bool {
        self.is_in_reaction.get()
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

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(&ImmutableMappingPanel) + 'static + Copy,
    ) {
        when_async(event, self.party_is_over(), self, move |view| {
            let view_mirror = view.clone();
            view_mirror.is_in_reaction.set(true);
            scopeguard::defer! { view_mirror.is_in_reaction.set(false); }
            view.with_immutable(reaction);
        });
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
        // TODO-high Do later, not so important
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

    fn update_mapping_feedback_enabled(&mut self) {
        self.mapping.feedback_is_enabled.set(
            self.view
                .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_name(&mut self) -> Result<(), &'static str> {
        let value = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL)
            .text()?;
        self.mapping.name.set(value);
        Ok(())
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

    fn update_source_channel(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(Channel::new(id as _)),
        };
        self.mapping.source_model.channel.set(value);
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
        if target.character() == TargetCharacter::Discrete {
            target
                .convert_discrete_value_to_unit_value(text.parse().ok()?)
                .ok()
        } else {
            target.parse_unit_value(text.as_str()).ok()
        }
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

    fn update_mode_min_step_size_from_edit_control(&mut self) {
        let value = self
            .get_value_from_step_size_edit_control(root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .step_size_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn get_value_from_step_size_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        if self
            .mapping
            .with_context(self.session.context())
            .target_should_be_hit_with_increments()
        {
            let text = self.view.require_control(edit_control_id).text().ok()?;
            self.real_target()?
                .convert_discrete_value_to_unit_value(text.parse().ok()?)
                .ok()
        } else {
            self.get_value_from_target_edit_control(edit_control_id)
        }
    }

    fn update_mode_max_step_size_from_edit_control(&mut self) {
        let value = self
            .get_value_from_step_size_edit_control(root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .step_size_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_eel_control_transformation(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or("".to_string());
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
            .unwrap_or("".to_string());
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

    fn update_mode_min_step_size_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .step_size_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_step_size_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .step_size_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
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

    fn update_target_value_from_slider(&mut self, slider: Window) {
        if let Some(t) = self.real_target() {
            t.control(ControlValue::Absolute(slider.slider_unit_value()));
        }
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
        let mut target = &mut self.mapping.target_model;
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

    fn update_target_track_or_command(&mut self) -> Result<(), &'static str> {
        let data = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX)
            .selected_combo_box_item_data();
        let mut target = &mut self.mapping.target_model;
        if target.supports_track() {
            use VirtualTrack::*;
            let target_with_context = target.with_context(self.session.context());
            let project = target_with_context.project();
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
            target.track.set(track);
        } else if target.r#type.get() == TargetType::Action {
            // TODO Do as soon as we are sure about the action picker
        }
        Ok(())
    }

    fn update_target_from_combo_box_three(&mut self) -> Result<(), &'static str> {
        let combo = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_COMBO_BOX);
        let mut target = &mut self.mapping.target_model;
        if target.supports_fx() {
            let data = combo.selected_combo_box_item_data();
            let fx_index = if data == -1 { None } else { Some(data as u32) };
            target.fx_index.set(fx_index);
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
        Ok(())
    }

    fn update_target_fx_parameter(&mut self) {
        let data = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_COMBO_BOX)
            .selected_combo_box_item_data();
        let mut target = &mut self.mapping.target_model;
        target.param_index.set(data as _);
    }

    fn update_target_value_from_edit_control(&mut self) {
        // TODO Do later, not so important
    }
}

impl<'a> ImmutableMappingPanel<'a> {
    fn fill_all_controls(&self) {
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

    fn invalidate_mapping_control_enabled_check_box(&self) {
        self.view
            .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
            .set_checked(self.mapping.control_is_enabled.get());
    }

    fn invalidate_mapping_feedback_enabled_check_box(&self) {
        self.view
            .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
            .set_checked(self.mapping.feedback_is_enabled.get());
    }

    fn invalidate_source_controls(&self) {
        self.invalidate_source_control_appearance();
        self.invalidate_source_type_combo_box();
        self.invalidate_learn_source_button();
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

    fn invalidate_learn_source_button(&self) {
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
                b.select_combo_box_item_by_data(-1);
            }
            Some(ch) => {
                b.select_combo_box_item_by_data(ch.get() as _);
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
        combo.select_combo_box_item_by_data(data);
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
        self.invalidate_target_track_or_action_combo_box();
        self.invalidate_target_line_three();
        self.invalidate_target_only_if_fx_has_focus_check_box();
        self.invalidate_target_only_if_track_is_selected_check_box();
        self.invalidate_target_fx_param_combo_box();
        self.invalidate_target_value_controls();
        self.invalidate_learn_target_button();
    }

    fn invalidate_target_type_combo_box(&self) {
        self.view
            .require_control(root::ID_TARGET_TYPE_COMBO_BOX)
            .select_combo_box_item(self.target.r#type.get().into());
    }

    fn invalidate_target_track_or_action_combo_box(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_CMD_LABEL_TEXT);
        let target = self.target;
        if target.supports_track() {
            combo.show();
            label.show();
            self.fill_target_track_combo_box(label, combo);
            self.set_target_track_combo_box_value(combo);
        } else if target.r#type.get() == TargetType::Action {
            combo.show();
            label.show();
            // TODO Later find a good solution for choosing actions, preferably one which doesn't
            //  need filling a combo box with thousands of actions
            combo.clear_combo_box();
        // self.fill_target_action_combo_box();
        // self.set_target_action_combo_box_value();
        } else {
            label.hide();
            combo.hide();
        }
    }

    fn fill_target_track_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Track");
        let mut v = vec![
            (-3isize, VirtualTrack::This),
            (-2isize, VirtualTrack::Selected),
            (-1isize, VirtualTrack::Master),
        ];
        let target = self.target;
        let session = self.session;
        let target_with_context = target.with_context(session.context());
        let project = target_with_context.project();
        v.extend(
            project
                .tracks()
                .enumerate()
                .map(|(i, track)| (i as isize, VirtualTrack::Particular(track))),
        );
        combo.fill_combo_box_with_data_vec(v);
    }

    fn set_target_track_combo_box_value(&self, combo: Window) {
        use VirtualTrack::*;
        let data: isize = match self.target.track.get_ref() {
            This => -3,
            Selected => -2,
            Master => -1,
            Particular(t) => t.index().expect("we know it's not the master track") as _,
        };
        combo.select_combo_box_item_by_data(data);
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
        let target = self.target;
        let session = self.session;
        let target_with_context = target.with_context(session.context());
        let track = match target_with_context.effective_track().ok() {
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
        let target = self.target;
        let session = self.session;
        let target_with_context = target.with_context(session.context());
        let fx = match target_with_context.fx().ok() {
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
        let target = self.target;
        let session = self.session;
        let target_with_context = target.with_context(session.context());
        let track = match target_with_context.effective_track().ok() {
            None => {
                combo.clear_combo_box();
                return;
            }
            Some(t) => t,
        };
        let fx_chain = if target.is_input_fx.get() {
            track.input_fx_chain()
        } else {
            track.normal_fx_chain()
        };
        let fxs = fx_chain
            .fxs()
            .enumerate()
            .map(|(i, fx)| (i as isize, get_fx_label(Some(&fx), Some(i as u32))).to_owned());
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

    fn invalidate_learn_target_button(&self) {
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
            .when(self.session.mapping_which_learns_source.changed(), |view| {
                view.invalidate_learn_source_button();
            });
        self.panel
            .when(self.session.mapping_which_learns_target.changed(), |view| {
                view.invalidate_learn_target_button();
            });
        let reaper = Reaper::get();
        self.panel.when(
            reaper
                .track_added()
                .map_to(())
                .merge(reaper.track_removed().map_to(()))
                .merge(reaper.track_selected_changed().map_to(())),
            |view| {
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            reaper
                .fx_reordered()
                .map_to(())
                .merge(reaper.fx_added().map_to(()))
                .merge(reaper.fx_removed().map_to(())),
            |view| {
                // TODO The C++ code yields here:
                //  Yield. Because the model might also listen to such events and we want the model
                // to digest it *before* the  UI. It happened that this UI handler
                // is called *before* the model handler in some cases. Then it is super
                //  important - otherwise crash.
                view.invalidate_target_controls();
            },
        );
    }

    fn register_mapping_listeners(&self) {
        self.panel.when(self.mapping.name.changed(), |view| {
            view.invalidate_window_title();
            view.invalidate_mapping_name_edit_control();
        });
        self.panel
            .when(self.mapping.control_is_enabled.changed(), |view| {
                view.invalidate_mapping_control_enabled_check_box();
            });
        self.panel
            .when(self.mapping.feedback_is_enabled.changed(), |view| {
                view.invalidate_mapping_feedback_enabled_check_box();
            });
    }

    fn register_source_listeners(&self) {
        let source = self.source;
        self.panel.when(source.r#type.changed(), |view| {
            view.invalidate_source_type_combo_box();
            view.invalidate_source_control_appearance();
            view.invalidate_mode_controls();
        });
        self.panel.when(source.channel.changed(), |view| {
            view.invalidate_source_channel_combo_box();
        });
        self.panel.when(source.is_14_bit.changed(), |view| {
            view.invalidate_source_14_bit_check_box();
            view.invalidate_mode_controls();
            view.invalidate_source_control_appearance();
        });
        self.panel
            .when(source.midi_message_number.changed(), |view| {
                view.invalidate_source_midi_message_number_controls();
            });
        self.panel
            .when(source.parameter_number_message_number.changed(), |view| {
                view.invalidate_source_parameter_number_message_number_controls();
            });
        self.panel.when(source.is_registered.changed(), |view| {
            view.invalidate_source_is_registered_check_box();
        });
        self.panel.when(source.custom_character.changed(), |view| {
            view.invalidate_source_character_combo_box();
        });
        self.panel
            .when(source.midi_clock_transport_message.changed(), |view| {
                view.invalidate_source_midi_clock_transport_message_type_combo_box();
            });
    }

    fn invalidate_mode_controls(&self) {
        self.invalidate_mode_type_combo_box();
        self.invalidate_mode_control_appearance();
        self.invalidate_mode_source_value_controls();
        self.invalidate_mode_target_value_controls();
        self.invalidate_mode_step_size_controls();
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

    fn invalidate_mode_control_labels(&self) {
        // TODO Instead of always constructing the TargetWithContext object, we could provide
        //  a use_target_with_context(|t| t) function.
        let step_label = if self
            .mapping
            .with_context(self.session.context())
            .target_should_be_hit_with_increments()
        {
            "Step count"
        } else {
            "Step size"
        };
        self.view
            .require_control(root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT)
            .set_text(step_label);
    }

    fn invalidate_mode_control_visibilities(&self) {
        let (session, mapping, mode, target) = (self.session, self.mapping, self.mode, self.target);
        let target_with_context = target.with_context(session.context());
        self.show_if(
            mode.supports_round_target_value() && target_with_context.is_known_to_can_be_discrete(),
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
            mode.supports_rotate_is_enabled(),
            &[root::ID_SETTINGS_ROTATE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_ignore_out_of_range_source_values(),
            &[root::ID_SETTINGS_IGNORE_OUT_OF_RANGE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_step_size(),
            &[
                root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
            ],
        );
        let show_value_text = mapping
            .with_context(session.context())
            .target_should_be_hit_with_increments()
            || !target_with_context.is_known_to_be_discrete();
        self.show_if(
            mode.supports_step_size() && show_value_text,
            &[
                root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
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
            self.mode.source_value_interval.get_ref().min(),
        );
    }

    fn invalidate_mode_max_source_value_controls(&self) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval.get_ref().max(),
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
            .unwrap_or("".to_string());
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
            self.mode.target_value_interval.get_ref().min(),
        );
    }

    fn invalidate_mode_max_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
            self.mode.target_value_interval.get_ref().max(),
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
                        .unwrap_or("".to_string())
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

    fn get_text_right_to_target_edit_control(&self, t: &ReaperTarget, value: UnitValue) -> String {
        if t.can_parse_values() {
            t.unit().to_string()
        } else if t.character() == TargetCharacter::Discrete {
            // Please note that discrete FX parameters can only show their *current* value,
            // unless they implement the REAPER VST extension functions.
            t.format_value(value)
        } else {
            format!("{}  {}", t.unit(), t.format_value(value))
        }
    }

    fn invalidate_mode_min_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
            self.mode.jump_interval.get_ref().min(),
        );
    }

    fn invalidate_mode_max_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
            self.mode.jump_interval.get_ref().max(),
        );
    }

    fn invalidate_mode_step_size_controls(&self) {
        self.invalidate_mode_min_step_size_controls();
        self.invalidate_mode_max_step_size_controls();
    }

    fn invalidate_mode_min_step_size_controls(&self) {
        self.invalidate_mode_step_size_controls_internal(
            root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
            self.mode.step_size_interval.get_ref().min(),
        );
    }

    fn invalidate_mode_max_step_size_controls(&self) {
        self.invalidate_mode_step_size_controls_internal(
            root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
            self.mode.step_size_interval.get_ref().max(),
        );
    }

    fn invalidate_mode_step_size_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: UnitValue,
    ) {
        let (session, mapping) = (self.session, self.mapping);
        let (edit_text, value_text) = match &self.real_target() {
            Some(target) => {
                let send_increments = mapping
                    .with_context(session.context())
                    .target_should_be_hit_with_increments();
                let is_discrete = target.character() == TargetCharacter::Discrete;
                if send_increments || is_discrete {
                    let edit_text = target
                        .convert_unit_value_to_discrete_value(value)
                        .map(|v| v.to_string())
                        .unwrap_or("".to_string());
                    if send_increments {
                        // "count {x}"
                        (edit_text, "x".to_string())
                    } else {
                        // "count"
                        (edit_text, "".to_string())
                    }
                } else {
                    // "{size} {unit}"
                    let edit_text = target.format_value_without_unit(value);
                    let value_text = self.get_text_right_to_target_edit_control(target, value);
                    (edit_text, value_text)
                }
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
            .set_text(value_text)
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
        // let target_value_changed = MappingModel::target_value_changed(
        //     self.shared_mapping.clone(),
        //     self.session.context().clone(),
        // );
        // self.panel.when(target_value_changed, |view| {
        //     view.invalidate_target_value_controls();
        // });
        self.panel.when(target.r#type.changed(), |view| {
            view.invalidate_target_controls();
            view.invalidate_mode_controls();
        });
        self.panel.when(target.track.changed(), |view| {
            view.invalidate_target_controls();
            view.invalidate_mode_controls();
        });
        // TODO-high ReaLearn C++ had additional ugly code to keep the FX synced on fxAdded,
        //  fxRemoved  and fxReordered. See how it behaves in ReaLearn RS (uses other techniques)
        //  and - if still relevant - write hopefully not so ugly code to handle that.
        self.panel.when(
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
        self.panel.when(target.param_index.changed(), |view| {
            view.invalidate_target_value_controls();
            view.invalidate_mode_controls();
        });
        self.panel
            .when(target.action_invocation_type.changed(), |view| {
                view.invalidate_target_line_three();
                view.invalidate_mode_controls();
            });
    }

    fn register_mode_listeners(&self) {
        let mode = self.mode;
        self.panel.when(mode.r#type.changed(), |view| {
            view.invalidate_mode_control_appearance();
        });
        self.panel
            .when(mode.target_value_interval.changed(), |view| {
                view.invalidate_mode_min_target_value_controls();
                view.invalidate_mode_max_target_value_controls();
            });
        self.panel
            .when(mode.source_value_interval.changed(), |view| {
                view.invalidate_mode_source_value_controls();
            });
        self.panel.when(mode.jump_interval.changed(), |view| {
            view.invalidate_mode_min_jump_controls();
            view.invalidate_mode_max_jump_controls();
        });
        self.panel.when(mode.step_size_interval.changed(), |view| {
            view.invalidate_mode_step_size_controls();
        });
        self.panel
            .when(mode.ignore_out_of_range_source_values.changed(), |view| {
                view.invalidate_mode_ignore_out_of_range_check_box();
            });
        self.panel.when(mode.round_target_value.changed(), |view| {
            view.invalidate_mode_round_target_value_check_box();
        });
        self.panel
            .when(mode.approach_target_value.changed(), |view| {
                view.invalidate_mode_approach_check_box();
            });
        self.panel.when(mode.rotate.changed(), |view| {
            view.invalidate_mode_rotate_check_box();
        });
        self.panel.when(mode.reverse.changed(), |view| {
            view.invalidate_mode_reverse_check_box();
        });
        self.panel
            .when(mode.eel_control_transformation.changed(), |view| {
                view.invalidate_mode_eel_control_transformation_edit_control();
            });
        self.panel
            .when(mode.eel_feedback_transformation.changed(), |view| {
                view.invalidate_mode_eel_feedback_transformation_edit_control();
            });
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
        self.target
            .with_context(self.session.context())
            .create_target()
            .ok()
    }
}

impl View for MappingPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        self.memorize_all_slider_controls();
        true
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        self.hide();
        true
    }

    fn closed(self: SharedView<Self>, window: Window) {
        self.sliders.replace(None);
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        if matches!(resource_id, root::ID_OK | raw::IDCANCEL) {
            self.hide();
            return;
        }
        self.with_mutable(|p| {
            use root::*;
            match resource_id {
                // Mapping
                ID_MAPPING_CONTROL_ENABLED_CHECK_BOX => p.update_mapping_control_enabled(),
                ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX => p.update_mapping_feedback_enabled(),
                // Source
                ID_SOURCE_LEARN_BUTTON => p.toggle_learn_source(),
                ID_SOURCE_RPN_CHECK_BOX => p.update_source_is_registered(),
                ID_SOURCE_14_BIT_CHECK_BOX => p.update_source_is_14_bit(),
                // Mode
                ID_SETTINGS_ROTATE_CHECK_BOX => p.update_mode_rotate(),
                ID_SETTINGS_IGNORE_OUT_OF_RANGE_CHECK_BOX => {
                    p.update_mode_ignore_out_of_range_values()
                }
                ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX => p.update_mode_round_target_value(),
                ID_SETTINGS_SCALE_MODE_CHECK_BOX => p.update_mode_approach(),
                ID_SETTINGS_REVERSE_CHECK_BOX => p.update_mode_reverse(),
                ID_SETTINGS_RESET_BUTTON => p.reset_mode(),
                // Target
                ID_TARGET_INPUT_FX_CHECK_BOX => p.update_target_is_input_fx(),
                ID_TARGET_FX_FOCUS_CHECK_BOX => p.update_target_only_if_fx_has_focus(),
                ID_TARGET_TRACK_SELECTED_CHECK_BOX => p.update_target_only_if_track_is_selected(),
                ID_TARGET_LEARN_BUTTON => p.toggle_learn_target(),
                ID_TARGET_OPEN_BUTTON => p.open_target(),
                _ => unreachable!(),
            }
        });
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        self.with_mutable(|p| {
            use root::*;
            match resource_id {
                // Source
                ID_SOURCE_CHANNEL_COMBO_BOX => p.update_source_channel(),
                ID_SOURCE_NUMBER_COMBO_BOX => p.update_source_midi_message_number(),
                ID_SOURCE_CHARACTER_COMBO_BOX => p.update_source_character(),
                ID_SOURCE_TYPE_COMBO_BOX => p.update_source_type(),
                ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX => {
                    p.update_source_midi_clock_transport_message_type()
                }
                // Mode
                ID_SETTINGS_MODE_COMBO_BOX => p.update_mode_type(),
                // Target
                ID_TARGET_TYPE_COMBO_BOX => p.update_target_type(),
                ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX => {
                    p.update_target_track_or_command();
                }
                ID_TARGET_FX_OR_SEND_COMBO_BOX => {
                    p.update_target_from_combo_box_three();
                }
                ID_TARGET_FX_PARAMETER_COMBO_BOX => p.update_target_fx_parameter(),
                _ => unreachable!(),
            }
        });
    }

    fn slider_moved(self: SharedView<Self>, slider: Window) {
        self.with_mutable(|p| {
            use root::*;
            let sliders = p.panel.sliders.borrow();
            let sliders = sliders.as_ref().expect("sliders not set");
            match slider {
                // Mode
                s if s == sliders.mode_min_target_value => {
                    p.update_mode_min_target_value_from_slider(s)
                }
                s if s == sliders.mode_max_target_value => {
                    p.update_mode_max_target_value_from_slider(s)
                }
                s if s == sliders.mode_min_source_value => {
                    p.update_mode_min_source_value_from_slider(s)
                }
                s if s == sliders.mode_max_source_value => {
                    p.update_mode_max_source_value_from_slider(s)
                }
                s if s == sliders.mode_min_step_size => p.update_mode_min_step_size_from_slider(s),
                s if s == sliders.mode_max_step_size => p.update_mode_max_step_size_from_slider(s),
                s if s == sliders.mode_min_jump => p.update_mode_min_jump_from_slider(s),
                s if s == sliders.mode_max_jump => p.update_mode_max_jump_from_slider(s),
                s if s == sliders.target_value => p.update_target_value_from_slider(s),
                _ => unreachable!(),
            };
        });
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        if self.is_in_reaction() {
            // We don't want to continue if the edit control change was not caused by the user.
            // Although the edit control text is changed programmatically, it also triggers the
            // change handler. Ignore it! Most of those events are filtered out already
            // by the dialog proc reentrancy check, but this one is not because the
            // dialog proc is not reentered - we are just reacting (async) to a change.
            return false;
        }
        self.with_mutable(|p| {
            use root::*;
            match resource_id {
                // Mapping
                ID_MAPPING_NAME_EDIT_CONTROL => {
                    let _ = p.update_mapping_name();
                }
                // Source
                ID_SOURCE_NUMBER_EDIT_CONTROL => p.update_source_parameter_number_message_number(),
                // Mode
                ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL => {
                    p.update_mode_min_target_value_from_edit_control()
                }
                ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL => {
                    p.update_mode_max_target_value_from_edit_control()
                }
                ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL => {
                    p.update_mode_min_jump_from_edit_control()
                }
                ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL => {
                    p.update_mode_max_jump_from_edit_control()
                }
                ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL => {
                    p.update_mode_min_source_value_from_edit_control()
                }
                ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL => {
                    p.update_mode_max_source_value_from_edit_control()
                }
                ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL => {
                    p.update_mode_min_step_size_from_edit_control()
                }
                ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL => {
                    p.update_mode_max_step_size_from_edit_control()
                }
                ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL => {
                    p.update_mode_eel_control_transformation()
                }
                ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL => {
                    p.update_mode_eel_feedback_transformation()
                }
                // Target
                ID_TARGET_VALUE_EDIT_CONTROL => p.update_target_value_from_edit_control(),
                _ => return false,
            }
            true
        })
    }

    fn edit_control_focus_killed(self: SharedView<Self>, resource_id: u32) -> bool {
        // This is also called when the window is hidden.
        self.with_immutable(|p| {
            // The edit control which is currently edited by the user doesn't get invalidated during
            // `edit_control_changed()`, for good reasons. As soon as the edit control loses focus,
            // we should invalidate it. This is especially important if the user entered
            // an invalid value. Because we are lazy and edit controls are not
            // manipulated very frequently, we just invalidate all controls.
            p.invalidate_all_controls();
        });
        false
    }
}

trait WindowExt {
    fn slider_unit_value(&self) -> UnitValue;
    fn set_slider_unit_value(&self, value: UnitValue);
}

impl WindowExt for Window {
    fn slider_unit_value(&self) -> UnitValue {
        let discrete_value = self.slider_value();
        UnitValue::new(discrete_value as f64 / 100.0)
    }

    fn set_slider_unit_value(&self, value: UnitValue) {
        // TODO-low Refactor that map_to_interval stuff to be more generic and less boilerplate
        let slider_interval = Interval::new(DiscreteValue::new(0), DiscreteValue::new(100));
        self.set_slider_range(slider_interval.min().get(), slider_interval.max().get());
        let discrete_value = value.map_from_unit_interval_to_discrete(&slider_interval);
        self.set_slider_value(discrete_value.get());
    }
}
