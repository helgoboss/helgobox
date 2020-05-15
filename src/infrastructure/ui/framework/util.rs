use crate::infrastructure::ui::framework::{DialogUnits, Pixels, Window};

/// A value used for calculating window size and spacing from dialog units.
///
/// Might have to be chosen a bit differently on each OS.
const UI_SCALE_FACTOR: f64 = 1.7;

/// Converts the given dialog dimensions to pixels.
///
/// On Windows the calculation is based on HiDPI settings. The given window must be a dialog window,
/// otherwise it returns the wrong value
///
/// On other systems the calculation just uses a constant factor.
pub fn convert_dialog_units_to_pixels(
    window: Window,
    (width, height): (DialogUnits, DialogUnits),
) -> (Pixels, Pixels) {
    #[cfg(target_family = "windows")]
    {
        use crate::infrastructure::common::bindings::root::*;
        let mut rect = tagRECT {
            left: 0,
            top: 0,
            right: width.get() as _,
            bottom: height.get() as _,
        };
        unsafe {
            MapDialogRect(window.get_hwnd() as _, &mut rect as _);
        }
        (Pixels(rect.right as u32), Pixels(rect.bottom as u32))
    }
    #[cfg(target_family = "unix")]
    (
        Pixels((UI_SCALE_FACTOR * width.get() as f64) as _),
        Pixels((UI_SCALE_FACTOR * height.get() as f64) as _),
    )
}
