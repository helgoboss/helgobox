use reaper_medium::{Hbrush, Hdc};
use std::fmt::Debug;

use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util;
use crate::infrastructure::ui::util::colors::ColorPair;
use swell_ui::{
    DialogScaling, DialogUnits, Dimensions, Point, SharedView, View, ViewContext, Window, ZOrder,
};

/// A panel painted in a certain color and put below a specific section of the parent window.
///
/// We use the color panel on macOS and Linux only because Windows doesn't seem to like
/// overlapping child windows, it flickers like hell.
#[derive(Debug)]
pub struct ColorPanel {
    view: ViewContext,
    desc: ColorPanelDesc,
}

impl ColorPanel {
    pub fn new(desc: ColorPanelDesc) -> Self {
        Self {
            view: Default::default(),
            desc,
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

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.set_everything_in_dialog_units(
            Point::new(DialogUnits(self.desc.x), DialogUnits(self.desc.y))
                .scale(&self.desc.scaling),
            Dimensions::new(DialogUnits(self.desc.width), DialogUnits(self.desc.height))
                .scale(&self.desc.scaling),
            ZOrder::Bottom,
        );
        false
    }

    fn control_color_dialog(self: SharedView<Self>, _hdc: Hdc, _window: Window) -> Option<Hbrush> {
        util::view::get_brush(self.desc.color_pair)
    }
}

#[derive(Debug)]
pub struct ColorPanelDesc {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub color_pair: ColorPair,
    pub scaling: DialogScaling,
}
