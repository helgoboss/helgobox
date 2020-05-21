use crate::domain::{MidiSourceModel, ModeModel, TargetModel};
use rx_util::{LocalProp, LocalStaticProp};

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Clone, Debug, Default)]
pub struct MappingModel {
    pub name: LocalStaticProp<String>,
    pub control_is_enabled: LocalStaticProp<bool>,
    pub feedback_is_enabled: LocalStaticProp<bool>,
    pub source_model: MidiSourceModel,
    pub mode_model: ModeModel,
    pub target_model: TargetModel,
}
