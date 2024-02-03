use palette::Srgb;
use reaper_low::raw;
use reaper_low::raw::{HBRUSH, HDC};
use std::fmt::Debug;

use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct ColorPanel {
    view: ViewContext,
    label: &'static str,
    light_theme_color: Srgb<u8>,
    dark_theme_color: Srgb<u8>,
}

impl ColorPanel {
    pub fn new(
        label: &'static str,
        light_theme_color: Srgb<u8>,
        dark_theme_color: Srgb<u8>,
    ) -> Self {
        Self {
            view: Default::default(),
            label,
            light_theme_color,
            dark_theme_color,
        }
    }
}

impl View for ColorPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_COLOR_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        // self.view
        //     .require_control(root::ID_COLOR_PANEL_LABEL)
        //     .set_text(self.label);
        true
    }

    fn control_color_static(self: SharedView<Self>, hdc: HDC, window: Window) -> HBRUSH {
        self.control_color_dialog(hdc, window)
    }

    fn control_color_dialog(self: SharedView<Self>, hdc: raw::HDC, window: Window) -> raw::HBRUSH {
        let color = if Window::dark_mode_is_enabled() {
            self.dark_theme_color
        } else {
            self.light_theme_color
        };
        util::view::control_color_dialog_default(hdc, color)
    }
}
