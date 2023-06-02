use crate::domain::{ControlEventTimestamp, EelTransformation, LuaFeedbackScript};

pub type Mode =
    helgoboss_learn::Mode<EelTransformation, LuaFeedbackScript<'static>, ControlEventTimestamp>;
