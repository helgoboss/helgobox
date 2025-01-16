use crate::domain::{AdditionalLuaMidiSourceScriptInput, EelMidiSourceScript, LuaMidiSourceScript};
use helgoboss_learn::{FeedbackValue, MidiSourceScript, MidiSourceScriptOutcome};
use std::borrow::Cow;

#[derive(Debug)]
pub enum FlexibleMidiSourceScript<'lua> {
    Eel(EelMidiSourceScript),
    Lua(LuaMidiSourceScript<'lua>),
}

impl<'a, 'lua: 'a> MidiSourceScript<'a> for FlexibleMidiSourceScript<'lua> {
    type AdditionalInput = AdditionalLuaMidiSourceScriptInput<'a>;

    fn execute(
        &self,
        input_value: FeedbackValue,
        additional_input: Self::AdditionalInput,
    ) -> Result<MidiSourceScriptOutcome, Cow<'static, str>> {
        match self {
            FlexibleMidiSourceScript::Eel(s) => s.execute(input_value, ()),
            FlexibleMidiSourceScript::Lua(s) => s.execute(input_value, additional_input),
        }
    }
}
