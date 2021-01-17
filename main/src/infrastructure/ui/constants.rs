use swell_ui::{DialogUnits, Dimensions};

/// The optimal size of the main panel in dialog units.
pub const MAIN_PANEL_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(470), DialogUnits(423));

pub mod symbols {
    /// Previously we had 🡹 but this doesn't show on Windows 7.
    pub const fn arrow_up_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "↑"
        }
        #[cfg(target_os = "macos")]
        {
            "⬆"
        }
        #[cfg(target_os = "linux")]
        {
            "Up"
        }
    }

    /// Previously we had 🡻 but this doesn't show on Windows 7.
    pub const fn arrow_down_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "↓"
        }
        #[cfg(target_os = "macos")]
        {
            "⬇"
        }
        #[cfg(target_os = "linux")]
        {
            "Down"
        }
    }

    /// Previously we had 🡸 but this doesn't show on Windows 7.
    pub const fn arrow_left_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "←"
        }
        #[cfg(target_os = "macos")]
        {
            "⬅"
        }
        #[cfg(target_os = "linux")]
        {
            "<="
        }
    }

    /// Previously we had 🡺 but this doesn't show on Windows 7.
    pub const fn arrow_right_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "→"
        }
        #[cfg(target_os = "macos")]
        {
            "⮕"
        }
        #[cfg(target_os = "linux")]
        {
            "=>"
        }
    }
}
