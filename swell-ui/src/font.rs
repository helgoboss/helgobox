use crate::Color;
use base::hash_util::NonCryptoHashMap;
use reaper_low::{raw, Swell};
use reaper_medium::Hfont;
use std::cell::RefCell;

#[derive(Debug, Default)]
pub struct FontCache {
    fonts: RefCell<NonCryptoHashMap<FontDescriptor, Option<Font>>>,
}

impl FontCache {
    /// Returns a handle to a cached font according to the given descriptor.
    ///
    /// The returned handle is guaranteed to remain valid because we require a static self.
    pub fn get_font(&'static self, descriptor: FontDescriptor) -> Option<Hfont> {
        let mut fonts = self.fonts.borrow_mut();
        fonts
            .entry(descriptor)
            .or_insert_with(|| Font::from_descriptor(descriptor))
            .as_ref()
            // It's okay to do this here because we require self to be 'static
            .map(|font| font.to_inner())
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug)]
pub struct FontDescriptor {
    pub name: &'static str,
    pub size: u32,
}

impl FontDescriptor {
    pub const fn new(name: &'static str, size: u32) -> Self {
        Self { name, size }
    }
}

/// Owned font.
#[derive(Debug)]
pub struct Font(Hfont);

impl Font {
    pub fn from_descriptor(desc: FontDescriptor) -> Option<Self> {
        let mut font = raw::LOGFONT {
            lfHeight: desc.size as _,
            ..Default::default()
        };
        for (i, byte) in desc.name.bytes().take(31).enumerate() {
            font.lfFaceName[i] = byte as _;
        }
        let font = unsafe { Swell::get().CreateFontIndirect(&mut font as *mut _) };
        Hfont::new(font).map(Self)
    }

    pub fn to_inner(&self) -> Hfont {
        self.0
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            Swell::get().DeleteObject(self.0.as_ptr());
        }
    }
}
