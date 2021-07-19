use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    ActionInvocationType, AdditionalFeedbackEvent, ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};
use helgoboss_midi::U14;
use reaper_high::{Action, ActionCharacter, Project, Reaper};
use reaper_medium::{ActionValueChange, CommandId, WindowContext};
use std::convert::TryFrom;

#[derive(Clone, Debug, PartialEq)]
pub struct ActionTarget {
    pub action: Action,
    pub invocation_type: ActionInvocationType,
    pub project: Project,
}

impl RealearnTarget for ActionTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        match self.invocation_type {
            ActionInvocationType::Trigger => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            ActionInvocationType::Absolute => match self.action.character() {
                ActionCharacter::Toggle => {
                    (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
                }
                ActionCharacter::Trigger => {
                    (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
                }
            },
            ActionInvocationType::Relative => (ControlType::Relative, TargetCharacter::Discrete),
        }
    }

    fn open(&self) {
        // Just open action window
        Reaper::get()
            .main_section()
            .action_by_command_id(CommandId::new(40605))
            .invoke_as_trigger(Some(self.project));
    }

    fn format_value(&self, _: UnitValue) -> String {
        "".to_owned()
    }

    fn hit(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        match value {
            ControlValue::AbsoluteContinuous(v) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if !v.is_zero() {
                        self.invoke_with_unit_value(v);
                    }
                }
                ActionInvocationType::Absolute => {
                    self.invoke_with_unit_value(v);
                }
                ActionInvocationType::Relative => {
                    return Err("relative invocation type can't take absolute values");
                }
            },
            ControlValue::Relative(i) => {
                if let ActionInvocationType::Relative = self.invocation_type {
                    self.action.invoke(i.get() as f64, true, Some(self.project));
                } else {
                    return Err("relative values need relative invocation type");
                }
            }
            ControlValue::AbsoluteDiscrete(f) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if !f.is_zero() {
                        self.invoke_with_fraction(f)
                    }
                }
                ActionInvocationType::Absolute => self.invoke_with_fraction(f),
                ActionInvocationType::Relative => {
                    return Err("relative invocation type can't take absolute values");
                }
            },
        };
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.action.is_available()
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            // We can't provide a value from the event itself because the action hooks don't
            // pass values.
            AdditionalFeedbackEvent::ActionInvoked(e)
                if e.command_id == self.action.command_id() =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for ActionTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = if let Some(state) = self.action.is_on() {
            // Toggle action: Return toggle state as 0 or 1.
            convert_bool_to_unit_value(state)
        } else {
            // Non-toggle action. Try to return current absolute value if this is a
            // MIDI CC/mousewheel action.
            if let Some(value) = self.action.normalized_value() {
                UnitValue::new(value)
            } else {
                UnitValue::MIN
            }
        };
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

impl ActionTarget {
    fn invoke_with_fraction(&self, f: Fraction) {
        if let Ok(u14) = U14::try_from(f.actual()) {
            self.action.invoke_directly(
                ActionValueChange::AbsoluteHighRes(u14),
                WindowContext::Win(Reaper::get().main_window()),
                self.project.context(),
            );
        }
    }
}

impl ActionTarget {
    fn invoke_with_unit_value(&self, v: UnitValue) {
        self.action.invoke(v.get(), false, Some(self.project))
    }
}
