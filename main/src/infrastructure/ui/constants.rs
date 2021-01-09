use swell_ui::{DialogUnits, Dimensions};

/// The optimal size of the main panel in dialog units.
pub const MAIN_PANEL_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(470), DialogUnits(423));

pub mod symbols {
    #[cfg(target_os = "windows")]
    pub const ARROW_UP_SYMBOL: &str = "🡹";
    #[cfg(target_os = "macos")]
    pub const ARROW_UP_SYMBOL: &str = "⬆";
    #[cfg(target_os = "linux")]
    pub const ARROW_UP_SYMBOL: &str = "Up";

    #[cfg(target_os = "windows")]
    pub const ARROW_DOWN_SYMBOL: &str = "🡻";
    #[cfg(target_os = "macos")]
    pub const ARROW_DOWN_SYMBOL: &str = "⬇";
    #[cfg(target_os = "linux")]
    pub const ARROW_DOWN_SYMBOL: &str = "Down";

    #[cfg(target_os = "windows")]
    pub const ARROW_LEFT_SYMBOL: &str = "🡸";
    #[cfg(target_os = "macos")]
    pub const ARROW_LEFT_SYMBOL: &str = "⬅";
    #[cfg(target_os = "linux")]
    pub const ARROW_LEFT_SYMBOL: &str = "<=";

    #[cfg(target_os = "windows")]
    pub const ARROW_RIGHT_SYMBOL: &str = "🡺";
    #[cfg(target_os = "macos")]
    pub const ARROW_RIGHT_SYMBOL: &str = "⮕";
    #[cfg(target_os = "linux")]
    pub const ARROW_RIGHT_SYMBOL: &str = "=>";
}
