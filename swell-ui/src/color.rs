use palette::rgb::Rgb;
use palette::{LinSrgb, Srgb};
use reaper_low::Swell;

/// Construct a color like this: `color!("EFEFEF")`
#[macro_export]
macro_rules! color {
    ($arr:literal) => {
        swell_ui::Color::from_array(swell_ui::hex!($arr))
    };
}

#[macro_export]
macro_rules! colors {
    (
        $(
            $name:ident = $arr:literal;
        )+
    ) => {
        $(
            pub const $name: swell_ui::Color = swell_ui::color!($arr);
        )+
    };
}

/// A color for being used in Win32/SWELL.
///
/// A non-linear sRGB color to be specific.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    /// Converts an arbitrary palette crate color to our color type.
    pub fn from_palette<S, T>(color: Rgb<S, T>) -> Self
    where
        Srgb<u8>: From<Rgb<S, T>>,
    {
        // We want non-linear sRGB
        let srgb: Srgb<u8> = color.into();
        Self::rgb(srgb.red, srgb.green, srgb.blue)
    }

    /// Creates this color by providing the non-linear sRGB components contained in an array.
    pub const fn from_array(rgb: [u8; 3]) -> Self {
        Self::rgb(rgb[0], rgb[1], rgb[2])
    }

    /// Creates this color by providing the non-linear sRGB components.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Converts this color to an arbitrary palette crate color type.
    pub fn to_palette<S, T>(&self) -> Rgb<S, T>
    where
        Rgb<S, T>: From<Srgb<u8>>,
    {
        let srgb = Srgb::new(self.r, self.g, self.b);
        srgb.into()
    }

    /// Convenience function to start working with the color.
    pub fn to_linear(&self) -> LinSrgb {
        self.to_palette()
    }

    /// Converts this color to a single integer as expected by Win32/SWELL.
    pub const fn to_raw(&self) -> u32 {
        Swell::RGB(self.r, self.g, self.b)
    }
}

impl<S, T> From<Rgb<S, T>> for Color
where
    Srgb<u8>: From<Rgb<S, T>>,
{
    fn from(value: Rgb<S, T>) -> Self {
        Color::from_palette(value)
    }
}

impl<S, T> From<Color> for Rgb<S, T>
where
    Rgb<S, T>: From<Srgb<u8>>,
{
    fn from(value: Color) -> Self {
        value.to_palette()
    }
}
