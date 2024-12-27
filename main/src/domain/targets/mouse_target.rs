use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    convert_count_to_step_size, CompartmentKind, ControlContext, ExtendedProcessorContext,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use base::enigo::EnigoMouse;
use base::{Mouse, MouseCursorPosition};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target};
use helgobox_api::persistence::{Axis, MouseButton};
use std::fmt::Debug;
use strum::EnumIter;

#[derive(Debug)]
pub struct UnresolvedMouseTarget {
    pub action_type: MouseActionType,
    pub axis: Axis,
    pub button: MouseButton,
}

impl UnresolvedReaperTargetDef for UnresolvedMouseTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::Mouse(EnigoMouseTarget {
            mouse: EnigoMouse::new(),
            action_type: self.action_type,
            axis: self.axis,
            button: self.button,
        })])
    }
}

pub type EnigoMouseTarget = MouseTarget<EnigoMouse>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MouseTarget<M> {
    mouse: M,
    action_type: MouseActionType,
    axis: Axis,
    button: MouseButton,
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    derive_more::Display,
    EnumIter,
    num_enum::TryFromPrimitive,
    num_enum::IntoPrimitive,
)]
#[repr(usize)]
pub enum MouseActionType {
    #[display(fmt = "Move cursor to")]
    MoveTo,
    #[display(fmt = "Move cursor by")]
    MoveBy,
    #[display(fmt = "Press or release button")]
    PressOrRelease,
    #[display(fmt = "Turn scroll wheel")]
    Scroll,
}

impl Default for MouseActionType {
    fn default() -> Self {
        Self::MoveTo
    }
}

impl<M: Mouse> RealearnTarget for MouseTarget<M> {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        use MouseActionType::*;
        match self.action_type {
            MoveTo => {
                let control_type = ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(self.axis_size()),
                    is_retriggerable: false,
                };
                (control_type, TargetCharacter::Discrete)
            }
            MoveBy => (ControlType::Relative, TargetCharacter::Discrete),
            PressOrRelease => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Switch,
            ),
            Scroll => (ControlType::Relative, TargetCharacter::Discrete),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        use MouseActionType::*;
        match self.action_type {
            MoveTo | MoveBy => self.move_cursor(value),
            PressOrRelease => self.click_button(value),
            Scroll => self.scroll_wheel(value),
        }
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Mouse)
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }

    fn can_report_current_value(&self) -> bool {
        use MouseActionType::*;
        match self.action_type {
            MoveTo | MoveBy => self.mouse.cursor_position().is_ok(),
            PressOrRelease => self.mouse.is_pressed(self.button).is_ok(),
            Scroll => false,
        }
    }
}

impl<M: Mouse> MouseTarget<M> {
    fn axis_size(&self) -> u32 {
        self.mouse.axis_size(self.axis)
    }

    fn move_cursor(&mut self, value: ControlValue) -> Result<HitResponse, &'static str> {
        let instruction = match value {
            // Move to pixel
            ControlValue::AbsoluteDiscrete(v) => MoveCursorInstruction::To(v.actual()),
            // Move by pixels
            ControlValue::RelativeDiscrete(v) => MoveCursorInstruction::By(v.get()),
            // Move to percentage of canvas
            ControlValue::AbsoluteContinuous(v) => {
                let axis_size = self.axis_size();
                let new_pos = v.get() * axis_size as f64;
                MoveCursorInstruction::To(new_pos.round() as u32)
            }
            // Move by percentage of canvas
            ControlValue::RelativeContinuous(v) => {
                let axis_size = self.axis_size();
                let amount = v.get() * axis_size as f64;
                MoveCursorInstruction::By(amount.round() as i32)
            }
        };
        match instruction {
            MoveCursorInstruction::To(pos) => {
                let current_pos = self.mouse.cursor_position()?;
                let new_pos = match self.axis {
                    Axis::X => MouseCursorPosition::new(pos, current_pos.y),
                    Axis::Y => MouseCursorPosition::new(current_pos.x, pos),
                };
                self.mouse.set_cursor_position(new_pos)?;
            }
            MoveCursorInstruction::By(delta) => {
                let (x_delta, y_delta) = match self.axis {
                    Axis::X => (delta, 0),
                    Axis::Y => (0, delta),
                };
                self.mouse.adjust_cursor_position(x_delta, y_delta)?;
            }
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn scroll_wheel(&mut self, value: ControlValue) -> Result<HitResponse, &'static str> {
        let delta = match value {
            ControlValue::RelativeContinuous(v) => v.to_discrete_increment().get(),
            ControlValue::RelativeDiscrete(v) => v.get(),
            _ => return Err("needs to be controlled relatively"),
        };
        self.mouse.scroll(self.axis, delta)?;
        Ok(HitResponse::processed_with_effect())
    }

    fn click_button(&mut self, value: ControlValue) -> Result<HitResponse, &'static str> {
        if value.is_on() {
            self.mouse.press(self.button)?;
        } else {
            self.mouse.release(self.button)?;
        }
        Ok(HitResponse::processed_with_effect())
    }
}

enum MoveCursorInstruction {
    To(u32),
    By(i32),
}

impl<'a, M: Mouse> Target<'a> for MouseTarget<M> {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        use MouseActionType::*;
        match self.action_type {
            MoveTo | MoveBy => {
                let axis_size = self.axis_size();
                let pos = self.mouse.cursor_position().ok()?;
                let pos_on_axis = get_pos_on_axis(pos, self.axis);
                let fraction = Fraction::new(pos_on_axis, axis_size);
                Some(AbsoluteValue::Discrete(fraction))
            }
            PressOrRelease => {
                let is_pressed = self.mouse.is_pressed(self.button).ok()?;
                Some(AbsoluteValue::Continuous(convert_bool_to_unit_value(
                    is_pressed,
                )))
            }
            Scroll => None,
        }
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

fn get_pos_on_axis(pos: MouseCursorPosition, axis: Axis) -> u32 {
    match axis {
        Axis::X => pos.x,
        Axis::Y => pos.y,
    }
}

pub const MOUSE_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Global,
    name: "Mouse",
    short_name: "Mouse",
    #[cfg(target_os = "macos")]
    hint: "Needs macOS accessibility permissions",
    supports_axis: true,
    supports_mouse_button: true,
    ..DEFAULT_TARGET
};
