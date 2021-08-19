use reaper_high::Reaper;
use swell_ui::{DialogUnits, Dimensions, Window};

/// The optimal size of the main panel in dialog units.
pub const MAIN_PANEL_DIMENSIONS: Dimensions<DialogUnits> =
    Dimensions::new(DialogUnits(470), DialogUnits(447));

pub mod symbols {
    pub fn arrow_up_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if arrows_are_supported() {
                "🡹"
            } else {
                "Up"
            }
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
            if arrows_are_supported() {
                "🡸"
            } else {
                "<="
            }
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
            if arrows_are_supported() {
                "🡺"
            } else {
                "=>"
            }
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
    use std::ptr::null_mut;
    use swell_ui::Window;

    const SHADED_WHITE: (u8, u8, u8) = (248, 248, 248);
    const ORANGE: (u8, u8, u8) = (255, 87, 34);

    pub fn control_color_static_default(hdc: raw::HDC, brush: Option<raw::HBRUSH>) -> raw::HBRUSH {
        unsafe {
            Swell::get().SetBkMode(hdc, raw::TRANSPARENT as _);
        }
        brush.unwrap_or(null_mut())
    }

    pub fn control_color_dialog_default(_hdc: raw::HDC, brush: Option<raw::HBRUSH>) -> raw::HBRUSH {
        brush.unwrap_or(null_mut())
    }

    pub fn mapping_row_background_brush() -> Option<raw::HBRUSH> {
        static BRUSH: Lazy<Option<isize>> = Lazy::new(create_mapping_row_background_brush);
        let brush = (*BRUSH)?;
        Some(brush as _)
    }

    pub fn match_indicator_brush() -> raw::HBRUSH {
        static BRUSH: Lazy<isize> = Lazy::new(create_match_indicator_brush);
        *BRUSH as _
    }

    /// Use with care! Should be freed after use.
    fn create_mapping_row_background_brush() -> Option<isize> {
        if Window::dark_mode_is_enabled() {
            None
        } else {
            Some(create_brush(SHADED_WHITE))
        }
    }

    /// Use with care! Should be freed after use.
    fn create_match_indicator_brush() -> isize {
        create_brush(ORANGE)
    }

    /// Use with care! Should be freed after use.
    fn create_brush(color: (u8, u8, u8)) -> isize {
        Swell::get().CreateSolidBrush(rgb(color)) as _
    }

    fn rgb((r, g, b): (u8, u8, u8)) -> std::os::raw::c_int {
        Swell::RGB(r, g, b) as _
    }
}

pub fn open_in_browser(url: &str) {
    if webbrowser::open(url).is_err() {
        Reaper::get().show_console_msg(
            format!("Couldn't open browser. Please open the following address in your browser manually:\n\n{}\n\n", url)
        );
    }
}

pub fn open_in_text_editor(
    text: &str,
    parent_window: Window,
    suffix: &str,
) -> Result<String, &'static str> {
    edit::edit_with_builder(&text, edit::Builder::new().prefix("realearn-").suffix(suffix)).map_err(|e| {
        use std::io::ErrorKind::*;
        let msg = match e.kind() {
            NotFound => "Couldn't find text editor.".to_owned(),
            InvalidData => {
                "File is not properly UTF-8 encoded. Either avoid any special characters or make sure you use UTF-8 encoding!".to_owned()
            }
            _ => e.to_string()
        };
        parent_window
            .alert("ReaLearn", format!("Couldn't obtain text:\n\n{}", msg));
        "couldn't obtain text"
    })
}
