use reaper_low::{raw, Swell};
use reaper_medium::Hbrush;
use std::fmt::Debug;
use std::ptr::null_mut;

use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util;
use crate::infrastructure::ui::util::colors::ColorPair;
use swell_ui::{
    DeviceContext, DialogScaling, DialogUnits, Dimensions, Point, SharedView, View, ViewContext,
    Window, ZOrder,
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

    /// Used on Windows only.
    pub fn paint_manually(&self, device_context: DeviceContext, window: Window) {
        let swell = Swell::get();
        let pos: Point<_> = window.convert_to_pixels(self.desc.scaled_position());
        let size: Dimensions<_> = window.convert_to_pixels(self.desc.scaled_size());
        let rc = raw::RECT {
            left: pos.x.get() as _,
            top: pos.y.get() as _,
            right: (pos.x + size.width).get() as _,
            bottom: (pos.y + size.height).get() as _,
        };
        let brush = util::view::get_brush_for_color_pair(self.desc.color_pair)
            .map(|b| b.as_ptr())
            .unwrap_or(null_mut());
        unsafe {
            swell.FillRect(device_context.as_ptr(), &rc, brush);
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
            self.desc.scaled_position(),
            self.desc.scaled_size(),
            ZOrder::Bottom,
        );
        false
    }

    fn control_color_dialog(
        self: SharedView<Self>,
        _device_context: DeviceContext,
        _window: Window,
    ) -> Option<Hbrush> {
        util::view::get_brush_for_color_pair(self.desc.color_pair)
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

impl ColorPanelDesc {
    pub fn scaled_position(&self) -> Point<DialogUnits> {
        Point::new(DialogUnits(self.x), DialogUnits(self.y)).scale(&self.scaling)
    }

    pub fn scaled_size(&self) -> Dimensions<DialogUnits> {
        Dimensions::new(DialogUnits(self.width), DialogUnits(self.height)).scale(&self.scaling)
    }
}
