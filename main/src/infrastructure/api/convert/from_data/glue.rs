use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::ModeModelData;
use helgoboss_learn::{
    AbsoluteMode, ButtonUsage, DiscreteIncrement, EncoderUsage, FeedbackType, FireMode,
    GroupInteraction, OutOfRangeBehavior, TakeoverMode, UnitValue, VirtualColor,
};
use realearn_api::persistence;
use realearn_api::persistence::{NumericFeedback, PropColor, TextFeedback};

pub fn convert_glue(
    data: ModeModelData,
    style: ConversionStyle,
) -> ConversionResult<persistence::Glue> {
    let glue = persistence::Glue {
        absolute_mode: convert_absolute_mode(data.r#type, style),
        source_interval: style.required_value_with_default(
            convert_unit_interval(data.min_source_value, data.max_source_value),
            defaults::GLUE_SOURCE_INTERVAL,
        ),
        target_interval: style.required_value_with_default(
            convert_unit_interval(data.min_target_value, data.max_target_value),
            defaults::GLUE_TARGET_INTERVAL,
        ),
        reverse: style.required_value_with_default(data.reverse_is_enabled, defaults::GLUE_REVERSE),
        wrap: style.required_value_with_default(data.rotate_is_enabled, defaults::GLUE_WRAP),
        jump_interval: style.required_value_with_default(
            convert_unit_interval(data.min_target_jump, data.max_target_jump),
            defaults::GLUE_JUMP_INTERVAL,
        ),
        step_size_interval: {
            style.required_value_with_default(
                persistence::Interval(
                    data.min_step_size.get().abs(),
                    data.max_step_size.get().abs(),
                ),
                defaults::GLUE_STEP_SIZE_INTERVAL,
            )
        },
        step_factor_interval: {
            let interval = persistence::Interval(
                data.min_step_factor
                    .unwrap_or(DiscreteIncrement::POSITIVE_MIN)
                    .get(),
                data.max_step_factor
                    .unwrap_or(DiscreteIncrement::POSITIVE_MIN)
                    .get(),
            );
            style.required_value_with_default(interval, defaults::GLUE_STEP_FACTOR_INTERVAL)
        },
        out_of_range_behavior: {
            use persistence::OutOfRangeBehavior as T;
            use OutOfRangeBehavior::*;
            let v = match data.out_of_range_behavior {
                MinOrMax => T::MinOrMax,
                Min => T::Min,
                Ignore => T::Ignore,
            };
            style.required_value(v)
        },
        takeover_mode: {
            use persistence::TakeoverMode as T;
            use TakeoverMode::*;
            let v = match data.takeover_mode {
                Pickup => T::PickUp,
                LongTimeNoSee => T::LongTimeNoSee,
                Parallel => T::Parallel,
                CatchUp => T::CatchUp,
            };
            style.required_value(v)
        },
        round_target_value: style.required_value_with_default(
            data.round_target_value,
            defaults::GLUE_ROUND_TARGET_VALUE,
        ),
        control_transformation: style.required_value(data.eel_control_transformation),
        button_filter: {
            use persistence::ButtonFilter as T;
            use ButtonUsage::*;
            match data.button_usage {
                Both => None,
                PressOnly => Some(T::PressOnly),
                ReleaseOnly => Some(T::ReleaseOnly),
            }
        },
        encoder_filter: {
            use persistence::EncoderFilter as T;
            use EncoderUsage::*;
            match data.encoder_usage {
                Both => None,
                IncrementOnly => Some(T::IncrementOnly),
                DecrementOnly => Some(T::DecrementOnly),
            }
        },
        relative_mode: {
            let v = if data.make_absolute_enabled {
                persistence::RelativeMode::MakeAbsolute
            } else {
                persistence::RelativeMode::Normal
            };
            style.required_value(v)
        },
        interaction: {
            use persistence::Interaction as T;
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
        target_value_sequence: style.required_value(data.target_value_sequence.to_string()),
        feedback: {
            use persistence::Feedback as T;
            use FeedbackType::*;
            let v = match data.feedback_type {
                Numerical => T::Numeric(NumericFeedback {
                    commons: convert_feedback_commons(
                        data.feedback_color,
                        data.feedback_background_color,
                    )?,
                    transformation: style.required_value(data.eel_feedback_transformation),
                }),
                Textual => T::Text(TextFeedback {
                    commons: convert_feedback_commons(
                        data.feedback_color,
                        data.feedback_background_color,
                    )?,
                    text_expression: style.required_value(data.eel_feedback_transformation),
                }),
            };
            style.required_value(v)
        },
        fire_mode: {
            use persistence::FireMode as T;
            use FireMode::*;
            let v = match data.fire_mode {
                Normal => T::Normal(persistence::NormalFireMode {
                    press_duration_interval: {
                        let interval = persistence::Interval(
                            data.min_press_millis as _,
                            data.max_press_millis as _,
                        );
                        style.required_value_with_default(
                            interval,
                            defaults::FIRE_MODE_PRESS_DURATION_INTERVAL,
                        )
                    },
                }),
                AfterTimeout => T::AfterTimeout(persistence::AfterTimeoutFireMode {
                    timeout: style.required_value_with_default(
                        data.min_press_millis as _,
                        defaults::FIRE_MODE_TIMEOUT,
                    ),
                }),
                AfterTimeoutKeepFiring => {
                    T::AfterTimeoutKeepFiring(persistence::AfterTimeoutKeepFiringFireMode {
                        timeout: style.required_value_with_default(
                            data.min_press_millis as _,
                            defaults::FIRE_MODE_TIMEOUT,
                        ),
                        rate: style.required_value_with_default(
                            data.turbo_rate as _,
                            defaults::FIRE_MODE_RATE,
                        ),
                    })
                }
                OnSinglePress => T::OnSinglePress(persistence::OnSinglePressFireMode {
                    max_duration: style.required_value_with_default(
                        data.max_press_millis as _,
                        defaults::FIRE_MODE_SINGLE_PRESS_MAX_DURATION,
                    ),
                }),
                OnDoublePress => T::OnDoublePress(persistence::OnDoublePressFireMode),
            };
            style.required_value(v)
        },
        feedback_value_table: data.feedback_value_table,
    };
    Ok(glue)
}

fn convert_absolute_mode(
    v: AbsoluteMode,
    style: ConversionStyle,
) -> Option<persistence::AbsoluteMode> {
    use persistence::AbsoluteMode as T;
    use AbsoluteMode::*;
    let mode = match v {
        Normal => T::Normal,
        IncrementalButton => T::IncrementalButton,
        ToggleButton => T::ToggleButton,
        MakeRelative => T::MakeRelative,
        PerformanceControl => T::PerformanceControl,
    };
    style.required_value(mode)
}

fn convert_unit_interval(min: UnitValue, max: UnitValue) -> persistence::Interval<f64> {
    persistence::Interval(min.get(), max.get())
}

fn convert_virtual_color(v: VirtualColor) -> persistence::VirtualColor {
    use persistence::VirtualColor as T;
    use VirtualColor::*;
    match v {
        Rgb(c) => T::Rgb(persistence::RgbColor(c.r(), c.g(), c.b())),
        Prop { prop } => T::Prop(PropColor { prop }),
    }
}

fn convert_feedback_commons(
    color: Option<VirtualColor>,
    background_color: Option<VirtualColor>,
) -> ConversionResult<persistence::FeedbackCommons> {
    let commons = persistence::FeedbackCommons {
        color: color.map(convert_virtual_color),
        background_color: background_color.map(convert_virtual_color),
    };
    Ok(commons)
}
