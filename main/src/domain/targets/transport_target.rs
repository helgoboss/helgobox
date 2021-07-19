use crate::domain::{
    format_value_as_on_off, transport_is_enabled_unit_value, AdditionalFeedbackEvent,
    ControlContext, RealearnTarget, TargetCharacter, TransportAction,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Reaper};

#[derive(Clone, Debug, PartialEq)]
pub struct TransportTarget {
    pub project: Project,
    pub action: TransportAction,
}

impl RealearnTarget for TransportTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        use TransportAction::*;
        match self.action {
            // Retriggerable because we want to be able to retrigger play!
            PlayStop | PlayPause => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Switch,
            ),
            Stop | Pause | Record | Repeat => {
                (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
            }
        }
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        use TransportAction::*;
        let on = !value.to_unit_value()?.is_zero();
        match self.action {
            PlayStop => {
                if on {
                    self.project.play();
                } else {
                    self.project.stop();
                }
            }
            PlayPause => {
                if on {
                    self.project.play();
                } else {
                    self.project.pause();
                }
            }
            Stop => {
                if on {
                    self.project.stop();
                }
            }
            Pause => {
                if on {
                    self.project.pause();
                }
            }
            Record => {
                if on {
                    Reaper::get().enable_record_in_current_project();
                } else {
                    Reaper::get().disable_record_in_current_project();
                }
            }
            Repeat => {
                if on {
                    self.project.enable_repeat();
                } else {
                    self.project.disable_repeat();
                }
            }
        };
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use ChangeEvent::*;
        use TransportAction::*;
        match self.action {
            PlayStop | PlayPause => match evt {
                PlayStateChanged(e) if e.project == self.project => (
                    true,
                    Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                        e.new_value.is_playing,
                    ))),
                ),
                _ => (false, None),
            },
            Stop => match evt {
                PlayStateChanged(e) if e.project == self.project => (
                    true,
                    Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                        !e.new_value.is_playing && !e.new_value.is_paused,
                    ))),
                ),
                _ => (false, None),
            },
            Pause => match evt {
                PlayStateChanged(e) if e.project == self.project => (
                    true,
                    Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                        e.new_value.is_paused,
                    ))),
                ),
                _ => (false, None),
            },
            Record => match evt {
                PlayStateChanged(e) if e.project == self.project => (
                    true,
                    Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                        e.new_value.is_recording,
                    ))),
                ),
                _ => (false, None),
            },
            Repeat => match evt {
                RepeatStateChanged(e) if e.project == self.project => (
                    true,
                    Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                        e.new_value,
                    ))),
                ),
                _ => (false, None),
            },
        }
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        if self.action == TransportAction::Repeat {
            return (false, None);
        }
        match evt {
            AdditionalFeedbackEvent::BeatChanged(e)
                if e.project == self.project && e.project != Reaper::get().current_project() =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TransportTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        use TransportAction::*;
        let play_state = self.project.play_state();
        let value = match self.action {
            PlayStop | PlayPause => transport_is_enabled_unit_value(play_state.is_playing),
            Stop => {
                transport_is_enabled_unit_value(!play_state.is_playing && !play_state.is_paused)
            }
            Pause => transport_is_enabled_unit_value(play_state.is_paused),
            Record => transport_is_enabled_unit_value(play_state.is_recording),
            Repeat => transport_is_enabled_unit_value(self.project.repeat_is_enabled()),
        };
        Some(AbsoluteValue::Continuous(value))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
