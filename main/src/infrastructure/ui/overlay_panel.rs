use crate::infrastructure::ui::bindings::root;
use reaper_high::Reaper;
use reaper_low::{raw, Swell};
use std::ptr::null_mut;
use swell_ui::{SharedView, SwellStringArg, View, ViewContext, Window};

#[derive(Debug)]
pub struct OverlayPanel {
    view: ViewContext,
}

impl OverlayPanel {
    pub fn new() -> OverlayPanel {
        OverlayPanel {
            view: Default::default(),
        }
    }
}

impl View for OverlayPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_OVERLAY
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn paint(self: SharedView<Self>) -> bool {
        unsafe fn variant_1(hwnd: raw::HWND) {
            let swell = Swell::get();
            let mut ps = raw::PAINTSTRUCT {
                hdc: null_mut(),
                fErase: 0,
                rcPaint: raw::RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
            };
            let hdc = swell.BeginPaint(hwnd, &mut ps as *mut _);
            paint_1(hdc, ps);
            swell.EndPaint(hwnd, &mut ps as *mut _);
        }
        unsafe fn variant_2(hwnd: raw::HWND) {
            let swell = Swell::get();
            let mut rect = raw::RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            // swell.GetClientRect(hwnd, &mut rect as *mut _);
            // swell.InvalidateRect(hwnd, &rect as *const _, 1);
            let hdc = swell.GetDC(hwnd);
            paint_2(hwnd, hdc);
            swell.ReleaseDC(hwnd, hdc);
        }
        unsafe {
            variant_1(self.view.require_window().raw());
            // variant_2(self.view.require_window().raw());
        }
        true
    }
}

unsafe fn paint_1(hdc: raw::HDC, mut ps: raw::PAINTSTRUCT) {
    let swell = Swell::get();
    swell.FillRect(hdc, &ps.rcPaint, (raw::COLOR_WINDOW + 1) as _);
    let text = SwellStringArg::from("hey");
    swell.DrawText(
        hdc,
        text.as_ptr(),
        -1,
        &mut ps.rcPaint as _,
        (raw::DT_CENTER | raw::DT_VCENTER) as _,
    );
}

unsafe fn paint_2(hwnd: raw::HWND, hdc: raw::HDC) {
    // TODO
    let swell = Swell::get();
    let reaper = Reaper::get().medium_reaper().low();
    let mut rect = raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    swell.GetClientRect(hwnd, &mut rect as *mut _);
    let width = rect.right;
    let height = rect.bottom;
    let bitmap = reaper.LICE_CreateBitmap(0, width, height);
    fn lice_rgba(r: u32, g: u32, b: u32, a: u32) -> u32 {
        (((b) & 0xff) | (((g) & 0xff) << 8) | (((r) & 0xff) << 16) | (((a) & 0xff) << 24))
    }
    // reaper.LICE_DrawRect(
    //     bitmap,
    //     0,
    //     0,
    //     width,
    //     height,
    //     lice_rgba(255, 0, 0, 255),
    //     1.0,
    //     0,
    // );
    let text = SwellStringArg::from("hey");
    reaper.LICE_DrawText(
        bitmap,
        0,
        0,
        text.as_ptr(),
        lice_rgba(255, 0, 0, 255),
        1.0,
        0,
    );
    let source_hdc = reaper.LICE__GetDC(bitmap);
    // Blit to window
    // let hdc = swell.GetDC(hwnd);
    // swell.StretchBlt(
    //     hdc,
    //     0,
    //     0,
    //     width,
    //     height,
    //     source_hdc,
    //     0,
    //     0,
    //     width,
    //     height,
    //     raw::SRCCOPY_USEALPHACHAN as _,
    // );
    swell.BitBlt(
        hdc,
        0,
        0,
        width,
        height,
        source_hdc,
        0,
        0,
        raw::SRCCOPY as _,
    );
    // swell.ReleaseDC(hwnd, hdc);
    reaper.LICE__Destroy(bitmap);
}

unsafe fn paint_3(window: Window) {
    let hwnd = window.raw();
    // TODO
    let swell = Swell::get();
    let mut rect = raw::RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    swell.GetClientRect(hwnd, &mut rect as *mut _);
    swell.InvalidateRect(hwnd, &rect as *const _, 1);
    let reaper = Reaper::get().medium_reaper().low();
    let width = 100;
    let height = 100;
    let bitmap = reaper.LICE_CreateBitmap(1, width, height);
    fn lice_rgba(r: u32, g: u32, b: u32, a: u32) -> u32 {
        (((b) & 0xff) | (((g) & 0xff) << 8) | (((r) & 0xff) << 16) | (((a) & 0xff) << 24))
    }
    reaper.LICE_DrawRect(
        bitmap,
        0,
        0,
        width,
        height,
        lice_rgba(100, 100, 100, 255),
        1.0,
        0,
    );
    let source_hdc = reaper.LICE__GetDC(bitmap);
    // Blit to window
    let hdc = swell.GetDC(hwnd);
    swell.StretchBlt(
        hdc,
        0,
        0,
        width,
        height,
        source_hdc,
        0,
        0,
        width,
        height,
        raw::SRCCOPY_USEALPHACHAN as _,
    );
    swell.ReleaseDC(hwnd, hdc);
    // reaper.LICE__Destroy(bitmap);
}
