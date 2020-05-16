use crate::infrastructure::ui::framework::{DialogUnits, Dimensions, Pixels};

/// The optimal size of the main panel in dialog units.
pub const MAIN_PANEL_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(449), DialogUnits(323));
