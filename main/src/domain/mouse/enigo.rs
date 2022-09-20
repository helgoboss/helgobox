use crate::domain::{Mouse, MouseCursorPosition};
use enigo::{Enigo, MouseControllable};
use realearn_api::persistence::MouseButton;
use std::fmt::Debug;

#[derive(Debug, Default)]
pub struct EnigoMouse(Enigo);

impl Clone for EnigoMouse {
    fn clone(&self) -> Self {
        Self(Enigo::default())
    }
}

impl PartialEq for EnigoMouse {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for EnigoMouse {}

impl Mouse for EnigoMouse {
    fn cursor_position(&self) -> Result<MouseCursorPosition, &'static str> {
        let (x, y) = Enigo::mouse_location();
        Ok(MouseCursorPosition::new(x.max(0) as u32, y.max(0) as u32))
    }

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> Result<(), &'static str> {
        self.0.mouse_move_to(new_pos.x as _, new_pos.y as _);
        Ok(())
    }

    fn scroll(&mut self, delta: i32) -> Result<(), &'static str> {
        self.0.mouse_scroll_y(delta);
        Ok(())
    }

    fn press(&mut self, button: MouseButton) -> Result<(), &'static str> {
        self.0.mouse_down(convert_button(button));
        Ok(())
    }

    fn release(&mut self, button: MouseButton) -> Result<(), &'static str> {
        self.0.mouse_up(convert_button(button));
        Ok(())
    }

    fn is_pressed(&self, _button: MouseButton) -> Result<bool, &'static str> {
        Err("not supported")
    }
}

fn convert_button(button: MouseButton) -> enigo::MouseButton {
    match button {
        MouseButton::Left => enigo::MouseButton::Left,
        MouseButton::Middle => enigo::MouseButton::Middle,
        MouseButton::Right => enigo::MouseButton::Right,
    }
}
