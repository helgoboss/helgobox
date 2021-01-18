use swell_ui::{DialogUnits, Dimensions};

/// The optimal size of the main panel in dialog units.
pub const MAIN_PANEL_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(470), DialogUnits(423));

pub mod symbols {
    pub fn arrow_up_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if arrows_are_supported() { "🡹" } else { "Up" }
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

    pub fn arrow_down_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if arrows_are_supported() {
                "🡻"
            } else {
                "Down"
            }
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

    pub fn arrow_left_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if arrows_are_supported() { "🡸" } else { "<=" }
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

    pub fn arrow_right_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if arrows_are_supported() { "🡺" } else { "=>" }
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

    #[cfg(target_os = "windows")]
    fn arrows_are_supported() -> bool {
        use once_cell::sync::Lazy;
        static SOMETHING_LIKE_WINDOWS_10: Lazy<bool> = Lazy::new(|| {
            let win_version = if let Ok(v) = sys_info::os_release() {
                v
            } else {
                return true;
            };
            win_version.as_str() >= "6.2"
        });
        *SOMETHING_LIKE_WINDOWS_10
    }
}

pub mod view {
    use once_cell::sync::Lazy;
    use reaper_low::{raw, Swell};

    pub fn erase_background_with(hwnd: raw::HWND, hdc: raw::HDC, brush: raw::HBRUSH) -> bool {
        unsafe {
            let swell = Swell::get();
            let mut rc = raw::RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            swell.GetClientRect(hwnd, &mut rc as *mut _);
            swell.FillRect(hdc, &rc, brush);
        }
        true
    }

    pub fn control_color_static_with(hdc: raw::HDC, brush: raw::HBRUSH) -> raw::HBRUSH {
        let swell = Swell::get();
        unsafe {
            swell.SetBkMode(hdc, raw::TRANSPARENT as _);
        }
        brush
    }

    pub fn row_brush() -> raw::HBRUSH {
        // static BRUSH: Lazy<isize> = Lazy::new(|| create_brush(225, 245, 254));
        // *BRUSH as _
        rows_brush()
    }

    pub fn rows_brush() -> raw::HBRUSH {
        static BRUSH: Lazy<isize> = Lazy::new(|| create_brush(252, 252, 252));
        *BRUSH as _
    }

    /// Use with care! Should be freed after use.
    fn create_brush(r: u8, g: u8, b: u8) -> isize {
        Swell::get().CreateSolidBrush(Swell::RGB(r, g, b) as _) as _
    }
}
