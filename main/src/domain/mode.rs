use crate::base::CloneAsDefault;
use crate::domain::{
    AdditionalLuaFeedbackScriptInput, ControlEventTimestamp, EelTransformation, LuaFeedbackScript,
};
use base::hash_util::NonCryptoHashSet;
use helgoboss_learn::{FeedbackScript, FeedbackScriptInput, FeedbackScriptOutput, ModeContext};
use std::borrow::Cow;
use std::error::Error;

pub type RealearnModeContext<'a> = ModeContext<AdditionalLuaFeedbackScriptInput<'a>>;

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

impl<'a> FeedbackScript<'a> for FeedbackScriptType {
    type AdditionalInput = AdditionalLuaFeedbackScriptInput<'a>;

    fn feedback(
        &self,
        input: FeedbackScriptInput,
        additional_input: Self::AdditionalInput,
    ) -> Result<FeedbackScriptOutput, Cow<'static, str>> {
        self.get_script()?.feedback(input, additional_input)
    }

    fn used_props(&self) -> Result<NonCryptoHashSet<String>, Box<dyn Error>> {
        self.get_script()?.used_props()
    }
}
