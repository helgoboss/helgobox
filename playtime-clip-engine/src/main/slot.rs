use crate::main::Clip;

#[derive(Clone, Debug, Default)]
pub struct Slot {
    pub clip: Option<Clip>,
}
