use crate::Color;
use reaper_low::{raw, Swell};
use reaper_medium::Hdc;

/// Represents a device context (HDC).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DeviceContext(Hdc);

impl DeviceContext {
    pub fn new(hdc: Hdc) -> DeviceContext {
        Self(hdc)
    }

    pub fn as_ptr(&self) -> raw::HDC {
        self.0.as_ptr()
    }

    pub fn set_bk_mode_to_transparent(&self) {
        unsafe {
            Swell::get().SetBkMode(self.as_ptr(), raw::TRANSPARENT as _);
        }
    }

    pub fn set_text_color(&self, color: Color) {
        unsafe {
            Swell::get().SetTextColor(self.as_ptr(), color.to_raw() as _);
        }
    }
}
