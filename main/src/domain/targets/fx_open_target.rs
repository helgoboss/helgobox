use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_value_as_on_off, CompoundChangeEvent, ControlContext, FxDisplayType,
    HitInstructionReturnValue, MappingControlContext, RealearnTarget, ReaperTargetType,
    TargetCharacter,
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
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
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
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
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
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use CompoundChangeEvent::*;
        match evt {
            Reaper(ChangeEvent::FxOpened(e)) if e.fx == self.fx => (true, None),
            Reaper(ChangeEvent::FxClosed(e)) if e.fx == self.fx => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxOpen)
    }
}

impl<'a> Target<'a> for FxOpenTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
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

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}
