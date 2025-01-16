use crate::base::CloneAsDefault;
use crate::domain::{AdditionalLuaMidiSourceScriptInput, FlexibleMidiSourceScript};
use helgoboss_learn::{FeedbackValue, MidiSourceScript, MidiSourceScriptOutcome};
use std::borrow::Cow;

/// The helgoboss-learn MidiSource, integrated into ReaLearn.
///
/// Now this needs some explanation: Why do we wrap the MIDI source script type with
/// `CloneAsDefault<Option<...>>`!? Because the script is compiled and therefore doesn't suit itself
/// to being cloned. But we need the MidiSource to be cloneable because we clone it whenever we
/// sync the mapping(s) from the main processor to the real-time processor. Fortunately, the
/// real-time processor doesn't use the compiled scripts anyway because those scripts are
/// responsible for feedback only.
///
/// Using `Arc` sounds like a good solution at first but it means that deallocation of the compiled
/// script could be triggered *in the real-time thread*. Now, we have a custom global deallocator
/// for automatically deferring deallocation if we are in a real-time thread. **But!** We use
/// non-Rust script engines (EEL and Lua), so they are not aware of our global allocator ... and
/// that means we would still get a real-time deallocation :/ Yes, we could handle this by manually
/// sending the obsolete structs to a deallocation thread *before* the Rust wrappers around the
/// script engines are even dropped (as we did before), but go there if the real-time processor
/// doesn't even use the scripts.
///
/// Introducing a custom method (not `clone`) would be quite much effort because we can't
/// derive its usage.
type ScriptType = CloneAsDefault<Option<FlexibleMidiSourceScript<'static>>>;

pub type MidiSource = helgoboss_learn::MidiSource<ScriptType>;

impl<'a> MidiSourceScript<'a> for ScriptType {
    type AdditionalInput = AdditionalLuaMidiSourceScriptInput<'a>;

    fn execute(
        &self,
        input_value: FeedbackValue,
        additional_input: Self::AdditionalInput,
    ) -> Result<MidiSourceScriptOutcome, Cow<'static, str>> {
        let script = self
            .get()
            .as_ref()
            .ok_or(Cow::Borrowed("script was removed on clone"))?;
        script.execute(input_value, additional_input)
    }
}
