use crate::infrastructure::ui::framework::{DialogUnits, Dimensions, Pixels};

/// The optimal size of the main view in dialog units.
pub const MAIN_VIEW_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(449), DialogUnits(323));
