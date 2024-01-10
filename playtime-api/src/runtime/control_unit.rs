use crate::persistence::SlotAddress;
use serde::Serialize;

#[derive(Clone, PartialEq, Debug, Default, Serialize)]
pub struct ControlUnitConfig {
    #[serde(default)]
    pub control_units: Vec<ControlUnit>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Serialize)]
pub struct ControlUnitId(u32);

impl ControlUnitId {
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn get(&self) -> u32 {
        self.0
    }
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ControlUnit {
    pub id: ControlUnitId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_color: Option<u32>,
    pub top_left_corner: SlotAddress,
    pub column_count: u32,
    pub row_count: u32,
}
