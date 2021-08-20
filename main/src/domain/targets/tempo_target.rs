use crate::domain::{
    bpm_span, format_step_size_as_bpm_without_unit, format_value_as_bpm_without_unit,
    parse_step_size_from_bpm, parse_value_from_bpm, tempo_unit_value, ControlContext,
    HitInstructionReturnValue, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project};
use reaper_medium::UndoBehavior;

#[derive(Clone, Debug, PartialEq)]
pub struct TempoTarget {
    pub project: Project,
}

impl RealearnTarget for TempoTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRoundable {
                rounding_step_size: UnitValue::new(1.0 / bpm_span()),
            },
            TargetCharacter::Continuous,
        )
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_value_from_bpm(text)
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_step_size_from_bpm(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_value_as_bpm_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        format_step_size_as_bpm_without_unit(step_size)
    }

    fn hide_formatted_value(&self) -> bool {
        true
    }

    fn hide_formatted_step_size(&self) -> bool {
        true
    }

    fn value_unit(&self) -> &'static str {
        "bpm"
    }

    fn step_size_unit(&self) -> &'static str {
        "bpm"
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: ControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let tempo = reaper_high::Tempo::from_normalized_value(value.to_unit_value()?.get());
        self.project.set_tempo(tempo, UndoBehavior::OmitUndoPoint);
        Ok(None)
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
        match evt {
            ChangeEvent::MasterTempoChanged(e) if e.project == self.project => (
                true,
                Some(AbsoluteValue::Continuous(tempo_unit_value(
                    reaper_high::Tempo::from_bpm(e.new_value),
                ))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for TempoTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = tempo_unit_value(self.project.tempo());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
