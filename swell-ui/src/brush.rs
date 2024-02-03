use base::hash_util::NonCryptoHashMap;
use palette::Srgb;
use reaper_low::raw::HBRUSH;
use reaper_low::Swell;
use reaper_medium::Hbrush;
use std::cell::{Ref, RefCell, RefMut};

#[derive(Debug, Default)]
pub struct BrushCache {
    brushes: RefCell<NonCryptoHashMap<BrushDescriptor, Option<Brush>>>,
}

impl BrushCache {
    /// Returns a handle to a cached brush according to the given descriptor.
    ///
    /// The returned handle is guaranteed to remain valid because we require a static self.
    pub fn get_brush(&'static self, descriptor: BrushDescriptor) -> Option<ValidBrushHandle> {
        let mut brushes = self.brushes.borrow_mut();
        brushes
            .entry(descriptor)
            .or_insert_with(|| Brush::from_descriptor(descriptor))
            .as_ref()
            .map(|brush| unsafe {
                // It's okay to do this here because we require self to be 'static
                ValidBrushHandle::new(brush.to_inner())
            })
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug)]
pub struct BrushDescriptor {
    color: BrushColor,
}

impl BrushDescriptor {
    pub const fn solid(color: Srgb<u8>) -> Self {
        Self {
            color: BrushColor {
                r: color.red,
                g: color.green,
                b: color.blue,
            },
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct BrushColor {
    r: u8,
    g: u8,
    b: u8,
}

/// Owned brush.
#[derive(Debug)]
pub struct Brush(Hbrush);

/// (Non-owned) handle to a brush that is guaranteed to be valid.
#[derive(Copy, Clone, Debug)]
pub struct ValidBrushHandle(Hbrush);

impl ValidBrushHandle {
    /// # Safety
    ///
    /// The caller must make sure that the given brush handle remains valid for the rest of
    /// lifetime of this program!
    pub unsafe fn new(raw: Hbrush) -> Self {
        Self(raw)
    }

    pub fn as_ptr(&self) -> HBRUSH {
        self.0.as_ptr()
    }
}

impl Brush {
    pub fn from_descriptor(desc: BrushDescriptor) -> Option<Self> {
        let swell_rgb = Swell::RGB(desc.color.r, desc.color.g, desc.color.b);
        let brush = Swell::get().CreateSolidBrush(swell_rgb as _);
        Hbrush::new(brush).map(Self)
    }

    pub fn to_inner(&self) -> Hbrush {
        self.0
    }
}

impl Drop for Brush {
    fn drop(&mut self) {
        unsafe {
            Swell::get().DeleteObject(self.0.as_ptr());
        }
    }
}
