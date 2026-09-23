use helgobox_api::persistence::{Axis, MouseButton};

pub trait Mouse {
    fn axis_size(&self, axis: Axis) -> u32;

    fn cursor_position(&self) -> anyhow::Result<MouseCursorPosition>;

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> anyhow::Result<()>;

    /// Moves the mouse cursor relatively to its current position.
    ///
    /// - On the x axis, positive delta scrolls right and negative left.
    /// - On the y axis, positive delta scrolls down and negative up (because it's natural for
    ///   screens to consider the top-left as zero).
    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> anyhow::Result<()>;

    /// Invokes the scroll wheel.
    ///
    /// - On the x axis, positive delta scrolls right and negative left.
    /// - On the y axis, positive delta scrolls up and negative down (because it's natural for
    ///   knobs and especially faders to increase when scrolling up).
    fn scroll(&mut self, axis: Axis, delta: i32) -> anyhow::Result<()>;

    fn press(&mut self, button: MouseButton) -> anyhow::Result<()>;

    fn release(&mut self, button: MouseButton) -> anyhow::Result<()>;

    fn is_pressed(&self, button: MouseButton) -> anyhow::Result<bool>;
}

#[derive(Copy, Clone, Debug)]
pub struct MouseCursorPosition {
    pub x: u32,
    pub y: u32,
}

impl MouseCursorPosition {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}
