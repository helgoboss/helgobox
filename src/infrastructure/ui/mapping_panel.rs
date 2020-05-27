use crate::domain::{
    MappingModel, MidiControlInput, MidiFeedbackOutput, MidiSourceModel, MidiSourceType, ModeModel,
    ModeType, ReaperTarget, Session, SharedMappingModel, TargetCharacter, TargetModel, TargetType,
};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use c_str_macro::c_str;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    ControlValue, DiscreteValue, Interval, MidiClockTransportMessage, SourceCharacter, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_low::{raw, Swell};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId, ReaperString};
use rx_util::{LocalProp, UnitEvent};
use rxrust::prelude::*;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::iter;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::str::FromStr;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

/// The upper part of the main panel, containing buttons such as "Add mapping".
pub struct MappingPanel {
    view: ViewContext,
    session: SharedSession,
    mapping: SharedMappingModel,
}

impl MappingPanel {
    pub fn new(session: SharedSession, mapping: SharedMappingModel) -> MappingPanel {
        MappingPanel {
            view: Default::default(),
            session,
            mapping,
        }
    }

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
            .set_text(format!("Edit mapping {}", self.mapping().name.get_ref()));
    }

    fn invalidate_mapping_name_edit_control(&self) {
        let c = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL);
        if c.has_focus() {
            return;
        }
        c.set_text(self.mapping().name.get_ref().as_str());
    }

    fn invalidate_mapping_control_enabled_check_box(&self) {
        self.view
            .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
            .set_checked(self.mapping().control_is_enabled.get());
    }

    fn invalidate_mapping_feedback_enabled_check_box(&self) {
        self.view
            .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
            .set_checked(self.mapping().feedback_is_enabled.get());
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

    fn mapping(&self) -> Ref<MappingModel> {
        self.mapping.borrow()
    }

    fn mapping_mut(&self) -> RefMut<MappingModel> {
        self.mapping.borrow_mut()
    }

    fn source(&self) -> Ref<MidiSourceModel> {
        Ref::map(self.mapping(), |m| &m.source_model)
    }

    fn source_mut(&self) -> RefMut<MidiSourceModel> {
        RefMut::map(self.mapping_mut(), |m| &mut m.source_model)
    }

    fn mode(&self) -> Ref<ModeModel> {
        Ref::map(self.mapping(), |m| &m.mode_model)
    }

    fn mode_mut(&self) -> RefMut<ModeModel> {
        RefMut::map(self.mapping_mut(), |m| &mut m.mode_model)
    }

    fn target(&self) -> Ref<TargetModel> {
        Ref::map(self.mapping(), |m| &m.target_model)
    }

    fn target_mut(&self) -> RefMut<TargetModel> {
        RefMut::map(self.mapping_mut(), |m| &mut m.target_model)
    }

    fn invalidate_source_control_labels(&self) {
        self.view
            .require_control(root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT)
            .set_text(self.source().r#type.get().number_label())
    }

    fn invalidate_source_control_visibilities(&self) {
        let source = self.source();
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
            .select_combo_box_item(self.source().r#type.get().into());
    }

    fn invalidate_learn_source_button(&self) {
        self.invalidate_learn_button(
            self.session
                .borrow()
                .mapping_is_learning_source(self.mapping.as_ptr()),
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
        match self.source().channel.get() {
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
                self.source()
                    .is_14_bit
                    .get()
                    .expect("14-bit == None not yet supported"),
            );
    }

    fn invalidate_source_is_registered_check_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
            .set_checked(
                self.source()
                    .is_registered
                    .get()
                    .expect("registered == None not yet supported"),
            );
    }

    fn invalidate_source_midi_message_number_controls(&self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        match self.source().midi_message_number.get() {
            None => {
                b.select_combo_box_item_by_data(-1);
            }
            Some(n) => {
                b.select_combo_box_item_by_data(n.get() as _);
            }
        };
    }

    fn invalidate_source_parameter_number_message_number_controls(&self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        if c.has_focus() {
            return;
        }
        let text = match self.source().parameter_number_message_number.get() {
            None => "".to_string(),
            Some(n) => n.to_string(),
        };
        c.set_text(text)
    }

    fn invalidate_source_character_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX)
            .select_combo_box_item(self.source().custom_character.get().into());
    }

    fn invalidate_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .select_combo_box_item(self.source().midi_clock_transport_message.get().into());
    }

    fn toggle_learn_source(&self) {
        self.session.borrow_mut().toggle_learn_source(&self.mapping);
    }

    fn update_mapping_control_enabled(&self) {
        self.mapping_mut().control_is_enabled.set(
            self.view
                .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_feedback_enabled(&self) {
        self.mapping_mut().feedback_is_enabled.set(
            self.view
                .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_name(&self) -> Result<(), &'static str> {
        let value = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL)
            .text()?;
        self.mapping_mut().name.set(value);
        Ok(())
    }

    fn update_source_is_registered(&self) {
        self.source_mut().is_registered.set(Some(
            self.view
                .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
                .is_checked(),
        ));
    }

    fn update_source_is_14_bit(&self) {
        self.source_mut().is_14_bit.set(Some(
            self.view
                .require_control(root::ID_SOURCE_14_BIT_CHECK_BOX)
                .is_checked(),
        ));
    }

    fn update_source_channel(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(Channel::new(id as _)),
        };
        self.source_mut().channel.set(value);
    }

    fn update_source_midi_message_number(&self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(U7::new(id as _)),
        };
        self.source_mut().midi_message_number.set(value);
    }

    fn update_source_character(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        self.source_mut().custom_character.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid source character"),
        );
    }

    fn update_source_type(&self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        self.source_mut().r#type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid source type"),
        );
    }

    fn update_source_midi_clock_transport_message_type(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX);
        self.source_mut().midi_clock_transport_message.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid MTC message type"),
        );
    }

    fn update_source_parameter_number_message_number(&self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        let value = c.text().ok().and_then(|t| t.parse::<U14>().ok());
        self.source_mut().parameter_number_message_number.set(value);
    }

    fn invalidate_target_controls(&self) {
        // TODO
    }

    fn register_listeners(self: &SharedView<Self>) {
        self.register_session_listeners();
        self.register_mapping_listeners();
        self.register_source_listeners();
        self.register_target_listeners();
        self.register_settings_listeners();
    }

    fn register_session_listeners(self: &SharedView<Self>) {
        let session = self.session.borrow();
        self.when(session.mapping_which_learns_source.changed(), |view| {
            view.invalidate_learn_source_button();
        });
        // TODO
    }

    fn register_mapping_listeners(self: &SharedView<Self>) {
        self.when(self.mapping().name.changed(), |view| {
            view.invalidate_window_title();
            view.invalidate_mapping_name_edit_control();
        });
        self.when(self.mapping().control_is_enabled.changed(), |view| {
            view.invalidate_mapping_control_enabled_check_box();
        });
        self.when(self.mapping().feedback_is_enabled.changed(), |view| {
            view.invalidate_mapping_feedback_enabled_check_box();
        });
    }

    fn register_source_listeners(self: &SharedView<Self>) {
        let source = self.source();
        self.when(source.r#type.changed(), |view| {
            view.invalidate_source_type_combo_box();
            view.invalidate_source_control_appearance();
            view.invalidate_mode_controls();
        });
        self.when(source.channel.changed(), |view| {
            view.invalidate_source_channel_combo_box();
        });
        self.when(source.is_14_bit.changed(), |view| {
            view.invalidate_source_14_bit_check_box();
            view.invalidate_mode_controls();
            view.invalidate_source_control_appearance();
        });
        self.when(source.midi_message_number.changed(), |view| {
            view.invalidate_source_midi_message_number_controls();
        });
        self.when(source.parameter_number_message_number.changed(), |view| {
            view.invalidate_source_parameter_number_message_number_controls();
        });
        self.when(source.is_registered.changed(), |view| {
            view.invalidate_source_is_registered_check_box();
        });
        self.when(source.custom_character.changed(), |view| {
            view.invalidate_source_character_combo_box();
        });
        self.when(source.midi_clock_transport_message.changed(), |view| {
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
        self.invalidate_mode_scale_mode_check_box();
        self.invalidate_mode_reverse_check_box();
        self.invalidate_mode_eel_control_transformation_edit_control();
        self.invalidate_mode_eel_feedback_transformation_edit_control();
    }

    fn invalidate_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .select_combo_box_item(self.mode().r#type.get().into());
    }

    fn invalidate_mode_control_appearance(&self) {
        self.invalidate_mode_control_labels();
        self.invalidate_mode_control_visibilities();
    }

    fn session(&self) -> debug_cell::Ref<Session> {
        self.session.borrow()
    }

    fn invalidate_mode_control_labels(&self) {
        let step_label = if self
            .mapping()
            .target_should_be_hit_with_increments(self.session().containing_fx())
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
        let (session, mapping, mode, target) =
            (self.session(), self.mapping(), self.mode(), self.target());
        let target_with_context = target.with_context(session.containing_fx());
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
        let show_value_text = mapping.target_should_be_hit_with_increments(session.containing_fx())
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
            self.mode().min_source_value.get(),
        );
    }

    fn invalidate_mode_max_source_value_controls(&self) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
            self.mode().max_source_value.get(),
        );
    }

    fn invalidate_mode_source_value_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value: UnitValue,
    ) {
        let formatted_value = self
            .source()
            .format_control_value(ControlValue::Absolute(value))
            .unwrap_or("".to_string());
        self.view
            .require_control(edit_control_id)
            .set_text(formatted_value);
        self.view
            .require_control(slider_control_id)
            .set_slider_to_unit_value(value);
    }

    fn invalidate_mode_min_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_TEXT,
            self.mode().min_target_value.get(),
        );
    }

    fn invalidate_mode_max_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
            self.mode().max_target_value.get(),
        );
    }

    fn invalidate_target_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: UnitValue,
    ) {
        let (session, target) = (self.session(), self.target());
        let real_target = target
            .with_context(session.containing_fx())
            .create_target()
            .ok();
        let (edit_text, value_text) = match &real_target {
            Some(target) => {
                let edit_text = if target.character() == TargetCharacter::Discrete {
                    target
                        .convert_value_to_discrete_value(value)
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
            .set_slider_to_unit_value(value);
        self.view
            .require_control(edit_control_id)
            .set_text(edit_text);
        self.view
            .require_control(value_text_control_id)
            .set_text(value_text);
    }

    fn get_text_right_to_target_edit_control(&self, t: &ReaperTarget, value: UnitValue) -> String {
        if t.can_parse_real_values() {
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
            self.mode().min_jump.get(),
        );
    }

    fn invalidate_mode_max_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
            self.mode().max_jump.get(),
        );
    }

    fn invalidate_mode_step_size_controls(&self) {
        self.invalidate_mode_min_step_size_controls();
        self.invalidate_mode_max_step_size_controls();
    }

    fn invalidate_mode_min_step_size_controls(&self) {
        // TODO
    }

    fn invalidate_mode_max_step_size_controls(&self) {
        // TODO
    }

    fn invalidate_mode_step_size_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: UnitValue,
    ) {
        let (session, mapping, target_model) = (self.session(), self.mapping(), self.target());
        let target = target_model
            .with_context(session.containing_fx())
            .create_target()
            .ok();
        let (edit_text, value_text) = match &target {
            Some(target) => {
                if mapping.target_should_be_hit_with_increments(session.containing_fx()) {
                    let edit_text = target
                        .convert_step_size_to_step_count(value)
                        .map(|v| v.to_string())
                        .unwrap_or("".to_string());
                    (edit_text, "x".to_string())
                } else if target.character() == TargetCharacter::Discrete {
                    let edit_text = target
                        .convert_value_to_discrete_value(value)
                        .map(|v| v.to_string())
                        .unwrap_or("".to_string());
                    (edit_text, "".to_string())
                } else {
                    let edit_text = target.format_value_without_unit(value);
                    let value_text = self.get_text_right_to_target_edit_control(target, value);
                    (edit_text, value_text)
                }
            }
            None => ("".to_string(), "".to_string()),
        };
        self.view
            .require_control(slider_control_id)
            .set_slider_to_unit_value(value);
        self.view
            .require_control(edit_control_id)
            .set_text(edit_text);
        self.view
            .require_control(value_text_control_id)
            .set_text(value_text)
    }

    fn invalidate_mode_rotate_check_box(&self) {
        // TODO
    }

    fn invalidate_mode_ignore_out_of_range_check_box(&self) {
        // TODO
    }

    fn invalidate_mode_round_target_value_check_box(&self) {
        // TODO
    }

    fn invalidate_mode_scale_mode_check_box(&self) {
        // TODO
    }

    fn invalidate_mode_reverse_check_box(&self) {
        // TODO
    }

    fn invalidate_mode_eel_control_transformation_edit_control(&self) {
        // TODO
    }

    fn invalidate_mode_eel_feedback_transformation_edit_control(&self) {
        // TODO
    }

    fn register_target_listeners(self: &SharedView<Self>) {
        // TODO
    }

    fn register_settings_listeners(self: &SharedView<Self>) {
        // TODO
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

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when_async(event, reaction, &self, self.view.closed());
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
        self.fill_all_controls();
        self.invalidate_all_controls();
        self.register_listeners();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            // General
            ID_OK => self.close(),
            // Mapping
            ID_MAPPING_CONTROL_ENABLED_CHECK_BOX => self.update_mapping_control_enabled(),
            ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX => self.update_mapping_feedback_enabled(),
            // Source
            ID_SOURCE_LEARN_BUTTON => self.toggle_learn_source(),
            ID_SOURCE_RPN_CHECK_BOX => self.update_source_is_registered(),
            ID_SOURCE_14_BIT_CHECK_BOX => self.update_source_is_14_bit(),
            _ => unreachable!(),
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            // Source
            ID_SOURCE_CHANNEL_COMBO_BOX => self.update_source_channel(),
            ID_SOURCE_NUMBER_COMBO_BOX => self.update_source_midi_message_number(),
            ID_SOURCE_CHARACTER_COMBO_BOX => self.update_source_character(),
            ID_SOURCE_TYPE_COMBO_BOX => self.update_source_type(),
            ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX => {
                self.update_source_midi_clock_transport_message_type()
            }
            _ => unreachable!(),
        }
    }

    fn virtual_key_pressed(self: SharedView<Self>, key_code: u32) -> bool {
        // TODO Really not sure if this is necessary
        // Don't close this window just by pressing enter
        false
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        use root::*;
        match resource_id {
            // Mapping
            ID_MAPPING_NAME_EDIT_CONTROL => {
                let _ = self.update_mapping_name();
            }
            // Source
            ID_SOURCE_NUMBER_EDIT_CONTROL => self.update_source_parameter_number_message_number(),
            _ => return false,
        }
        true
    }
}

trait WindowExt {
    fn set_slider_to_unit_value(&self, value: UnitValue);
}

impl WindowExt for Window {
    fn set_slider_to_unit_value(&self, value: UnitValue) {
        let slider_interval = Interval::new(DiscreteValue::new(0), DiscreteValue::new(100));
        self.set_slider_range(slider_interval.min().get(), slider_interval.max().get());
        let discrete_value = value.map_from_unit_interval_to_discrete(&slider_interval);
        self.set_slider_value(discrete_value.get());
    }
}
