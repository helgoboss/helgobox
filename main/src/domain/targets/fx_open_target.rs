use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_value_as_on_off, ControlContext, FxDisplayType, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};
use reaper_medium::FxChainVisibility;

#[derive(Clone, Debug, PartialEq)]
pub struct FxOpenTarget {
    pub fx: Fx,
    pub display_type: FxDisplayType,
}

impl RealearnTarget for FxOpenTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        use FxDisplayType::*;
        if value.to_unit_value()?.is_zero() {
            match self.display_type {
                FloatingWindow => {
                    self.fx.hide_floating_window();
                }
                Chain => {
                    self.fx.chain().hide();
                }
            }
        } else {
            match self.display_type {
                FloatingWindow => {
                    self.fx.show_in_floating_window();
                }
                Chain => {
                    self.fx.show_in_chain();
                }
            }
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.fx.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.fx.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx.track()
    }

    fn fx(&self) -> Option<&Fx> {
        Some(&self.fx)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::FxOpened(e) if e.fx == self.fx => (true, None),
            ChangeEvent::FxClosed(e) if e.fx == self.fx => (true, None),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for FxOpenTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        use FxDisplayType::*;
        let is_open = match self.display_type {
            FloatingWindow => self.fx.floating_window().is_some(),
            Chain => {
                use FxChainVisibility::*;
                match self.fx.chain().visibility() {
                    Hidden | Visible(None) | Unknown(_) => false,
                    Visible(Some(i)) => self.fx.index() == i,
                }
            }
        };
        Some(AbsoluteValue::Continuous(convert_bool_to_unit_value(
            is_open,
        )))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
