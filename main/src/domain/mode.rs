use crate::base::CloneAsDefault;
use crate::domain::{ControlEventTimestamp, EelTransformation, LuaFeedbackScript};
use helgoboss_learn::{FeedbackScript, FeedbackScriptInput, FeedbackScriptOutput};
use std::borrow::Cow;
use std::collections::HashSet;
use std::error::Error;

/// See [`crate::domain::MidiSource`] for an explanation of the feedback script wrapping.
type FeedbackScriptType = CloneAsDefault<Option<LuaFeedbackScript<'static>>>;

pub type Mode = helgoboss_learn::Mode<EelTransformation, FeedbackScriptType, ControlEventTimestamp>;

impl FeedbackScriptType {
    fn get_script(&self) -> Result<&LuaFeedbackScript<'static>, Cow<'static, str>> {
        self.get()
            .as_ref()
            .ok_or(Cow::Borrowed("script was removed on clone"))
    }
}

impl FeedbackScript for FeedbackScriptType {
    fn feedback(
        &self,
        input: FeedbackScriptInput,
    ) -> Result<FeedbackScriptOutput, Cow<'static, str>> {
        self.get_script()?.feedback(input)
    }

    fn used_props(&self) -> Result<HashSet<String>, Box<dyn Error>> {
        self.get_script()?.used_props()
    }
}
