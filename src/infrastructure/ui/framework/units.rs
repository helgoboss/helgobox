use crate::infrastructure::ui::framework::Window;

/// An abstract unit used for dialog dimensions, independent of HiDPI and stuff.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DialogUnits(pub u32);

impl DialogUnits {
    pub fn get(self) -> u32 {
        self.0
    }

    pub fn as_raw(self) -> i32 {
        self.0 as _
    }
}

/// Pixels on a screen.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Pixels(pub u32);

impl Pixels {
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Dimensions of a rectangle.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Dimensions<T> {
    pub width: T,
    pub height: T,
}

impl<T> Dimensions<T> {
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl Dimensions<Pixels> {
    pub fn to_vst(&self) -> (i32, i32) {
        (self.width.get() as _, self.height.get() as _)
    }
}

impl Dimensions<DialogUnits> {
    /// A value used for calculating window size and spacing from dialog units.
    ///
    /// Might have to be chosen a bit differently on each OS.
    const UI_SCALE_FACTOR: f64 = 1.7;

    /// Converts the given dialog dimensions to pixels.
    ///
    /// The Window struct contains a method which can do this including Windows HiDPI information.
    pub fn to_pixels(&self) -> Dimensions<Pixels> {
        // TODO On Windows this works differently. See original ReaLearn. But on the other hand
        //  ... this is only for the first short render before the optimal size is calculated.
        //  So as long as it works, this heuristic is okay.
        Dimensions {
            width: Pixels((Self::UI_SCALE_FACTOR * self.width.get() as f64) as _),
            height: Pixels((Self::UI_SCALE_FACTOR * self.height.get() as f64) as _),
        }
    }
}
