use crate::infrastructure::api::convert::defaults;
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::data::ModeModelData;
use helgoboss_learn::{DiscreteIncrement, SoftSymmetricUnitValue, UnitValue};
use helgobox_api::persistence::*;
use std::convert::TryInto;

pub fn convert_glue(g: Glue) -> ConversionResult<ModeModelData> {
    let source_interval =
        convert_unit_value_interval(g.source_interval.unwrap_or(defaults::GLUE_SOURCE_INTERVAL))?;
    let target_interval =
        convert_unit_value_interval(g.target_interval.unwrap_or(defaults::GLUE_TARGET_INTERVAL))?;
    let jump_interval =
        convert_unit_value_interval(g.jump_interval.unwrap_or(defaults::GLUE_JUMP_INTERVAL))?;
    let step_size_interval = convert_step_size_interval(
        g.step_size_interval
            .unwrap_or(defaults::GLUE_STEP_SIZE_INTERVAL),
    )?;
    let step_factor_interval = convert_step_factor_interval(
        g.step_factor_interval
            .unwrap_or(defaults::GLUE_STEP_FACTOR_INTERVAL),
    )?;
    let fire_mode = g.fire_mode.unwrap_or_default();
    struct FbCommonsData {
        color: Option<helgoboss_learn::VirtualColor>,
        background_color: Option<helgoboss_learn::VirtualColor>,
    }
    struct FbData {
        feedback_type: helgoboss_learn::FeedbackType,
        commons: FbCommonsData,
        transformation: String,
    }
    let fb_data = {
        use helgoboss_learn::FeedbackType as T;
        use Feedback::*;
        fn convert_fb_commons(commons: FeedbackCommons) -> FbCommonsData {
            FbCommonsData {
                color: commons.color.map(convert_virtual_color),
                background_color: commons.background_color.map(convert_virtual_color),
            }
        }
        match g.feedback.unwrap_or_default() {
            Numeric(fb) => FbData {
                feedback_type: T::Numeric,
                commons: convert_fb_commons(fb.commons),
                transformation: fb.transformation.unwrap_or_default(),
            },
            Text(fb) => FbData {
                feedback_type: T::Text,
                commons: convert_fb_commons(fb.commons),
                transformation: fb.text_expression.unwrap_or_default(),
            },
            Dynamic(fb) => FbData {
                feedback_type: T::Dynamic,
                commons: convert_fb_commons(fb.commons),
                transformation: fb.script.unwrap_or_default(),
            },
        }
    };
    let (min_press_millis, max_press_millis) = {
        use FireMode::*;
        match &fire_mode {
            Normal(m) => {
                let api_interval = m
                    .press_duration_interval
                    .unwrap_or(defaults::FIRE_MODE_PRESS_DURATION_INTERVAL);
                let interval = helgoboss_learn::Interval::try_new(
                    api_interval.0 as u64,
                    api_interval.1 as u64,
                )
                .map_err(anyhow::Error::msg)?;
                (interval.min_val(), interval.max_val())
            }
            OnSinglePress(m) => {
                let max = m
                    .max_duration
                    .unwrap_or(defaults::FIRE_MODE_SINGLE_PRESS_MAX_DURATION)
                    as u64;
                (0, max)
            }
            AfterTimeout(m) => {
                let min = m.timeout.unwrap_or(defaults::FIRE_MODE_TIMEOUT) as u64;
                (min, min)
            }
            AfterTimeoutKeepFiring(m) => {
                let min = m.timeout.unwrap_or(defaults::FIRE_MODE_TIMEOUT) as u64;
                (min, min)
            }
            OnDoublePress => (0, 0),
        }
    };
    let data = ModeModelData {
        r#type: {
            use helgoboss_learn::AbsoluteMode as T;
            use AbsoluteMode::*;
            match g.absolute_mode.unwrap_or_default() {
                Normal => T::Normal,
                IncrementalButton => T::IncrementalButton,
                ToggleButton => T::ToggleButton,
                MakeRelative => T::MakeRelative,
                PerformanceControl => T::PerformanceControl,
            }
        },
        min_source_value: source_interval.min_val(),
        max_source_value: source_interval.max_val(),
        min_target_value: target_interval.min_val(),
        max_target_value: target_interval.max_val(),
        min_target_jump: jump_interval.min_val(),
        max_target_jump: jump_interval.max_val(),
        min_step_size: step_size_interval.min_val(),
        max_step_size: step_size_interval.max_val(),
        min_step_factor: Some(step_factor_interval.min_val()),
        max_step_factor: Some(step_factor_interval.max_val()),
        min_press_millis,
        max_press_millis,
        turbo_rate: {
            use FireMode::*;
            match &fire_mode {
                AfterTimeoutKeepFiring(m) => m.rate.unwrap_or(defaults::FIRE_MODE_RATE) as u64,
                _ => 0,
            }
        },
        eel_control_transformation: g.control_transformation.unwrap_or_default(),
        eel_feedback_transformation: fb_data.transformation,
        reverse_is_enabled: g.reverse.unwrap_or(defaults::GLUE_REVERSE),
        feedback_color: fb_data.commons.color,
        feedback_background_color: fb_data.commons.background_color,
        ignore_out_of_range_source_values_is_enabled: false,
        out_of_range_behavior: {
            use helgoboss_learn::OutOfRangeBehavior as T;
            use OutOfRangeBehavior::*;
            match g.out_of_range_behavior.unwrap_or_default() {
                MinOrMax => T::MinOrMax,
                Min => T::Min,
                Ignore => T::Ignore,
            }
        },
        fire_mode: {
            use helgoboss_learn::FireMode as T;
            use FireMode::*;
            match &fire_mode {
                Normal(_) => T::Normal,
                AfterTimeout(_) => T::AfterTimeout,
                AfterTimeoutKeepFiring(_) => T::AfterTimeoutKeepFiring,
                OnSinglePress(_) => T::OnSinglePress,
                OnDoublePress => T::OnDoublePress,
            }
        },
        round_target_value: g
            .round_target_value
            .unwrap_or(defaults::GLUE_ROUND_TARGET_VALUE),
        scale_mode_enabled: false,
        takeover_mode: {
            use helgoboss_learn::TakeoverMode as T;
            use TakeoverMode::*;
            match g.takeover_mode.unwrap_or_default() {
                Off => T::Off,
                PickUpTolerant => T::PickupTolerant,
                PickUp => T::Pickup,
                LongTimeNoSee => T::LongTimeNoSee,
                Parallel => T::Parallel,
                CatchUp => T::CatchUp,
            }
        },
        button_usage: {
            use helgoboss_learn::ButtonUsage as T;
            if let Some(f) = g.button_filter {
                use ButtonFilter::*;
                match f {
                    PressOnly => T::PressOnly,
                    ReleaseOnly => T::ReleaseOnly,
                }
            } else {
                T::Both
            }
        },
        encoder_usage: {
            use helgoboss_learn::EncoderUsage as T;
            if let Some(f) = g.encoder_filter {
                use EncoderFilter::*;
                match f {
                    IncrementOnly => T::IncrementOnly,
                    DecrementOnly => T::DecrementOnly,
                }
            } else {
                T::Both
            }
        },
        rotate_is_enabled: g.wrap.unwrap_or(defaults::GLUE_WRAP),
        make_absolute_enabled: g.relative_mode.unwrap_or_default() == RelativeMode::MakeAbsolute,
        group_interaction: {
            use helgoboss_learn::GroupInteraction as T;
            if let Some(i) = g.interaction {
                use Interaction::*;
                match i {
                    SameControl => T::SameControl,
                    SameTargetValue => T::SameTargetValue,
                    InverseControl => T::InverseControl,
                    InverseTargetValue => T::InverseTargetValue,
                    InverseTargetValueOnOnly => T::InverseTargetValueOnOnly,
                    InverseTargetValueOffOnly => T::InverseTargetValueOffOnly,
                }
            } else {
                T::None
            }
        },
        target_value_sequence: if let Some(s) = g.target_value_sequence {
            s.parse().map_err(anyhow::Error::msg)?
        } else {
            Default::default()
        },
        feedback_type: fb_data.feedback_type,
        feedback_value_table: g.feedback_value_table,
    };
    Ok(data)
}

fn convert_step_factor_interval(
    i: Interval<i32>,
) -> ConversionResult<helgoboss_learn::Interval<DiscreteIncrement>> {
    let result = helgoboss_learn::Interval::try_new(
        i.0.try_into().unwrap_or(DiscreteIncrement::POSITIVE_MIN),
        i.1.try_into().unwrap_or(DiscreteIncrement::POSITIVE_MIN),
    )
    .map_err(anyhow::Error::msg)?;
    Ok(result)
}

fn convert_step_size_interval(
    i: Interval<f64>,
) -> ConversionResult<helgoboss_learn::Interval<SoftSymmetricUnitValue>> {
    let uv_interval = convert_unit_value_interval(i)?;
    let result = helgoboss_learn::Interval::try_new(
        uv_interval.min_val().to_symmetric(),
        uv_interval.max_val().to_symmetric(),
    )
    .map_err(anyhow::Error::msg)?;
    Ok(result)
}

fn convert_unit_value_interval(
    interval: Interval<f64>,
) -> ConversionResult<helgoboss_learn::Interval<UnitValue>> {
    let result = helgoboss_learn::Interval::try_new(
        interval.0.try_into().map_err(anyhow::Error::msg)?,
        interval.1.try_into().map_err(anyhow::Error::msg)?,
    )
    .map_err(anyhow::Error::msg)?;
    Ok(result)
}

fn convert_virtual_color(color: VirtualColor) -> helgoboss_learn::VirtualColor {
    use helgoboss_learn::VirtualColor as T;
    use VirtualColor::*;
    match color {
        Rgb(c) => T::Rgb(convert_rgb_color(c)),
        Prop(c) => T::Prop { prop: c.prop },
    }
}

fn convert_rgb_color(color: RgbColor) -> helgoboss_learn::RgbColor {
    helgoboss_learn::RgbColor::new(color.0, color.1, color.2)
}
