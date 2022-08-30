use crate::application::Session;
use crate::domain::{compartment_param_index_iter, Compartment, Tag};
use crate::infrastructure::ui::bindings::root;
use realearn_dialogs::constants;
use reaper_high::Reaper;
use std::str::FromStr;
use swell_ui::{DialogScaling, DialogUnits, Dimensions, Window};

/// The optimal size of the main panel in dialog units.
pub fn main_panel_dimensions() -> Dimensions<DialogUnits> {
    Dimensions::new(main_panel_width(), main_panel_height())
}

pub fn main_panel_width() -> DialogUnits {
    DialogUnits(constants::MAIN_PANEL_WIDTH).scale(GLOBAL_SCALING.width_scale)
}

pub fn main_panel_height() -> DialogUnits {
    header_panel_height() + mapping_rows_panel_height() + footer_panel_height()
}

pub fn header_panel_height() -> DialogUnits {
    DialogUnits(constants::HEADER_PANEL_HEIGHT).scale(HEADER_PANEL_SCALING.height_scale)
}

pub fn mapping_row_panel_height() -> DialogUnits {
    DialogUnits(constants::MAPPING_ROW_PANEL_HEIGHT).scale(GLOBAL_SCALING.height_scale)
}

pub fn mapping_rows_panel_height() -> DialogUnits {
    DialogUnits(constants::MAPPING_ROWS_PANEL_HEIGHT).scale(GLOBAL_SCALING.height_scale)
}

pub fn footer_panel_height() -> DialogUnits {
    DialogUnits(constants::FOOTER_PANEL_HEIGHT).scale(GLOBAL_SCALING.height_scale)
}

pub mod symbols {
    pub fn indicator_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if pretty_symbols_are_supported() {
                "●"
            } else {
                "*"
            }
        }
        #[cfg(target_os = "macos")]
        {
            "●"
        }
        #[cfg(target_os = "linux")]
        {
            "*"
        }
    }

    pub fn arrow_up_symbol() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            if pretty_symbols_are_supported() {
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
            if pretty_symbols_are_supported() {
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
            if pretty_symbols_are_supported() {
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
            if pretty_symbols_are_supported() {
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
    fn pretty_symbols_are_supported() -> bool {
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

    /// Use with care! Should be freed after use.
    fn create_mapping_row_background_brush() -> Option<isize> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            if swell_ui::Window::dark_mode_is_enabled() {
                None
            } else {
                const SHADED_WHITE: (u8, u8, u8) = (248, 248, 248);
                Some(create_brush(SHADED_WHITE))
            }
        }
        #[cfg(target_os = "linux")]
        {
            None
        }
    }

    /// Use with care! Should be freed after use.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn create_brush(color: (u8, u8, u8)) -> isize {
        Swell::get().CreateSolidBrush(rgb(color)) as _
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
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

pub fn parse_tags_from_csv(text: &str) -> Vec<Tag> {
    text.split(',')
        .filter_map(|item| Tag::from_str(item).ok())
        .collect()
}

pub fn compartment_parameter_dropdown_contents(
    session: &Session,
    compartment: Compartment,
) -> impl Iterator<Item = (isize, String)> + '_ {
    compartment_param_index_iter().map(move |i| {
        let param_name = session
            .params()
            .compartment_params(compartment)
            .get_parameter_name(i);
        (i.get() as isize, format!("{}. {}", i.get() + 1, param_name))
    })
}

const GLOBAL_SCALING: DialogScaling = DialogScaling {
    x_scale: root::GLOBAL_X_SCALE,
    y_scale: root::GLOBAL_Y_SCALE,
    width_scale: root::GLOBAL_WIDTH_SCALE,
    height_scale: root::GLOBAL_HEIGHT_SCALE,
};

pub const MAPPING_PANEL_SCALING: DialogScaling = DialogScaling {
    x_scale: root::MAPPING_PANEL_X_SCALE,
    y_scale: root::MAPPING_PANEL_Y_SCALE,
    width_scale: root::MAPPING_PANEL_WIDTH_SCALE,
    height_scale: root::MAPPING_PANEL_HEIGHT_SCALE,
};

const HEADER_PANEL_SCALING: DialogScaling = DialogScaling {
    x_scale: root::HEADER_PANEL_X_SCALE,
    y_scale: root::HEADER_PANEL_Y_SCALE,
    width_scale: root::HEADER_PANEL_WIDTH_SCALE,
    height_scale: root::HEADER_PANEL_HEIGHT_SCALE,
};
