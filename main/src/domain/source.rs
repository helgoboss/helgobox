use crate::domain::AdditionalLuaMidiSourceScriptInput;
use helgoboss_learn::SourceContext;

pub type RealearnSourceContext<'a> = SourceContext<AdditionalLuaMidiSourceScriptInput<'a>>;
