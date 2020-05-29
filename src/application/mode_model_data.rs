use crate::domain::{ModeModel, ModeType};
use helgoboss_learn::{Interval, UnitValue};
use serde::{Deserialize, Serialize};
use validator::ValidationErrors;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModeModelData {
    r#type: ModeType,
    min_source_value: UnitValue,
    max_source_value: UnitValue,
    min_target_value: UnitValue,
    max_target_value: UnitValue,
    min_target_jump: UnitValue,
    max_target_jump: UnitValue,
    min_step_size: UnitValue,
    max_step_size: UnitValue,
    eel_control_transformation: String,
    eel_feedback_transformation: String,
    reverse_is_enabled: bool,
    ignore_out_of_range_source_values_is_enabled: bool,
    round_target_value: bool,
    scale_mode_enabled: bool,
    rotate_is_enabled: bool,
}

impl Default for ModeModelData {
    fn default() -> Self {
        Self {
            r#type: ModeType::Absolute,
            min_source_value: UnitValue::MIN,
            max_source_value: UnitValue::MAX,
            min_target_value: UnitValue::MIN,
            max_target_value: UnitValue::MAX,
            min_target_jump: UnitValue::MIN,
            max_target_jump: UnitValue::MAX,
            min_step_size: UnitValue::new(0.01),
            max_step_size: UnitValue::new(0.01),
            eel_control_transformation: "".to_string(),
            eel_feedback_transformation: "".to_string(),
            reverse_is_enabled: false,
            ignore_out_of_range_source_values_is_enabled: false,
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
            min_source_value: model.source_value_interval.get_ref().min(),
            max_source_value: model.source_value_interval.get_ref().max(),
            min_target_value: model.target_value_interval.get_ref().min(),
            max_target_value: model.target_value_interval.get_ref().max(),
            min_target_jump: model.jump_interval.get_ref().min(),
            max_target_jump: model.jump_interval.get_ref().max(),
            min_step_size: model.step_size_interval.get_ref().min(),
            max_step_size: model.step_size_interval.get_ref().max(),
            eel_control_transformation: model.eel_control_transformation.get_ref().clone(),
            eel_feedback_transformation: model.eel_feedback_transformation.get_ref().clone(),
            reverse_is_enabled: model.reverse.get(),
            ignore_out_of_range_source_values_is_enabled: model
                .ignore_out_of_range_source_values
                .get(),
            round_target_value: model.round_target_value.get(),
            scale_mode_enabled: model.approach_target_value.get(),
            rotate_is_enabled: model.rotate.get(),
        }
    }

    pub fn apply_to_model(&self, model: &mut ModeModel) -> Result<(), ValidationErrors> {
        model.r#type.set(self.r#type);
        model
            .source_value_interval
            .set(Interval::new(self.min_source_value, self.max_source_value));
        model
            .target_value_interval
            .set(Interval::new(self.min_target_value, self.max_target_value));
        model
            .step_size_interval
            .set(Interval::new(self.min_step_size, self.max_step_size));
        model
            .jump_interval
            .set(Interval::new(self.min_target_jump, self.max_target_jump));
        model
            .eel_control_transformation
            .set(self.eel_control_transformation.clone());
        model
            .eel_feedback_transformation
            .set(self.eel_feedback_transformation.clone());
        model.reverse.set(self.reverse_is_enabled);
        model
            .ignore_out_of_range_source_values
            .set(self.ignore_out_of_range_source_values_is_enabled);
        model.round_target_value.set(self.round_target_value);
        model.approach_target_value.set(self.scale_mode_enabled);
        model.rotate.set(self.rotate_is_enabled);
        Ok(())
    }
}
