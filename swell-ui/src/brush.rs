use crate::Color;
use base::hash_util::NonCryptoHashMap;
use reaper_low::Swell;
use reaper_medium::Hbrush;
use std::cell::RefCell;

#[derive(Debug, Default)]
pub struct BrushCache {
    brushes: RefCell<NonCryptoHashMap<BrushDescriptor, Option<Brush>>>,
}

impl BrushCache {
    /// Returns a handle to a cached brush according to the given descriptor.
    ///
    /// The returned handle is guaranteed to remain valid because we require a static self.
    pub fn get_brush(&'static self, descriptor: BrushDescriptor) -> Option<Hbrush> {
        let mut brushes = self.brushes.borrow_mut();
        brushes
            .entry(descriptor)
            .or_insert_with(|| Brush::from_descriptor(descriptor))
            .as_ref()
            // It's okay to do this here because we require self to be 'static
            .map(|brush| brush.to_inner())
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug)]
pub struct BrushDescriptor {
    color: Color,
}

impl BrushDescriptor {
    pub const fn solid(color: Color) -> Self {
        Self { color }
    }
}

/// Owned brush.
#[derive(Debug)]
pub struct Brush(Hbrush);

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
