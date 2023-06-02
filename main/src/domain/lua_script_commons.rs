use helgoboss_learn::{
    AbsoluteValue, FeedbackStyle, FeedbackValue, NumericFeedbackValue, RgbColor,
    TextualFeedbackValue, UnitValue,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ScriptFeedbackEvent {
    pub value: Option<ScriptFeedbackValue>,
    pub color: Option<ScriptColor>,
    pub background_color: Option<ScriptColor>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ScriptFeedbackValue {
    Unit(f64),
    Text(String),
    Complex(serde_json::Value),
}

impl ScriptFeedbackEvent {
    pub fn into_api_feedback_value(self) -> FeedbackValue<'static> {
        let style = FeedbackStyle {
            color: self.color.map(|c| c.into()),
            background_color: self.background_color.map(|c| c.into()),
        };
        match self.value {
            None => FeedbackValue::Off,
            Some(ScriptFeedbackValue::Unit(v)) => {
                FeedbackValue::Numeric(NumericFeedbackValue::new(
                    style,
                    AbsoluteValue::Continuous(UnitValue::new_clamped(v)),
                ))
            }
            Some(ScriptFeedbackValue::Text(t)) => {
                FeedbackValue::Textual(TextualFeedbackValue::new(style, t.into()))
            }
            Some(ScriptFeedbackValue::Complex(_)) => todo!(),
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ScriptColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<RgbColor> for ScriptColor {
    fn from(c: RgbColor) -> Self {
        Self {
            r: c.r(),
            g: c.g(),
            b: c.b(),
        }
    }
}

impl From<ScriptColor> for RgbColor {
    fn from(c: ScriptColor) -> Self {
        Self::new(c.r, c.g, c.b)
    }
}
