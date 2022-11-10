use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

#[derive(PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Glue {
    //region Relevant for control and feedback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_mode: Option<AbsoluteMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_range_behavior: Option<OutOfRangeBehavior>,
    //endregion

    //region Relevant for control only (might change in future)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_target_value: Option<bool>,
    //endregion

    //region Relevant for control only (guaranteed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeover_mode: Option<TakeoverMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_transformation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_size_interval: Option<Interval<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_factor_interval: Option<Interval<i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button_filter: Option<ButtonFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_filter: Option<EncoderFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relative_mode: Option<RelativeMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction: Option<Interaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fire_mode: Option<FireMode>,
    //endregion

    //region Relevant for feedback only (guaranteed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<Feedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_value_table: Option<FeedbackValueTable>,
    //endregion
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum FeedbackValueTable {
    FromTextToDiscrete(FeedbackValueTableContent<String, u32>),
    FromTextToContinuous(FeedbackValueTableContent<String, f64>),
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackValueTableContent<K: Eq + Hash, V> {
    pub value: HashMap<K, V>,
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum AbsoluteMode {
    Normal,
    IncrementalButton,
    ToggleButton,
    MakeRelative,
    PerformanceControl,
}

impl Default for AbsoluteMode {
    fn default() -> Self {
        AbsoluteMode::Normal
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum RelativeMode {
    Normal,
    MakeAbsolute,
}

impl Default for RelativeMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct NormalFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub press_duration_interval: Option<Interval<u32>>,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct AfterTimeoutFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct AfterTimeoutKeepFiringFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    pub rate: Option<u32>,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct OnSinglePressFireMode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration: Option<u32>,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct OnDoublePressFireMode;

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VirtualColor {
    Rgb(RgbColor),
    Prop(PropColor),
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RgbColor(pub u8, pub u8, pub u8);

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PropColor {
    pub prop: String,
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum TakeoverMode {
    Normal,
    PickUpTolerant,
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

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum ButtonFilter {
    PressOnly,
    ReleaseOnly,
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum EncoderFilter {
    IncrementOnly,
    DecrementOnly,
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Interaction {
    SameControl,
    SameTargetValue,
    InverseControl,
    InverseTargetValue,
    InverseTargetValueOnOnly,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackCommons {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VirtualColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<VirtualColor>,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum Feedback {
    Numeric(NumericFeedback),
    Text(TextFeedback),
}

impl Default for Feedback {
    fn default() -> Self {
        Self::Numeric(NumericFeedback::default())
    }
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct NumericFeedback {
    #[serde(flatten)]
    pub commons: FeedbackCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transformation: Option<String>,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct TextFeedback {
    #[serde(flatten)]
    pub commons: FeedbackCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_expression: Option<String>,
}

#[derive(Copy, Clone, Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Interval<T>(pub T, pub T);
