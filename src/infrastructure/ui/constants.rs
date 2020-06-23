use swell_ui::{DialogUnits, Dimensions};

/// The optimal size of the main panel in dialog units.
pub const MAIN_PANEL_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(449), DialogUnits(413));

pub mod symbols {
    #[cfg(target_family = "windows")]
    pub const ARROW_UP_SYMBOL: char = '🡹';
    #[cfg(target_family = "unix")]
    pub const ARROW_UP_SYMBOL: char = '⬆';

    #[cfg(target_family = "windows")]
    pub const ARROW_DOWN_SYMBOL: char = '🡻';
    #[cfg(target_family = "unix")]
    pub const ARROW_DOWN_SYMBOL: char = '⬇';

    #[cfg(target_family = "windows")]
    pub const ARROW_LEFT_SYMBOL: char = '🡸';
    #[cfg(target_family = "unix")]
    pub const ARROW_LEFT_SYMBOL: char = '⬅';

    #[cfg(target_family = "windows")]
    pub const ARROW_RIGHT_SYMBOL: char = '🡺';
    #[cfg(target_family = "unix")]
    pub const ARROW_RIGHT_SYMBOL: char = '⮕';
}
