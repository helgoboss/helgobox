use crate::domain::{MidiSourceModel, ModeModel, TargetModel};
use rx_util::LocalProp;

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Clone, Debug, Default)]
pub struct MappingModel<'a> {
    pub name: LocalProp<'a, String>,
    pub control_is_enabled: LocalProp<'a, bool>,
    pub feedback_is_enabled: LocalProp<'a, bool>,
    pub source_model: MidiSourceModel<'a>,
    pub mode_model: ModeModel<'a>,
    pub target_model: TargetModel<'a>,
}
