use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    ActionInvocationType, AdditionalFeedbackEvent, ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Action, ActionCharacter, Project, Reaper};
use reaper_medium::CommandId;

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

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        match value {
            ControlValue::Absolute(v) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if !v.is_zero() {
                        self.action.invoke(v.get(), false, Some(self.project));
                    }
                }
                ActionInvocationType::Absolute => {
                    self.action.invoke(v.get(), false, Some(self.project))
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
        };
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.action.is_available()
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
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

    fn current_value(&self, _: ()) -> Option<UnitValue> {
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
        Some(val)
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
