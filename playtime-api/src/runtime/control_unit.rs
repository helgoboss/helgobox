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

/// A control unit represents a controller connected to Playtime.
///
/// While definitely a part of the Playtime domain, control units are **not** managed/persisted by
/// Playtime. That's the responsibility of the software that integrates Playtime and provides the
/// controller integration (in our case Helgobox with ReaLearn).
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ControlUnit {
    /// Uniquely identifies the control unit at runtime.
    ///
    /// In our case (Helgobox/ReaLearn), it's equal to the ReaLearn unit ID.
    pub id: ControlUnitId,
    /// A display name which should indicate what connected device we are talking about.
    pub name: String,
    /// Color in which the control unit should be visualized in the clip matrix.
    ///
    /// In our case (Helgobox/ReaLearn), the color is in most usage scenarios dictated by the
    /// global controller definition but should also work without using the "global controller"
    /// feature (by setting `custom_data.playtime.control_unit.palette_color` in the main
    /// compartment data).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette_color: Option<u32>,
    /// The top-left column/row which this control unit controls.
    ///
    /// This will change as the controller scrolls. So it must be changeable by ReaLearn's targets.
    pub top_left_corner: SlotAddress,
    /// Both column and row count are fixed.
    ///
    /// It should be dictated by the ReaLearn main compartment as it's a decision of the main preset
    /// which area of the controller's grid will be used for slot control.
    pub column_count: u32,
    pub row_count: u32,
}
