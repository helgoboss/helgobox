/// An abstract unit used for dialog dimensions, independent of HiDPI and stuff.
pub struct DialogUnits(pub u32);

impl DialogUnits {
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Pixels on a screen.
pub struct Pixels(pub u32);

impl Pixels {
    pub fn get(self) -> u32 {
        self.0
    }
}
