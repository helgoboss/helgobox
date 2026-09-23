use crate::{Mouse, MouseCursorPosition, blocking_lock};
use enigo::{Coordinate, Direction, Enigo, Mouse as MouseEnigo, Settings};
use enum_map::EnumMap;
use helgobox_api::persistence::{Axis, MouseButton};
use std::fmt::Debug;
use std::sync::{LazyLock, Mutex, MutexGuard};

static ENIGO_MOUSE_STATE: LazyLock<Result<Mutex<EnigoMouseState>, enigo::NewConError>> =
    LazyLock::new(|| {
        let state = EnigoMouseState {
            enigo: Enigo::new(&Settings::default())?,
            mouse_button_states: Default::default(),
        };
        Ok(Mutex::new(state))
    });

fn enigo_mouse_state() -> anyhow::Result<MutexGuard<'static, EnigoMouseState>> {
    let enigo = ENIGO_MOUSE_STATE.as_ref()?;
    Ok(blocking_lock(enigo, "enigo"))
}

pub struct EnigoMouseState {
    enigo: Enigo,
    mouse_button_states: EnumMap<MouseButton, bool>,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct EnigoMouse;

impl EnigoMouseState {
    fn invoke_button(&mut self, button: MouseButton, direction: Direction) -> anyhow::Result<()> {
        self.enigo
            .button(convert_button_to_enigo(button), direction)?;
        self.mouse_button_states[button] = match direction {
            Direction::Press => true,
            Direction::Release => false,
            Direction::Click => false,
        };
        Ok(())
    }
}

impl Mouse for EnigoMouse {
    fn axis_size(&self, axis: Axis) -> u32 {
        enigo_mouse_state().map(|s| s.axis_size(axis)).unwrap_or(0)
    }

    fn cursor_position(&self) -> anyhow::Result<MouseCursorPosition> {
        enigo_mouse_state()?.cursor_position()
    }

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> anyhow::Result<()> {
        enigo_mouse_state()?.set_cursor_position(new_pos)
    }

    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> anyhow::Result<()> {
        enigo_mouse_state()?.adjust_cursor_position(x_delta, y_delta)
    }

    fn scroll(&mut self, axis: Axis, delta: i32) -> anyhow::Result<()> {
        enigo_mouse_state()?.scroll(axis, delta)
    }

    fn press(&mut self, button: MouseButton) -> anyhow::Result<()> {
        enigo_mouse_state()?.press(button)
    }

    fn release(&mut self, button: MouseButton) -> anyhow::Result<()> {
        enigo_mouse_state()?.release(button)
    }

    fn is_pressed(&self, button: MouseButton) -> anyhow::Result<bool> {
        enigo_mouse_state()?.is_pressed(button)
    }
}

impl Mouse for EnigoMouseState {
    fn axis_size(&self, axis: Axis) -> u32 {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let Ok((width, height)) = self.enigo.main_display() else {
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
        let (x, y) = self.enigo.location()?;
        Ok(MouseCursorPosition::new(x.max(0) as u32, y.max(0) as u32))
    }

    fn set_cursor_position(&mut self, new_pos: MouseCursorPosition) -> anyhow::Result<()> {
        self.enigo
            .move_mouse(new_pos.x as _, new_pos.y as _, Coordinate::Abs)?;
        Ok(())
    }

    fn adjust_cursor_position(&mut self, x_delta: i32, y_delta: i32) -> anyhow::Result<()> {
        self.enigo.move_mouse(x_delta, y_delta, Coordinate::Rel)?;
        Ok(())
    }

    fn scroll(&mut self, axis: Axis, delta: i32) -> anyhow::Result<()> {
        let enigo_axis = match axis {
            Axis::X => enigo::Axis::Horizontal,
            Axis::Y => enigo::Axis::Vertical,
        };
        self.enigo.scroll(delta, enigo_axis)?;
        Ok(())
    }

    fn press(&mut self, button: MouseButton) -> anyhow::Result<()> {
        self.invoke_button(button, Direction::Press)
    }

    fn release(&mut self, button: MouseButton) -> anyhow::Result<()> {
        self.invoke_button(button, Direction::Release)
    }

    fn is_pressed(&self, button: MouseButton) -> anyhow::Result<bool> {
        Ok(self.mouse_button_states[button])
    }
}

fn convert_button_to_enigo(button: MouseButton) -> enigo::Button {
    match button {
        MouseButton::Left => enigo::Button::Left,
        MouseButton::Middle => enigo::Button::Middle,
        MouseButton::Right => enigo::Button::Right,
    }
}
