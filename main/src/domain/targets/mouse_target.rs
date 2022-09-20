use crate::domain::enigo::EnigoMouse;
use crate::domain::mouse_rs::RsMouse;
use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    convert_count_to_step_size, Compartment, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, Mouse, MouseCursorPosition, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target};
use realearn_api::persistence::{Axis, MouseAction, MouseButton};
use reaper_low::{raw, Swell};
use std::fmt::Debug;

#[derive(Debug)]
pub struct UnresolvedMouseTarget {
    pub action: MouseAction,
}

impl UnresolvedReaperTargetDef for UnresolvedMouseTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::Mouse(EnigoMouseTarget {
            mouse: Default::default(),
            action: self.action,
        })])
    }
}

pub type RsMouseTarget = MouseTarget<RsMouse>;
pub type EnigoMouseTarget = MouseTarget<EnigoMouse>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MouseTarget<M> {
    mouse: M,
    action: MouseAction,
}

impl<M: Mouse> RealearnTarget for MouseTarget<M> {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        match self.action {
            MouseAction::Move { axis, .. } | MouseAction::Drag { axis, .. } => {
                let control_type = ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(self.axis_size(axis)),
                    is_retriggerable: false,
                };
                (control_type, TargetCharacter::Discrete)
            }
            MouseAction::Click { .. } => (ControlType::AbsoluteContinuous, TargetCharacter::Switch),
            MouseAction::Scroll => (ControlType::Relative, TargetCharacter::Discrete),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        match self.action {
            MouseAction::Move { axis } => self.move_cursor(value, axis),
            MouseAction::Drag { axis, button } => self.drag_cursor(value, axis, button),
            MouseAction::Click { button } => self.click_button(value, button),
            MouseAction::Scroll => self.scroll_wheel(value),
        }
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Mouse)
    }
}

impl<M: Mouse> MouseTarget<M> {
    fn cursor_position(&self) -> Result<MouseCursorPosition, &'static str> {
        self.mouse.cursor_position()
    }

    fn axis_size(&self, axis: Axis) -> u32 {
        let index = match axis {
            Axis::X => raw::SM_CXSCREEN,
            Axis::Y => raw::SM_CYSCREEN,
        };
        Swell::get().GetSystemMetrics(index) as _
    }

    fn drag_cursor(
        &mut self,
        value: ControlValue,
        axis: Axis,
        _button: MouseButton,
    ) -> Result<HitResponse, &'static str> {
        // TODO-high Drag
        self.move_cursor(value, axis)
    }

    fn move_cursor(
        &mut self,
        value: ControlValue,
        axis: Axis,
    ) -> Result<HitResponse, &'static str> {
        let current_pos = self.cursor_position()?;
        let current_pos_on_axis = get_pos_on_axis(current_pos, axis);
        let new_pos_on_axis = match value {
            // Move to pixel
            ControlValue::AbsoluteDiscrete(v) => v.actual() as i32,
            // Move by pixels
            ControlValue::RelativeDiscrete(v) => current_pos_on_axis as i32 + v.get(),
            // Move to percentage of canvas
            ControlValue::AbsoluteContinuous(v) => {
                let axis_size = self.axis_size(axis);
                let new_pos = v.get() * axis_size as f64;
                new_pos.round() as i32
            }
            // Move by percentage of canvas
            ControlValue::RelativeContinuous(v) => {
                let axis_size = self.axis_size(axis);
                let amount = v.get() * axis_size as f64;
                let new_pos = current_pos_on_axis as f64 + amount;
                new_pos.round() as i32
            }
        };
        let new_pos_on_axis = new_pos_on_axis.max(0) as u32;
        let new_pos = match axis {
            Axis::X => MouseCursorPosition::new(new_pos_on_axis, current_pos.y),
            Axis::Y => MouseCursorPosition::new(current_pos.x, new_pos_on_axis),
        };
        self.mouse
            .set_cursor_position(new_pos)
            .map_err(|_| "couldn't move cursor")?;
        Ok(HitResponse::processed_with_effect())
    }

    fn scroll_wheel(&mut self, value: ControlValue) -> Result<HitResponse, &'static str> {
        let delta = match value {
            ControlValue::RelativeContinuous(v) => v.to_discrete_increment().get(),
            ControlValue::RelativeDiscrete(v) => v.get(),
            _ => return Err("needs to be controlled relatively"),
        };
        self.mouse.scroll(delta)?;
        Ok(HitResponse::processed_with_effect())
    }

    fn click_button(
        &mut self,
        value: ControlValue,
        button: MouseButton,
    ) -> Result<HitResponse, &'static str> {
        if value.is_on() {
            self.mouse.press(button)?;
        } else {
            self.mouse.release(button)?;
        }
        Ok(HitResponse::processed_with_effect())
    }
}

impl<'a, M: Mouse> Target<'a> for MouseTarget<M> {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        match self.action {
            MouseAction::Move { axis } | MouseAction::Drag { axis, .. } => {
                let axis_size = self.axis_size(axis);
                let pos = self.cursor_position().ok()?;
                let pos_on_axis = get_pos_on_axis(pos, axis);
                let fraction = Fraction::new(pos_on_axis, axis_size);
                Some(AbsoluteValue::Discrete(fraction))
            }
            MouseAction::Click { button } => {
                let is_pressed = self.mouse.is_pressed(button).ok()?;
                Some(AbsoluteValue::Continuous(convert_bool_to_unit_value(
                    is_pressed,
                )))
            }
            MouseAction::Scroll => None,
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
    name: "Global: Mouse",
    short_name: "Mouse",
    ..DEFAULT_TARGET
};
