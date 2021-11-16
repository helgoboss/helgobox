use crate::domain::{
    bpm_span, format_step_size_as_bpm_without_unit, format_value_as_bpm_without_unit,
    parse_step_size_from_bpm, parse_value_from_bpm, tempo_unit_value, CompoundChangeEvent,
    ControlContext, HitInstructionReturnValue, MappingControlContext, RealearnTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Tempo};
use reaper_medium::UndoBehavior;

#[derive(Clone, Debug, PartialEq)]
pub struct TempoTarget {
    pub project: Project,
}

impl RealearnTarget for TempoTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRoundable {
                rounding_step_size: UnitValue::new(1.0 / bpm_span()),
            },
            TargetCharacter::Continuous,
        )
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_bpm(text)
    }

    fn parse_as_step_size(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_step_size_from_bpm(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_bpm_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue, _: ControlContext) -> String {
        format_step_size_as_bpm_without_unit(step_size)
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "bpm"
    }

    fn step_size_unit(&self, _: ControlContext) -> &'static str {
        "bpm"
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let tempo = reaper_high::Tempo::from_normalized_value(value.to_unit_value()?.get());
        self.project.set_tempo(tempo, UndoBehavior::OmitUndoPoint);
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::MasterTempoChanged(e))
                if e.project == self.project =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(tempo_unit_value(
                        reaper_high::Tempo::from_bpm(e.new_value),
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(format!("{:.2} bpm", self.tempo().bpm().get()))
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.tempo().bpm().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Tempo)
    }
}

impl TempoTarget {
    fn tempo(&self) -> Tempo {
        self.project.tempo()
    }
}

impl<'a> Target<'a> for TempoTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = tempo_unit_value(self.tempo());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TEMPO_TARGET: TargetTypeDef = TargetTypeDef {
    short_name: "Tempo",
    ..DEFAULT_TARGET
};
