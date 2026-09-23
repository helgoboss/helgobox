use crate::{Mouse, MouseCursorPosition, blocking_lock};
use anyhow::Context;
use device_query::DeviceState;
use enigo::{Coordinate, Direction, Enigo, Mouse as MouseEnigo, Settings};
use helgobox_api::persistence::{Axis, MouseButton};
use std::fmt::Debug;
use std::sync::{LazyLock, Mutex, MutexGuard};

static ENIGO: LazyLock<Result<Mutex<Enigo>, enigo::NewConError>> = LazyLock::new(|| {
    let enigo = Enigo::new(&Settings::default())?;
    Ok(Mutex::new(enigo))
});

#[derive(Clone, Debug)]
pub struct EnigoMouse {
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
            device_state: create_device_state(),
        }
    }

    fn enigo(&self) -> anyhow::Result<MutexGuard<'_, Enigo>> {
        let enigo = ENIGO.as_ref()?;
        Ok(blocking_lock(enigo, "enigo"))
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
            let Ok(enigo) = self.enigo() else {
                return 0;
            };
            let Ok((width, height)) = enigo.main_display() else {
                return 0;
            };
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

    fn cursor_position(&self) -> anyhow::Result<MouseCursorPosition> {
        let (x, y) = self.enigo()?.location()?;
        Ok(MouseCursorPosition::new(x.max(0) as u32, y.max(0) as u32))
    }

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> anyhow::Result<()> {
        self.enigo()?
            .move_mouse(new_pos.x as _, new_pos.y as _, Coordinate::Abs)?;
        Ok(())
    }

    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> anyhow::Result<()> {
        self.enigo()?
            .move_mouse(x_delta, y_delta, Coordinate::Rel)?;
        Ok(())
    }

    fn scroll(&mut self, axis: Axis, delta: i32) -> anyhow::Result<()> {
        let enigo_axis = match axis {
            Axis::X => enigo::Axis::Horizontal,
            Axis::Y => enigo::Axis::Vertical,
        };
        self.enigo()?.scroll(delta, enigo_axis)?;
        Ok(())
    }

    fn press(&mut self, button: MouseButton) -> anyhow::Result<()> {
        self.enigo()?
            .button(convert_button_to_enigo(button), Direction::Press)?;
        Ok(())
    }

    fn release(&mut self, button: MouseButton) -> anyhow::Result<()> {
        self.enigo()?
            .button(convert_button_to_enigo(button), Direction::Release)?;
        Ok(())
    }

    fn is_pressed(&self, button: MouseButton) -> anyhow::Result<bool> {
        let mouse_state = self
            .device_state
            .as_ref()
            .context("macOS accessibility permissions not granted")?
            .query_pointer();
        let button_index = convert_button_to_device_query(button);
        let pressed = mouse_state
            .button_pressed
            .get(button_index)
            .context("couldn't get button")?;
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

fn convert_button_to_enigo(button: MouseButton) -> enigo::Button {
    match button {
        MouseButton::Left => enigo::Button::Left,
        MouseButton::Middle => enigo::Button::Middle,
        MouseButton::Right => enigo::Button::Right,
    }
}
