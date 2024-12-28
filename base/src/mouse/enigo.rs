use crate::{Mouse, MouseCursorPosition};
use device_query::DeviceState;
use enigo::{Enigo, MouseControllable};
use helgobox_api::persistence::{Axis, MouseButton};
use std::fmt::Debug;

#[derive(Debug)]
pub struct EnigoMouse {
    enigo: Enigo,
    device_state: Option<DeviceState>,
}

impl Default for EnigoMouse {
    fn default() -> Self {
        Self::new()
    }
}

impl EnigoMouse {
    pub fn new() -> Self {
        Self {
            enigo: Default::default(),
            device_state: create_device_state(),
        }
    }
}

fn create_device_state() -> Option<DeviceState> {
    #[cfg(target_os = "macos")]
    {
        let trusted =
            macos_accessibility_client::accessibility::application_is_trusted_with_prompt();
        if trusted {
            Some(DeviceState::new())
        } else {
            reaper_high::Reaper::get().show_console_msg("This Helgobox feature only works if Helgobox can access the state of your mouse. For this, it needs macOS accessibility permissions. Please grant REAPER the accessibility permission in the macOS system settings and restart it!\n\n");
            None
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(DeviceState::new())
    }
}

unsafe impl Send for EnigoMouse {}

impl Clone for EnigoMouse {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl PartialEq for EnigoMouse {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for EnigoMouse {}

impl Mouse for EnigoMouse {
    fn axis_size(&self, axis: Axis) -> u32 {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let (width, height) = Enigo::main_display_size();
            let axis_size = match axis {
                Axis::X => width,
                Axis::Y => height,
            };
            axis_size as u32
        }
        #[cfg(target_os = "linux")]
        {
            let index = match axis {
                Axis::X => reaper_low::raw::SM_CXSCREEN,
                Axis::Y => reaper_low::raw::SM_CYSCREEN,
            };
            reaper_low::Swell::get().GetSystemMetrics(index) as _
        }
    }

    fn cursor_position(&self) -> Result<MouseCursorPosition, &'static str> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let (x, y) = Enigo::mouse_location();
        #[cfg(target_os = "linux")]
        let (x, y) = {
            let device_state = self
                .device_state
                .as_ref()
                .expect("DeviceState should always work on Linux")
                .query_pointer();
            (device_state.coords.0, device_state.coords.1)
        };
        Ok(MouseCursorPosition::new(x.max(0) as u32, y.max(0) as u32))
    }

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> Result<(), &'static str> {
        self.enigo.mouse_move_to(new_pos.x as _, new_pos.y as _);
        Ok(())
    }

    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> Result<(), &'static str> {
        self.enigo.mouse_move_relative(x_delta, y_delta);
        Ok(())
    }

    fn scroll(&mut self, axis: Axis, delta: i32) -> Result<(), &'static str> {
        match axis {
            Axis::X => self.enigo.mouse_scroll_x(delta),
            Axis::Y => {
                // Handle https://github.com/enigo-rs/enigo/issues/117
                let final_delta = if cfg!(windows) { delta } else { -delta };
                self.enigo.mouse_scroll_y(final_delta)
            }
        }
        Ok(())
    }

    fn press(&mut self, button: MouseButton) -> Result<(), &'static str> {
        self.enigo.mouse_down(convert_button_to_enigo(button));
        Ok(())
    }

    fn release(&mut self, button: MouseButton) -> Result<(), &'static str> {
        self.enigo.mouse_up(convert_button_to_enigo(button));
        Ok(())
    }

    fn is_pressed(&self, button: MouseButton) -> Result<bool, &'static str> {
        let mouse_state = self
            .device_state
            .as_ref()
            .ok_or("macOS accessibility permissions not granted")?
            .query_pointer();
        let button_index = convert_button_to_device_query(button);
        let pressed = mouse_state
            .button_pressed
            .get(button_index)
            .ok_or("couldn't get button")?;
        Ok(*pressed)
    }
}

fn convert_button_to_device_query(button: MouseButton) -> usize {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 3,
        MouseButton::Right => 2,
    }
}

fn convert_button_to_enigo(button: MouseButton) -> enigo::MouseButton {
    match button {
        MouseButton::Left => enigo::MouseButton::Left,
        MouseButton::Middle => enigo::MouseButton::Middle,
        MouseButton::Right => enigo::MouseButton::Right,
    }
}
