use crate::application::UnitModel;
use crate::domain::{compartment_param_index_iter, CompartmentKind, Tag};
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
    use crate::infrastructure::ui::util::colors::ColorPair;
    use reaper_low::{raw, Swell};
    use reaper_medium::{Hbrush, Hdc};
    use swell_ui::{Color, ViewManager, Window};

    pub fn get_brush(color_pair: ColorPair) -> Option<Hbrush> {
        let color = if Window::dark_mode_is_enabled() {
            color_pair.dark
        } else {
            color_pair.light
        };
        ViewManager::get().get_solid_brush(color)
    }
}

pub mod colors {
    use palette::rgb::Rgb;
    use palette::{Darken, Hsl, IntoColor, Lighten, Srgb};
    use swell_ui::{color, Color};

    #[derive(Copy, Clone, Debug)]
    pub struct ColorPair {
        pub light: Color,
        pub dark: Color,
    }

    impl ColorPair {
        pub fn generate_from_light(light: Color) -> Self {
            Self {
                light,
                dark: invert_lightness(light),
            }
        }
    }

    fn invert_lightness(color: Color) -> Color {
        let hsl = color.to_hsl();
        Hsl::new_const(hsl.hue, hsl.saturation, 1.0 - hsl.lightness).into()
    }

    struct ColorPalette {
        /// For the light theme, lightens all colors by this factor.
        lighten_for_light_theme: f32,
        /// For dark theme, lightens all colors by this factor *after* inverting their lightness.
        lighten_for_dark_theme: f32,
        colors: [Color; 5],
    }

    impl ColorPalette {
        pub fn get(&self, index: usize) -> ColorPair {
            let original_color = self.colors[index];
            let light_color = original_color
                .to_linear_srgb()
                .lighten(self.lighten_for_light_theme)
                .into();
            ColorPair {
                light: light_color,
                dark: invert_lightness(original_color)
                    .to_linear_srgb()
                    .lighten(self.lighten_for_dark_theme)
                    .into(),
            }
        }
    }

    pub fn instance_panel_background() -> ColorPair {
        ColorPair::generate_from_light(tailwind::GRAY_200)
    }

    pub fn show_background() -> ColorPair {
        ColorPair::generate_from_light(tailwind::GRAY_300.to_linear_srgb().lighten(0.2).into())
    }

    pub fn mapping() -> ColorPair {
        MAPPING_PANEL_COLOR_PALETTE.get(0)
    }

    pub fn source() -> ColorPair {
        MAPPING_PANEL_COLOR_PALETTE.get(1)
    }

    pub fn target() -> ColorPair {
        MAPPING_PANEL_COLOR_PALETTE.get(2)
    }

    pub fn glue() -> ColorPair {
        MAPPING_PANEL_COLOR_PALETTE.get(3)
    }

    pub fn help() -> ColorPair {
        MAPPING_PANEL_COLOR_PALETTE.get(4)
    }

    /// Inspired by https://colorhunt.co/palette/96b6c5adc4ceeee0c9f1f0e8
    const MAPPING_PANEL_COLOR_PALETTE: ColorPalette = ColorPalette {
        lighten_for_light_theme: 0.4,
        lighten_for_dark_theme: 0.0,
        colors: [
            tailwind::GRAY_200,
            // The original color of the palette was ADC4CE, but that stands out too much.
            // This one is 20% lighter.
            color!("BDD0D8"),
            color!("EEE0C9"),
            color!("F1F0E8"),
            tailwind::GRAY_200,
        ],
    };

    // // 2. Nice pastel colors
    // const MAPPING_PANEL_COLOR_PALETTE: ColorPalette = ColorPalette {
    //     lighten: 0.0,
    //     colors: [SLATE_200, SKY_100, ORANGE_100, SLATE_100, SLATE_200],
    // };

    // 24. Not too bad. But too much similarity between source and target color.
    // const MAPPING_PANEL_COLOR_PALETTE: ColorPalette = ColorPalette {
    //     lighten: 0.5,
    //     colors: [
    //         color!("C9D7DD"),
    //         color!("FFF3CF"),
    //         color!("E8C872"),
    //         color!("C9D7DD"),
    //         color!("637A9F"),
    //     ],
    // };

    // // Alternative
    // const MAPPING_PANEL_COLOR_PALETTE: ColorPalette = ColorPalette {
    //     lighten: 0.5,
    //     colors: [
    //         color!("637A9F"),
    //         color!("FFF3CF"),
    //         color!("E8C872"),
    //         color!("C9D7DD"),
    //         color!("637A9F"),
    //     ],
    // };

    pub mod tailwind {
        use swell_ui::colors;

        colors! {
            SLATE_50 = "f8fafc";
            SLATE_100 = "f1f5f9";
            SLATE_200 = "e2e8f0";
            SLATE_300 = "cbd5e1";
            SLATE_400 = "94a3b8";
            SLATE_500 = "64748b";
            SLATE_600 = "475569";
            SLATE_700 = "334155";
            SLATE_800 = "1e293b";
            SLATE_900 = "0f172a";
            SLATE_950 = "020617";
            GRAY_50 = "f9fafb";
            GRAY_100 = "f3f4f6";
            GRAY_200 = "e5e7eb";
            GRAY_300 = "d1d5db";
            GRAY_400 = "9ca3af";
            GRAY_500 = "6b7280";
            GRAY_600 = "4b5563";
            GRAY_700 = "374151";
            GRAY_800 = "1f2937";
            GRAY_900 = "111827";
            GRAY_950 = "030712";
            ZINC_50 = "fafafa";
            ZINC_100 = "f4f4f5";
            ZINC_200 = "e4e4e7";
            ZINC_300 = "d4d4d8";
            ZINC_400 = "a1a1aa";
            ZINC_500 = "71717a";
            ZINC_600 = "52525b";
            ZINC_700 = "3f3f46";
            ZINC_800 = "27272a";
            ZINC_900 = "18181b";
            ZINC_950 = "09090b";
            NEUTRAL_50 = "fafafa";
            NEUTRAL_100 = "f5f5f5";
            NEUTRAL_200 = "e5e5e5";
            NEUTRAL_300 = "d4d4d4";
            NEUTRAL_400 = "a3a3a3";
            NEUTRAL_500 = "737373";
            NEUTRAL_600 = "525252";
            NEUTRAL_700 = "404040";
            NEUTRAL_800 = "262626";
            NEUTRAL_900 = "171717";
            NEUTRAL_950 = "0a0a0a";
            STONE_50 = "fafaf9";
            STONE_100 = "f5f5f4";
            STONE_200 = "e7e5e4";
            STONE_300 = "d6d3d1";
            STONE_400 = "a8a29e";
            STONE_500 = "78716c";
            STONE_600 = "57534e";
            STONE_700 = "44403c";
            STONE_800 = "292524";
            STONE_900 = "1c1917";
            STONE_950 = "0c0a09";
            RED_50 = "fef2f2";
            RED_100 = "fee2e2";
            RED_200 = "fecaca";
            RED_300 = "fca5a5";
            RED_400 = "f87171";
            RED_500 = "ef4444";
            RED_600 = "dc2626";
            RED_700 = "b91c1c";
            RED_800 = "991b1b";
            RED_900 = "7f1d1d";
            RED_950 = "450a0a";
            ORANGE_50 = "fff7ed";
            ORANGE_100 = "ffedd5";
            ORANGE_200 = "fed7aa";
            ORANGE_300 = "fdba74";
            ORANGE_400 = "fb923c";
            ORANGE_500 = "f97316";
            ORANGE_600 = "ea580c";
            ORANGE_700 = "c2410c";
            ORANGE_800 = "9a3412";
            ORANGE_900 = "7c2d12";
            ORANGE_950 = "431407";
            AMBER_50 = "fffbeb";
            AMBER_100 = "fef3c7";
            AMBER_200 = "fde68a";
            AMBER_300 = "fcd34d";
            AMBER_400 = "fbbf24";
            AMBER_500 = "f59e0b";
            AMBER_600 = "d97706";
            AMBER_700 = "b45309";
            AMBER_800 = "92400e";
            AMBER_900 = "78350f";
            AMBER_950 = "451a03";
            YELLOW_50 = "fefce8";
            YELLOW_100 = "fef9c3";
            YELLOW_200 = "fef08a";
            YELLOW_300 = "fde047";
            YELLOW_400 = "facc15";
            YELLOW_500 = "eab308";
            YELLOW_600 = "ca8a04";
            YELLOW_700 = "a16207";
            YELLOW_800 = "854d0e";
            YELLOW_900 = "713f12";
            YELLOW_950 = "422006";
            LIME_50 = "f7fee7";
            LIME_100 = "ecfccb";
            LIME_200 = "d9f99d";
            LIME_300 = "bef264";
            LIME_400 = "a3e635";
            LIME_500 = "84cc16";
            LIME_600 = "65a30d";
            LIME_700 = "4d7c0f";
            LIME_800 = "3f6212";
            LIME_900 = "365314";
            LIME_950 = "1a2e05";
            GREEN_50 = "f0fdf4";
            GREEN_100 = "dcfce7";
            GREEN_200 = "bbf7d0";
            GREEN_300 = "86efac";
            GREEN_400 = "4ade80";
            GREEN_500 = "22c55e";
            GREEN_600 = "16a34a";
            GREEN_700 = "15803d";
            GREEN_800 = "166534";
            GREEN_900 = "14532d";
            GREEN_950 = "052e16";
            EMERALD_50 = "ecfdf5";
            EMERALD_100 = "d1fae5";
            EMERALD_200 = "a7f3d0";
            EMERALD_300 = "6ee7b7";
            EMERALD_400 = "34d399";
            EMERALD_500 = "10b981";
            EMERALD_600 = "059669";
            EMERALD_700 = "047857";
            EMERALD_800 = "065f46";
            EMERALD_900 = "064e3b";
            EMERALD_950 = "022c22";
            TEAL_50 = "f0fdfa";
            TEAL_100 = "ccfbf1";
            TEAL_200 = "99f6e4";
            TEAL_300 = "5eead4";
            TEAL_400 = "2dd4bf";
            TEAL_500 = "14b8a6";
            TEAL_600 = "0d9488";
            TEAL_700 = "0f766e";
            TEAL_800 = "115e59";
            TEAL_900 = "134e4a";
            TEAL_950 = "042f2e";
            CYAN_50 = "ecfeff";
            CYAN_100 = "cffafe";
            CYAN_200 = "a5f3fc";
            CYAN_300 = "67e8f9";
            CYAN_400 = "22d3ee";
            CYAN_500 = "06b6d4";
            CYAN_600 = "0891b2";
            CYAN_700 = "0e7490";
            CYAN_800 = "155e75";
            CYAN_900 = "164e63";
            CYAN_950 = "083344";
            SKY_50 = "f0f9ff";
            SKY_100 = "e0f2fe";
            SKY_200 = "bae6fd";
            SKY_300 = "7dd3fc";
            SKY_400 = "38bdf8";
            SKY_500 = "0ea5e9";
            SKY_600 = "0284c7";
            SKY_700 = "0369a1";
            SKY_800 = "075985";
            SKY_900 = "0c4a6e";
            SKY_950 = "082f49";
            BLUE_50 = "eff6ff";
            BLUE_100 = "dbeafe";
            BLUE_200 = "bfdbfe";
            BLUE_300 = "93c5fd";
            BLUE_400 = "60a5fa";
            BLUE_500 = "3b82f6";
            BLUE_600 = "2563eb";
            BLUE_700 = "1d4ed8";
            BLUE_800 = "1e40af";
            BLUE_900 = "1e3a8a";
            BLUE_950 = "172554";
            INDIGO_50 = "eef2ff";
            INDIGO_100 = "e0e7ff";
            INDIGO_200 = "c7d2fe";
            INDIGO_300 = "a5b4fc";
            INDIGO_400 = "818cf8";
            INDIGO_500 = "6366f1";
            INDIGO_600 = "4f46e5";
            INDIGO_700 = "4338ca";
            INDIGO_800 = "3730a3";
            INDIGO_900 = "312e81";
            INDIGO_950 = "1e1b4b";
            VIOLET_50 = "f5f3ff";
            VIOLET_100 = "ede9fe";
            VIOLET_200 = "ddd6fe";
            VIOLET_300 = "c4b5fd";
            VIOLET_400 = "a78bfa";
            VIOLET_500 = "8b5cf6";
            VIOLET_600 = "7c3aed";
            VIOLET_700 = "6d28d9";
            VIOLET_800 = "5b21b6";
            VIOLET_900 = "4c1d95";
            VIOLET_950 = "2e1065";
            PURPLE_50 = "faf5ff";
            PURPLE_100 = "f3e8ff";
            PURPLE_200 = "e9d5ff";
            PURPLE_300 = "d8b4fe";
            PURPLE_400 = "c084fc";
            PURPLE_500 = "a855f7";
            PURPLE_600 = "9333ea";
            PURPLE_700 = "7e22ce";
            PURPLE_800 = "6b21a8";
            PURPLE_900 = "581c87";
            PURPLE_950 = "3b0764";
            FUCHSIA_50 = "fdf4ff";
            FUCHSIA_100 = "fae8ff";
            FUCHSIA_200 = "f5d0fe";
            FUCHSIA_300 = "f0abfc";
            FUCHSIA_400 = "e879f9";
            FUCHSIA_500 = "d946ef";
            FUCHSIA_600 = "c026d3";
            FUCHSIA_700 = "a21caf";
            FUCHSIA_800 = "86198f";
            FUCHSIA_900 = "701a75";
            FUCHSIA_950 = "4a044e";
            PINK_50 = "fdf2f8";
            PINK_100 = "fce7f3";
            PINK_200 = "fbcfe8";
            PINK_300 = "f9a8d4";
            PINK_400 = "f472b6";
            PINK_500 = "ec4899";
            PINK_600 = "db2777";
            PINK_700 = "be185d";
            PINK_800 = "9d174d";
            PINK_900 = "831843";
            PINK_950 = "500724";
            ROSE_50 = "fff1f2";
            ROSE_100 = "ffe4e6";
            ROSE_200 = "fecdd3";
            ROSE_300 = "fda4af";
            ROSE_400 = "fb7185";
            ROSE_500 = "f43f5e";
            ROSE_600 = "e11d48";
            ROSE_700 = "be123c";
            ROSE_800 = "9f1239";
            ROSE_900 = "881337";
            ROSE_950 = "4c0519";
        }
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
    compartment: CompartmentKind,
) -> impl Iterator<Item = (isize, String)> + '_ {
    compartment_param_index_iter().map(move |i| {
        let param_name = session
            .params()
            .compartment_params(compartment)
            .get_parameter_name(i);
        (i.get() as isize, format!("{}. {}", i.get() + 1, param_name))
    })
}

pub const GLOBAL_SCALING: DialogScaling = DialogScaling {
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

pub const HEADER_PANEL_SCALING: DialogScaling = DialogScaling {
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
