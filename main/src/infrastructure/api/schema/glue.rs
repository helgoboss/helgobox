use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Glue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_mode: Option<AbsoluteMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_size_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_factor_interval: Option<Interval<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_transformation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_foreground_color: Option<VirtualColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_background_color: Option<VirtualColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_range_behavior: Option<OutOfRangeBehavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeover_mode: Option<TakeoverMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_target_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_transformation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button_filter: Option<ButtonFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_filter: Option<EncoderFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_mode: Option<RelativeMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction: Option<Interaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_kind: Option<FeedbackKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fire_mode: Option<FireMode>,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum AbsoluteMode {
    Normal,
    IncrementalButton,
    ToggleButton,
}

impl Default for AbsoluteMode {
    fn default() -> Self {
        AbsoluteMode::Normal
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub enum RelativeMode {
    Normal,
    MakeAbsolute,
}

impl Default for RelativeMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum FireMode {
    Normal(NormalFireMode),
    AfterTimeout(AfterTimeoutFireMode),
    AfterTimeoutKeepFiring(AfterTimeoutKeepFiringFireMode),
    OnSinglePress(OnSinglePressFireMode),
    OnDoublePress(OnDoublePressFireMode),
}

impl Default for FireMode {
    fn default() -> Self {
        Self::Normal(Default::default())
    }
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct NormalFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub press_duration_interval: Option<Interval<u32>>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct AfterTimeoutFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct AfterTimeoutKeepFiringFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    pub rate: Option<u32>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct OnSinglePressFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration: Option<u32>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct OnDoublePressFireMode;

#[derive(Debug, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum VirtualColor {
    Rgb(RgbColor),
    Prop(PropColor),
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, TS)]
pub struct RgbColor(pub u8, pub u8, pub u8);

#[derive(Debug, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct PropColor {
    pub prop: String,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum OutOfRangeBehavior {
    MinOrMax,
    Min,
    Ignore,
}

impl Default for OutOfRangeBehavior {
    fn default() -> Self {
        Self::MinOrMax
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum TakeoverMode {
    PickUp,
    LongTimeNoSee,
    Parallel,
    CatchUp,
}

impl Default for TakeoverMode {
    fn default() -> Self {
        Self::PickUp
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum ButtonFilter {
    PressOnly,
    ReleaseOnly,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum EncoderFilter {
    IncrementOnly,
    DecrementOnly,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum Interaction {
    SameControl,
    SameTargetValue,
    InverseControl,
    InverseTargetValue,
    InverseTargetValueOnOnly,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum FeedbackKind {
    Numeric,
    Text,
}

impl Default for FeedbackKind {
    fn default() -> Self {
        Self::Numeric
    }
}

#[derive(Copy, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
pub struct Interval<T>(pub T, pub T);
