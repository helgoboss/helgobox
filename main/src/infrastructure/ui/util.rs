use crate::application::UnitModel;
use crate::domain::{compartment_param_index_iter, Compartment, Tag};
use crate::infrastructure::ui::bindings::root;
use realearn_dialogs::constants;
use reaper_high::Reaper;
use std::cell::RefCell;
use std::path::Path;
use std::str::FromStr;
use swell_ui::{DialogScaling, DialogUnits, Dimensions, SharedView, View, Window};

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
            format!("Couldn't open browser. Please open the following address in your browser manually:\n\n{url}\n\n")
        );
    }
}

#[cfg(target_os = "windows")]
const FILE_MANAGER_CMD: &str = "explorer";

#[cfg(target_os = "macos")]
const FILE_MANAGER_CMD: &str = "open";

#[cfg(target_os = "linux")]
const FILE_MANAGER_CMD: &str = "xdg-open";

pub fn open_in_file_manager(path: &Path) -> Result<(), &'static str> {
    let final_path = path
        .canonicalize()
        .map_err(|_| "couldn't canonicalize path")?;
    std::process::Command::new(FILE_MANAGER_CMD)
        .arg(final_path)
        .spawn()
        .map_err(|_| "couldn't execute command to open file manager")?;
    Ok(())
}

pub fn open_in_text_editor(
    text: &str,
    parent_window: Window,
    suffix: &str,
) -> Result<String, &'static str> {
    edit::edit_with_builder(text, edit::Builder::new().prefix("realearn-").suffix(suffix)).map_err(|e| {
        use std::io::ErrorKind::*;
        let msg = match e.kind() {
            NotFound => "Couldn't find text editor.".to_owned(),
            InvalidData => {
                "File is not properly UTF-8 encoded. Either avoid any special characters or make sure you use UTF-8 encoding!".to_owned()
            }
            _ => e.to_string()
        };
        parent_window
            .alert("ReaLearn", format!("Couldn't obtain text:\n\n{msg}"));
        "couldn't obtain text"
    })
}

pub fn parse_tags_from_csv(text: &str) -> Vec<Tag> {
    text.split(',')
        .filter_map(|item| Tag::from_str(item).ok())
        .collect()
}

pub fn compartment_parameter_dropdown_contents(
    session: &UnitModel,
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

pub fn open_child_panel_dyn<T: View + 'static>(
    panel_slot: &RefCell<Option<SharedView<dyn View>>>,
    panel: T,
    parent_window: Window,
) {
    let panel = SharedView::new(panel);
    let panel_clone = panel.clone();
    if let Some(existing_panel) = panel_slot.replace(Some(panel)) {
        existing_panel.close();
    };
    panel_clone.open(parent_window);
}

pub fn open_child_panel<T: View + 'static>(
    panel_slot: &RefCell<Option<SharedView<T>>>,
    panel: T,
    parent_window: Window,
) {
    let panel = SharedView::new(panel);
    let panel_clone = panel.clone();
    if let Some(existing_panel) = panel_slot.replace(Some(panel)) {
        existing_panel.close();
    };
    panel_clone.open(parent_window);
}

pub fn close_child_panel_if_open(panel: &RefCell<Option<SharedView<impl View + ?Sized>>>) {
    if let Some(existing_panel) = panel.take() {
        existing_panel.close();
    }
}

#[allow(dead_code)]
pub fn alert_feature_not_available() {
    // Made some progress with egui on Linux, but it's wonky. Flickering. Keyboard input
    // needs window to be refocused to work at all. And the worst: baseview makes the
    // window code run in a new thread. On macOS and Windows, the run_ui code all runs
    // in the main thread. Which is very convenient because it allows us to call REAPER
    // functions. I think we need to bypass baseview on Linux and write our own little
    // egui integration.
    crate::base::notification::alert(
        "This feature is not available in this installation of ReaLearn, either because it's \
        not yet supported on this operating system or it was intentionally not included in the build.",
    );
}
