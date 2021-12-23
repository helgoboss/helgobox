use crate::application::{Change, ModeCommand, ModeModel};
use crate::base::default_util::{is_default, is_unit_value_one, unit_value_one};
use crate::infrastructure::data::MigrationDescriptor;
use crate::infrastructure::plugin::App;
use helgoboss_learn::{
    AbsoluteMode, ButtonUsage, EncoderUsage, FeedbackType, FireMode, GroupInteraction, Interval,
    OutOfRangeBehavior, SoftSymmetricUnitValue, TakeoverMode, UnitValue, ValueSequence,
    VirtualColor,
};
use serde::{Deserialize, Serialize};
use slog::debug;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeModelData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub r#type: AbsoluteMode,
    #[serde(default, skip_serializing_if = "is_default")]
    pub min_source_value: UnitValue,
    #[serde(default = "unit_value_one", skip_serializing_if = "is_unit_value_one")]
    pub max_source_value: UnitValue,
    #[serde(default, skip_serializing_if = "is_default")]
    pub min_target_value: UnitValue,
    #[serde(default = "unit_value_one", skip_serializing_if = "is_unit_value_one")]
    pub max_target_value: UnitValue,
    #[serde(default, skip_serializing_if = "is_default")]
    pub min_target_jump: UnitValue,
    #[serde(default = "unit_value_one", skip_serializing_if = "is_unit_value_one")]
    pub max_target_jump: UnitValue,
    #[serde(
        default = "default_step_size",
        skip_serializing_if = "is_default_step_size"
    )]
    pub min_step_size: SoftSymmetricUnitValue,
    #[serde(
        default = "default_step_size",
        skip_serializing_if = "is_default_step_size"
    )]
    pub max_step_size: SoftSymmetricUnitValue,
    #[serde(default, skip_serializing_if = "is_default")]
    pub min_press_millis: u64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub max_press_millis: u64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub turbo_rate: u64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub eel_control_transformation: String,
    /// Also used as text expression for text feedback
    #[serde(default, skip_serializing_if = "is_default")]
    pub eel_feedback_transformation: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub reverse_is_enabled: bool,
    pub feedback_color: Option<VirtualColor>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub feedback_background_color: Option<VirtualColor>,
    // Serialization skipped because this is deprecated in favor of out_of_range_behavior
    // since ReaLearn v1.11.0.
    #[serde(default, skip_serializing)]
    pub ignore_out_of_range_source_values_is_enabled: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub out_of_range_behavior: OutOfRangeBehavior,
    #[serde(default, skip_serializing_if = "is_default")]
    pub fire_mode: FireMode,
    #[serde(default, skip_serializing_if = "is_default")]
    pub round_target_value: bool,
    // Serialization skipped because this is deprecated in favor of takeover_mode
    // since ReaLearn v2.8.0-pre3.
    #[serde(default, skip_serializing)]
    pub scale_mode_enabled: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub takeover_mode: TakeoverMode,
    #[serde(default, skip_serializing_if = "is_default")]
    pub button_usage: ButtonUsage,
    #[serde(default, skip_serializing_if = "is_default")]
    pub encoder_usage: EncoderUsage,
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotate_is_enabled: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub make_absolute_enabled: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub group_interaction: GroupInteraction,
    #[serde(default, skip_serializing_if = "is_default")]
    pub target_value_sequence: ValueSequence,
    #[serde(default, skip_serializing_if = "is_default")]
    pub feedback_type: FeedbackType,
}

fn default_step_size() -> SoftSymmetricUnitValue {
    SoftSymmetricUnitValue::new(0.01)
}

fn is_default_step_size(v: &SoftSymmetricUnitValue) -> bool {
    *v == default_step_size()
}

impl ModeModelData {
    pub fn from_model(model: &ModeModel) -> Self {
        Self {
            r#type: model.absolute_mode(),
            min_source_value: model.source_value_interval().min_val(),
            max_source_value: model.source_value_interval().max_val(),
            min_target_value: model.target_value_interval().min_val(),
            max_target_value: model.target_value_interval().max_val(),
            min_target_jump: model.jump_interval().min_val(),
            max_target_jump: model.jump_interval().max_val(),
            min_step_size: model.step_interval().min_val(),
            max_step_size: model.step_interval().max_val(),
            min_press_millis: model.press_duration_interval().min_val().as_millis() as _,
            max_press_millis: model.press_duration_interval().max_val().as_millis() as _,
            turbo_rate: model.turbo_rate().as_millis() as _,
            eel_control_transformation: model.eel_control_transformation().to_owned(),
            eel_feedback_transformation: if model.feedback_type().is_textual() {
                model.textual_feedback_expression().to_owned()
            } else {
                model.eel_feedback_transformation().to_owned()
            },
            feedback_color: model.feedback_color().cloned(),
            feedback_background_color: model.feedback_background_color().cloned(),
            reverse_is_enabled: model.reverse(),
            // Not used anymore since ReaLearn v1.11.0
            ignore_out_of_range_source_values_is_enabled: false,
            out_of_range_behavior: model.out_of_range_behavior(),
            fire_mode: model.fire_mode(),
            round_target_value: model.round_target_value(),
            // Not used anymore since ReaLearn v2.8.0-pre3
            scale_mode_enabled: false,
            takeover_mode: model.takeover_mode(),
            button_usage: model.button_usage(),
            encoder_usage: model.encoder_usage(),
            rotate_is_enabled: model.rotate(),
            make_absolute_enabled: model.make_absolute(),
            group_interaction: model.group_interaction(),
            target_value_sequence: model.target_value_sequence().clone(),
            feedback_type: model.feedback_type(),
        }
    }

    pub fn apply_to_model(&self, model: &mut ModeModel) {
        self.apply_to_model_flexible(model, &MigrationDescriptor::default(), "");
    }

    pub fn apply_to_model_flexible(
        &self,
        model: &mut ModeModel,
        migration_descriptor: &MigrationDescriptor,
        mapping_name: &str,
    ) {
        use ModeCommand as P;
        model.change(P::SetAbsoluteMode(self.r#type));
        model.change(P::SetSourceValueInterval(Interval::new(
            self.min_source_value,
            self.max_source_value,
        )));
        {
            let saved_target_interval = Interval::new(self.min_target_value, self.max_target_value);
            let actual_target_interval = if migration_descriptor.target_interval_transformation_117
                && self.reverse_is_enabled
                && self.r#type == AbsoluteMode::Normal
            {
                debug!(
                    App::logger(),
                    "Migration: Inverting target interval of mapping {} in order to not break existing behavior because of #117",
                    mapping_name
                );
                saved_target_interval.inverse()
            } else {
                saved_target_interval
            };
            model.change(P::SetTargetValueInterval(actual_target_interval));
        }
        model.change(P::SetStepInterval(Interval::new(
            self.min_step_size,
            self.max_step_size,
        )));
        model.change(P::SetPressDurationInterval(Interval::new(
            Duration::from_millis(self.min_press_millis),
            Duration::from_millis(self.max_press_millis),
        )));
        model.change(P::SetTurboRate(Duration::from_millis(self.turbo_rate)));
        model.change(P::SetJumpInterval(Interval::new(
            self.min_target_jump,
            self.max_target_jump,
        )));
        model.change(P::SetEelControlTransformation(
            self.eel_control_transformation.clone(),
        ));
        let (eel_fb_transformation, textual_fb_expression) = if self.feedback_type.is_textual() {
            (String::new(), self.eel_feedback_transformation.clone())
        } else {
            (self.eel_feedback_transformation.clone(), String::new())
        };
        model.change(P::SetEelFeedbackTransformation(eel_fb_transformation));
        model.change(P::SetTextualFeedbackExpression(textual_fb_expression));
        model.change(P::SetFeedbackColor(self.feedback_color.clone()));
        model.change(P::SetFeedbackBackgroundColor(
            self.feedback_background_color.clone(),
        ));
        model.change(P::SetReverse(self.reverse_is_enabled));
        let actual_out_of_range_behavior = if self.ignore_out_of_range_source_values_is_enabled {
            // Data saved with ReaLearn version < 1.11.0
            OutOfRangeBehavior::Ignore
        } else {
            self.out_of_range_behavior
        };
        model.change(P::SetFireMode(self.fire_mode));
        model.change(P::SetOutOfRangeBehavior(actual_out_of_range_behavior));
        model.change(P::SetRoundTargetValue(self.round_target_value));
        let takeover_mode = if self.scale_mode_enabled {
            // ReaLearn < 2.8.0-pre3 used this flag instead of the enum.
            TakeoverMode::LongTimeNoSee
        } else {
            self.takeover_mode
        };
        model.change(P::SetTakeoverMode(takeover_mode));
        model.change(P::SetButtonUsage(self.button_usage));
        model.change(P::SetEncoderUsage(self.encoder_usage));
        model.change(P::SetRotate(self.rotate_is_enabled));
        model.change(P::SetMakeAbsolute(self.make_absolute_enabled));
        model.change(P::SetGroupInteraction(self.group_interaction));
        model.change(P::SetTargetValueSequence(
            self.target_value_sequence.clone(),
        ));
        model.change(P::SetFeedbackType(self.feedback_type));
    }
}
