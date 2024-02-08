use std::ops::{Add, Mul, Sub};

/// An abstract unit used for dialog dimensions, independent of HiDPI and stuff.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct DialogUnits(pub u32);

impl DialogUnits {
    pub fn get(self) -> u32 {
        self.0
    }

    pub fn as_raw(self) -> i32 {
        self.0 as _
    }

    pub fn scale(&self, scale: f64) -> Self {
        DialogUnits((scale * self.0 as f64).round() as _)
    }
}

impl Add for DialogUnits {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Mul<u32> for DialogUnits {
    type Output = Self;

    fn mul(self, rhs: u32) -> Self::Output {
        Self(self.0 * rhs)
    }
}

/// Pixels on a screen.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Pixels(pub u32);

impl Pixels {
    pub fn get(self) -> u32 {
        self.0
    }

    pub fn as_raw(self) -> i32 {
        self.0 as _
    }

    pub fn scale(&self, scale: f64) -> Self {
        Pixels((scale * self.0 as f64).round() as _)
    }
}

impl Mul<f64> for Pixels {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self((self.0 as f64 * rhs).round() as _)
    }
}

impl Sub for Pixels {
    type Output = Pixels;

    fn sub(self, rhs: Self) -> Self::Output {
        Pixels(self.0 - rhs.0)
    }
}

impl Add for Pixels {
    type Output = Pixels;

    fn add(self, rhs: Self) -> Self::Output {
        Pixels(self.0 + rhs.0)
    }
}

/// Point in a coordinate system.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T> Point<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

/// These factors should correspond to those in `dialogs.cpp`.
fn effective_scale_factors() -> ScaleFactors {
    #[cfg(target_os = "linux")]
    {
        let scaling_256 = reaper_low::Swell::get().SWELL_GetScaling256();
        let hidpi_factor = scaling_256 as f64 / 256.0;
        ScaleFactors {
            main: 1.9 * hidpi_factor,
            y: 0.92,
        }
    }
    #[cfg(target_os = "macos")]
    {
        ScaleFactors { main: 1.6, y: 0.95 }
    }
    #[cfg(target_os = "windows")]
    {
        ScaleFactors { main: 1.0, y: 1.0 }
    }
}

struct ScaleFactors {
    /// The main scale factor which affects both x and y coordinates.
    ///
    /// Corresponds to `SWELL_DLG_SCALE_AUTOGEN` in `dialogs.cpp`.
    main: f64,
    /// An additional scale factor which is applied to y coordinates.
    ///
    /// Set to 1.0 if you want to use the main factor only.
    ///
    /// Corresponds to `SWELL_DLG_SCALE_AUTOGEN_YADJ` in `dialogs.cpp`.
    y: f64,
}

impl ScaleFactors {
    pub fn x_factor(&self) -> f64 {
        self.main
    }

    pub fn y_factor(&self) -> f64 {
        self.main * self.y
    }
}

impl Point<DialogUnits> {
    /// Converts this dialog unit point to pixels.
    ///
    /// The Window struct contains a method which can do this including Windows HiDPI information.
    pub fn in_pixels(&self) -> Point<Pixels> {
        // TODO-low On Windows this works differently. See original ReaLearn. But on the other hand
        //  ... this is only for the first short render before the optimal size is calculated.
        //  So as long as it works, this heuristic is okay.
        let scale_factors = effective_scale_factors();
        Point {
            x: Pixels((scale_factors.x_factor() * self.x.get() as f64) as _),
            y: Pixels((scale_factors.y_factor() * self.y.get() as f64) as _),
        }
    }

    pub fn scale(self, scaling: &DialogScaling) -> Self {
        Self {
            x: self.x.scale(scaling.x_scale),
            y: self.y.scale(scaling.y_scale),
        }
    }
}

impl<T: Copy> Point<T> {
    pub fn to_dimensions(self) -> Dimensions<T> {
        Dimensions::new(self.x, self.y)
    }
}

impl<T: Copy> From<Dimensions<T>> for Point<T> {
    fn from(d: Dimensions<T>) -> Self {
        d.to_point()
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

impl<T: Copy> Dimensions<T> {
    pub fn to_point(self) -> Point<T> {
        Point::new(self.width, self.height)
    }
}

impl Dimensions<Pixels> {
    pub fn to_vst(self) -> (i32, i32) {
        (self.width.get() as _, self.height.get() as _)
    }

    pub fn scale(self, scaling: DialogScaling) -> Self {
        Self {
            width: self.width.scale(scaling.width_scale),
            height: self.height.scale(scaling.height_scale),
        }
    }
}

impl Dimensions<DialogUnits> {
    /// Converts the given dialog unit dimensions to pixels.
    ///
    /// Doesn't take window-specific HIDPI info into account! Use `Window` for this.
    pub fn in_pixels(&self) -> Dimensions<Pixels> {
        self.to_point().in_pixels().to_dimensions()
    }

    pub fn scale(self, scaling: &DialogScaling) -> Self {
        Self {
            width: self.width.scale(scaling.width_scale),
            height: self.height.scale(scaling.height_scale),
        }
    }
}

impl<T: Copy> From<Point<T>> for Dimensions<T> {
    fn from(p: Point<T>) -> Self {
        p.to_dimensions()
    }
}

/// This is not the scaling applied by SWELL but the one applied before by us when generating
/// the RC file. In future we might produce different RC files for different operating systems.
/// Then this is maybe the only scaling info we need and we can ditch SWELL scaling.
#[derive(Debug)]
pub struct DialogScaling {
    pub x_scale: f64,
    pub y_scale: f64,
    pub width_scale: f64,
    pub height_scale: f64,
}
