use crate::domain::{EelMidiSourceScript, LuaMidiSourceScript};
use helgoboss_learn::{FeedbackValue, MidiSourceScript, MidiSourceScriptOutcome};

#[derive(Clone, Debug)]
pub enum FlexibleMidiSourceScript<'a> {
    Eel(EelMidiSourceScript),
    Lua(LuaMidiSourceScript<'a>),
}

impl<'a> MidiSourceScript for FlexibleMidiSourceScript<'a> {
    fn execute(&self, input_value: FeedbackValue) -> Result<MidiSourceScriptOutcome, &'static str> {
        match self {
            FlexibleMidiSourceScript::Eel(s) => s.execute(input_value),
            FlexibleMidiSourceScript::Lua(s) => s.execute(input_value),
        }
    }
}
