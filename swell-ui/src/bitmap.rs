use reaper_low::Swell;
use reaper_low::raw::HBITMAP;

pub struct NativeBitmap {
    pub width: u32,
    pub height: u32,
    pub raw: HBITMAP,
}

impl Drop for NativeBitmap {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                Swell::get().DeleteObject(self.raw.cast());
            }
        }
    }
}

impl std::fmt::Debug for NativeBitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitmapData")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
