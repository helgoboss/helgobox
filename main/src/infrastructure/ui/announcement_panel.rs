use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::open_in_browser;
use anyhow::Context;
use base::Global;
use image::RgbaImage;
use reaper_common_types::RgbColor;
use reaper_low::{Swell, raw};
use reaper_medium::Hdc;
use std::cell::RefCell;
use std::rc::Rc;
use swell_ui::{
    BitmapFit, Brush, BrushDescriptor, DeviceContext, DialogUnits, NativeBitmap, Pixels, Point,
    Rect, SharedView, View, ViewContext, Window,
};

#[derive(Debug)]
pub struct AnnouncementPanel {
    view: ViewContext,
    cached_bitmap: RefCell<Option<Rc<NativeBitmap>>>,
}

impl AnnouncementPanel {
    pub fn new() -> AnnouncementPanel {
        AnnouncementPanel {
            view: Default::default(),
            cached_bitmap: RefCell::new(None),
        }
    }

    fn paint_internal(self: SharedView<Self>) -> Option<()> {
        let remote_config = BackboneShell::remote_config()?;
        let announcement = remote_config.remote_config.announcement.as_ref()?;
        let config = BackboneShell::get().config();
        let last_acknowledged_announcement_id = config.last_acknowledged_announcement_id();
        if let Some(last_acknowledged_announcement_id) = last_acknowledged_announcement_id
            && last_acknowledged_announcement_id == announcement.id
        {
            return None;
        }
        if !announcement.audience_matches() {
            return None;
        }
        let image = remote_config.announcement_image.as_ref()?;
        let [r, g, b, _] = image.get_pixel(0, 0).0;
        let bg_brush_desc = BrushDescriptor::solid(RgbColor::from_array([r, g, b]));
        let window = self.view.require_window();
        let bitmap = self.get_or_create_bitmap(image).ok()?;
        window
            .paint(|dc, client_rect| {
                let brush =
                    Brush::from_descriptor(bg_brush_desc).context("unable to create brush")?;
                window.draw_fill_rect(dc, client_rect, &brush);
                window.draw_image(dc, &bitmap, BitmapFit::Contain)?;
                draw_close_button(dc, &close_button_rect(window, client_rect));
                Ok(())
            })
            .ok()?;
        Some(())
    }

    fn get_or_create_bitmap(&self, image: &RgbaImage) -> anyhow::Result<Rc<NativeBitmap>> {
        let mut cache = self.cached_bitmap.borrow_mut();
        if let Some(bitmap) = cache.as_ref() {
            return Ok(Rc::clone(bitmap));
        }
        let bitmap = Rc::new(Window::create_bitmap(image.clone())?);
        *cache = Some(Rc::clone(&bitmap));
        Ok(bitmap)
    }

    fn handle_click(&self, location: Point<i32>) -> Option<()> {
        let remote_config = BackboneShell::remote_config()?;
        let announcement = remote_config.remote_config.announcement.as_ref()?;
        let window = self.view.require_window();
        let client_rect = window.client_rect();
        let rect = close_button_rect(window, &client_rect);
        let close_clicked = rect.contains(location);
        if close_clicked {
            BackboneShell::get().acknowledge_announcement(announcement.id.clone());
            window.hide();
            return Some(());
        }
        if let Some(url) = &announcement.target_url {
            open_in_browser(url);
            return Some(());
        }
        None
    }
}
impl View for AnnouncementPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_ANNOUNCEMENT_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        false
    }
    fn left_button_up(self: SharedView<Self>, location: Point<i32>) -> bool {
        self.handle_click(location).is_some()
    }

    fn set_cursor(self: SharedView<Self>) -> bool {
        Window::set_cursor(raw::IDC_HAND).is_ok()
    }

    fn erase_background(self: SharedView<Self>, device_context: DeviceContext) -> bool {
        // No background erasure necessary because we take care of it in the paint call
        // (or hide the window anyway).
        let _ = device_context;
        true
    }

    fn paint(self: SharedView<Self>) -> bool {
        if self.clone().paint_internal().is_some() {
            true
        } else {
            let _ = Global::task_support().do_in_main_thread_from_main_thread_asap(move || {
                if let Some(window) = self.view.window() {
                    window.hide();
                }
            });
            false
        }
    }
}
fn close_button_rect(window: Window, area: &Rect) -> Rect {
    let dimensions: Point<Pixels> = window.convert_to_pixels(Point {
        x: DialogUnits(14),
        y: DialogUnits(0),
    });
    let size = dimensions.x.as_raw();

    let dimensions: Point<Pixels> = window.convert_to_pixels(Point {
        x: DialogUnits(3),
        y: DialogUnits(0),
    });
    let margin = dimensions.x.as_raw();

    Rect::from_ltrb(
        area.right() - margin - size,
        area.top + margin,
        area.right() - margin,
        area.top + margin + size,
    )
}

fn draw_close_button(dc: Hdc, rect: &Rect) {
    let swell = Swell::get();
    let padding = 5;
    let left = rect.left + padding;
    let top = rect.top + padding;
    let right = rect.right() - padding;
    let bottom = rect.bottom() - padding;
    unsafe {
        let pen = swell.CreatePen(0, 2, 0x00FFFFFF);
        let old_pen = swell.SelectObject(dc.as_ptr(), pen.cast());
        swell.MoveToEx(dc.as_ptr(), left, top, std::ptr::null_mut());
        swell.LineTo(dc.as_ptr(), right, bottom);
        swell.MoveToEx(dc.as_ptr(), right, top, std::ptr::null_mut());
        swell.LineTo(dc.as_ptr(), left, bottom);
        swell.SelectObject(dc.as_ptr(), old_pen);
        swell.DeleteObject(pen.cast());
    }
}
