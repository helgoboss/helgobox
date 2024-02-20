use reaper_common_types::RgbColor;
use reaper_low::Swell;

pub trait SwellRgbColorExt {
    /// Converts this color to a single integer as expected by Win32/SWELL.
    fn to_raw(&self) -> u32;
}

impl SwellRgbColorExt for RgbColor {
    fn to_raw(&self) -> u32 {
        Swell::RGB(self.r, self.g, self.b)
    }
}
