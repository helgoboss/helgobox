use crate::application::ModeModel;
use helgoboss_learn::{AbsoluteMode, Interval, OutOfRangeBehavior, SymmetricUnitValue, UnitValue};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModeModelData {
    r#type: AbsoluteMode,
    min_source_value: UnitValue,
    max_source_value: UnitValue,
    min_target_value: UnitValue,
    max_target_value: UnitValue,
    min_target_jump: UnitValue,
    max_target_jump: UnitValue,
    min_step_size: SymmetricUnitValue,
    max_step_size: SymmetricUnitValue,
    min_press_millis: u64,
    max_press_millis: u64,
    eel_control_transformation: String,
    eel_feedback_transformation: String,
    reverse_is_enabled: bool,
    // Serialization skipped because this is deprecated in favor of out_of_range_behavior
    // since ReaLearn v1.11.0.
    #[serde(skip_serializing)]
    ignore_out_of_range_source_values_is_enabled: bool,
    out_of_range_behavior: OutOfRangeBehavior,
    round_target_value: bool,
    scale_mode_enabled: bool,
    rotate_is_enabled: bool,
}

impl Default for ModeModelData {
    fn default() -> Self {
        Self {
            r#type: AbsoluteMode::Normal,
            min_source_value: UnitValue::MIN,
            max_source_value: UnitValue::MAX,
            min_target_value: UnitValue::MIN,
            max_target_value: UnitValue::MAX,
            min_target_jump: UnitValue::MIN,
            max_target_jump: UnitValue::MAX,
            min_step_size: SymmetricUnitValue::new(0.01),
            max_step_size: SymmetricUnitValue::new(0.01),
            min_press_millis: 0,
            max_press_millis: 0,
            eel_control_transformation: "".to_string(),
            eel_feedback_transformation: "".to_string(),
            reverse_is_enabled: false,
            ignore_out_of_range_source_values_is_enabled: false,
            out_of_range_behavior: OutOfRangeBehavior::MinOrMax,
            round_target_value: false,
            scale_mode_enabled: false,
            rotate_is_enabled: false,
        }
    }
}

impl ModeModelData {
    pub fn from_model(model: &ModeModel) -> Self {
        Self {
            r#type: model.r#type.get(),
            min_source_value: model.source_value_interval.get_ref().min_val(),
            max_source_value: model.source_value_interval.get_ref().max_val(),
            min_target_value: model.target_value_interval.get_ref().min_val(),
            max_target_value: model.target_value_interval.get_ref().max_val(),
            min_target_jump: model.jump_interval.get_ref().min_val(),
            max_target_jump: model.jump_interval.get_ref().max_val(),
            min_step_size: model.step_interval.get_ref().min_val(),
            max_step_size: model.step_interval.get_ref().max_val(),
            min_press_millis: model
                .press_duration_interval
                .get_ref()
                .min_val()
                .as_millis() as _,
            max_press_millis: model
                .press_duration_interval
                .get_ref()
                .max_val()
                .as_millis() as _,
            eel_control_transformation: model.eel_control_transformation.get_ref().clone(),
            eel_feedback_transformation: model.eel_feedback_transformation.get_ref().clone(),
            reverse_is_enabled: model.reverse.get(),
            // Not used anymore since ReaLearn v1.11.0
            ignore_out_of_range_source_values_is_enabled: false,
            out_of_range_behavior: model.out_of_range_behavior.get(),
            round_target_value: model.round_target_value.get(),
            scale_mode_enabled: model.approach_target_value.get(),
            rotate_is_enabled: model.rotate.get(),
        }
    }

    pub fn apply_to_model(&self, model: &mut ModeModel) {
        model.r#type.set_without_notification(self.r#type);
        model
            .source_value_interval
            .set_without_notification(Interval::new(self.min_source_value, self.max_source_value));
        model
            .target_value_interval
            .set_without_notification(Interval::new(self.min_target_value, self.max_target_value));
        model
            .step_interval
            .set_without_notification(Interval::new(self.min_step_size, self.max_step_size));
        model
            .press_duration_interval
            .set_without_notification(Interval::new(
                Duration::from_millis(self.min_press_millis),
                Duration::from_millis(self.max_press_millis),
            ));
        model
            .jump_interval
            .set_without_notification(Interval::new(self.min_target_jump, self.max_target_jump));
        model
            .eel_control_transformation
            .set_without_notification(self.eel_control_transformation.clone());
        model
            .eel_feedback_transformation
            .set_without_notification(self.eel_feedback_transformation.clone());
        model
            .reverse
            .set_without_notification(self.reverse_is_enabled);
        let actual_out_of_range_behavior = if self.ignore_out_of_range_source_values_is_enabled {
            // Data saved with ReaLearn version < 1.11.0
            OutOfRangeBehavior::Ignore
        } else {
            self.out_of_range_behavior
        };
        model
            .out_of_range_behavior
            .set_without_notification(actual_out_of_range_behavior);
        model
            .round_target_value
            .set_without_notification(self.round_target_value);
        model
            .approach_target_value
            .set_without_notification(self.scale_mode_enabled);
        model
            .rotate
            .set_without_notification(self.rotate_is_enabled);
    }
}
