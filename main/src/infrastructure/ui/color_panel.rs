use palette::Srgb;
use reaper_low::raw;
use reaper_low::raw::{HBRUSH, HDC};
use reaper_medium::{Hbrush, Hdc};
use std::fmt::Debug;

use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util;
use crate::infrastructure::ui::util::MAPPING_PANEL_SCALING;
use swell_ui::{
    Color, DialogScaling, DialogUnits, Dimensions, Point, SharedView, View, ViewContext, Window,
    ZOrder,
};

#[derive(Debug)]
pub struct ColorPanel {
    view: ViewContext,
    light_theme_color: Color,
    dark_theme_color: Color,
}

impl ColorPanel {
    pub fn new(light_theme_color: Color, dark_theme_color: Color) -> Self {
        Self {
            view: Default::default(),
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

    fn control_color_dialog(self: SharedView<Self>, hdc: Hdc, window: Window) -> Option<Hbrush> {
        let color = if Window::dark_mode_is_enabled() {
            self.dark_theme_color
        } else {
            self.light_theme_color
        };
        util::view::control_color_dialog_default(hdc, color)
    }
}

pub fn position_color_panel(
    panel: &SharedView<ColorPanel>,
    parent_window: Window,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    scaling: &DialogScaling,
) {
    if let Some(w) = panel.clone().open(parent_window) {
        w.set_everything_in_dialog_units(
            Point::new(DialogUnits(x), DialogUnits(y)).scale(scaling),
            Dimensions::new(DialogUnits(width), DialogUnits(height)).scale(scaling),
            ZOrder::Bottom,
        );
    }
}
