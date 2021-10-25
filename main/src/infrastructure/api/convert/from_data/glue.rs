use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::api::schema::PropColor;
use crate::infrastructure::data::ModeModelData;
use helgoboss_learn::{
    AbsoluteMode, ButtonUsage, EncoderUsage, FeedbackType, FireMode, GroupInteraction,
    OutOfRangeBehavior, TakeoverMode, UnitValue, VirtualColor,
};

pub fn convert_glue(data: ModeModelData) -> ConversionResult<schema::Glue> {
    let glue = schema::Glue {
        absolute_mode: convert_absolute_mode(data.r#type),
        source_interval: convert_unit_interval(data.min_source_value, data.max_source_value),
        target_interval: convert_unit_interval(data.min_target_value, data.max_target_value),
        reverse: Some(data.reverse_is_enabled),
        wrap: Some(data.rotate_is_enabled),
        jump_interval: convert_unit_interval(data.min_target_jump, data.max_target_value),
        step_size_interval: {
            let interval = schema::Interval(data.min_step_size.get(), data.max_step_size.get());
            Some(interval)
        },
        step_factor_interval: {
            let interval = schema::Interval(
                (data.min_step_size.get() * 100.0) as i32,
                (data.max_step_size.get() * 100.0) as i32,
            );
            Some(interval)
        },
        feedback_transformation: Some(data.eel_feedback_transformation),
        feedback_foreground_color: data.feedback_color.map(convert_virtual_color),
        feedback_background_color: data.feedback_background_color.map(convert_virtual_color),
        out_of_range_behavior: {
            use schema::OutOfRangeBehavior as T;
            use OutOfRangeBehavior::*;
            let v = match data.out_of_range_behavior {
                MinOrMax => T::MinOrMax,
                Min => T::Min,
                Ignore => T::Ignore,
            };
            Some(v)
        },
        takeover_mode: {
            use schema::TakeoverMode as T;
            use TakeoverMode::*;
            let v = match data.takeover_mode {
                Pickup => T::PickUp,
                LongTimeNoSee => T::LongTimeNoSee,
                Parallel => T::Parallel,
                CatchUp => T::CatchUp,
            };
            Some(v)
        },
        round_target_value: Some(data.round_target_value),
        control_transformation: Some(data.eel_control_transformation),
        button_filter: {
            use schema::ButtonFilter as T;
            use ButtonUsage::*;
            match data.button_usage {
                Both => None,
                PressOnly => Some(T::PressOnly),
                ReleaseOnly => Some(T::ReleaseOnly),
            }
        },
        encoder_filter: {
            use schema::EncoderFilter as T;
            use EncoderUsage::*;
            match data.encoder_usage {
                Both => None,
                IncrementOnly => Some(T::IncrementOnly),
                DecrementOnly => Some(T::DecrementOnly),
            }
        },
        relative_mode: {
            let v = if data.make_absolute_enabled {
                schema::RelativeMode::MakeAbsolute
            } else {
                schema::RelativeMode::Normal
            };
            Some(v)
        },
        interaction: {
            use schema::Interaction as T;
            use GroupInteraction::*;
            match data.group_interaction {
                None => Option::None,
                SameControl => Some(T::SameControl),
                SameTargetValue => Some(T::SameTargetValue),
                InverseControl => Some(T::InverseControl),
                InverseTargetValue => Some(T::InverseTargetValue),
                InverseTargetValueOnOnly => Some(T::InverseTargetValueOnOnly),
            }
        },
        target_value_sequence: Some(data.target_value_sequence.to_string()),
        feedback_kind: {
            use schema::FeedbackKind as T;
            use FeedbackType::*;
            let v = match data.feedback_type {
                Numerical => T::Numeric,
                Textual => T::Text,
            };
            Some(v)
        },
        fire_mode: {
            use schema::FireMode as T;
            use FireMode::*;
            let v = match data.fire_mode {
                WhenButtonReleased => T::Normal(schema::NormalFireMode {
                    press_duration_interval: {
                        let interval = schema::Interval(
                            data.min_press_millis as _,
                            data.max_press_millis as _,
                        );
                        Some(interval)
                    },
                }),
                AfterTimeout => T::AfterTimeout(schema::AfterTimeoutFireMode {
                    timeout: Some(data.min_press_millis as _),
                }),
                AfterTimeoutKeepFiring => {
                    T::AfterTimeoutKeepFiring(schema::AfterTimeoutKeepFiringFireMode {
                        timeout: Some(data.min_press_millis as _),
                        rate: Some(data.turbo_rate as _),
                    })
                }
                OnSinglePress => T::OnSinglePress(schema::OnSinglePressFireMode {
                    max_duration: Some(data.max_press_millis as _),
                }),
                OnDoublePress => T::OnDoublePress(schema::OnDoublePressFireMode),
            };
            Some(v)
        },
    };
    Ok(glue)
}

fn convert_absolute_mode(v: AbsoluteMode) -> Option<schema::AbsoluteMode> {
    use schema::AbsoluteMode as T;
    use AbsoluteMode::*;
    let mode = match v {
        Normal => T::Normal,
        IncrementalButtons => T::IncrementalButton,
        ToggleButtons => T::ToggleButton,
    };
    Some(mode)
}

fn convert_unit_interval(min: UnitValue, max: UnitValue) -> Option<schema::Interval<f64>> {
    let interval = schema::Interval(min.get(), max.get());
    Some(interval)
}

fn convert_virtual_color(v: VirtualColor) -> schema::VirtualColor {
    use schema::VirtualColor as T;
    use VirtualColor::*;
    match v {
        Rgb(c) => T::Rgb(schema::RgbColor(c.r(), c.g(), c.b())),
        Prop { prop } => T::Prop(PropColor { prop }),
    }
}
