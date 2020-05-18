use crate::domain::{MidiSourceModel, ModeModel, Property, TargetModel};

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Clone, Debug, Default)]
pub struct MappingModel<'a> {
    pub name: Property<'a, String>,
    pub control_is_enabled: Property<'a, bool>,
    pub feedback_is_enabled: Property<'a, bool>,
    pub source_model: MidiSourceModel<'a>,
    pub mode_model: ModeModel<'a>,
    pub target_model: TargetModel<'a>,
}
