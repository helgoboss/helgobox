use crate::domain::{Mouse, MouseCursorPosition};
use mouse_rs::types::keys::Keys;
use mouse_rs::Mouse as RawMouse;
use realearn_api::persistence::{Axis, MouseButton};
use reaper_low::{raw, Swell};
use std::fmt::{Debug, Formatter};

pub struct RsMouse(RawMouse);

impl Default for RsMouse {
    fn default() -> Self {
        Self(RawMouse::new())
    }
}

impl Clone for RsMouse {
    fn clone(&self) -> Self {
        Self(RawMouse::new())
    }
}

impl PartialEq for RsMouse {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for RsMouse {}

impl Debug for RsMouse {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_tuple("Mouse").finish()
    }
}

impl Mouse for RsMouse {
    fn axis_size(&self, axis: Axis) -> u32 {
        let index = match axis {
            Axis::X => raw::SM_CXSCREEN,
            Axis::Y => raw::SM_CYSCREEN,
        };
        Swell::get().GetSystemMetrics(index) as _
    }

    fn cursor_position(&self) -> Result<MouseCursorPosition, &'static str> {
        let point = self
            .0
            .get_position()
            .map_err(|_| "couldn't get mouse cursor position")?;
        Ok(MouseCursorPosition::new(
            point.x.max(0) as u32,
            point.y.max(0) as u32,
        ))
    }

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> Result<(), &'static str> {
        self.0
            .move_to(new_pos.x as _, new_pos.y as _)
            .map_err(|_| "couldn't move mouse cursor")
    }

    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> Result<(), &'static str> {
        let current_pos = self.cursor_position()?;
        let new_pos = MouseCursorPosition::new(
            (current_pos.x as i32 + x_delta).max(0) as _,
            (current_pos.y as i32 + y_delta).max(0) as _,
        );
        self.set_cursor_position(new_pos)
    }

    fn scroll(&mut self, delta: i32) -> Result<(), &'static str> {
        self.0
            .scroll(delta)
            .map_err(|_| "couldn't scroll mouse wheel")
    }

    fn press(&mut self, button: MouseButton) -> Result<(), &'static str> {
        self.0
            .press(&convert_button_to_key(button))
            .map_err(|_| "couldn't press mouse button")
    }

    fn release(&mut self, button: MouseButton) -> Result<(), &'static str> {
        self.0
            .release(&convert_button_to_key(button))
            .map_err(|_| "couldn't release mouse button")
    }

    fn is_pressed(&self, _button: MouseButton) -> Result<bool, &'static str> {
        Err("not supported")
    }
}

fn convert_button_to_key(button: MouseButton) -> Keys {
    match button {
        MouseButton::Left => Keys::LEFT,
        MouseButton::Middle => Keys::MIDDLE,
        MouseButton::Right => Keys::RIGHT,
    }
}
