use realearn_api::persistence::{Axis, MouseButton};

pub trait Mouse {
    fn axis_size(&self, axis: Axis) -> u32;

    fn cursor_position(&self) -> Result<MouseCursorPosition, &'static str>;

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> Result<(), &'static str>;

    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> Result<(), &'static str>;

    fn scroll(&mut self, axis: Axis, delta: i32) -> Result<(), &'static str>;

    fn press(&mut self, button: MouseButton) -> Result<(), &'static str>;

    fn release(&mut self, button: MouseButton) -> Result<(), &'static str>;

    fn is_pressed(&self, button: MouseButton) -> Result<bool, &'static str>;
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
