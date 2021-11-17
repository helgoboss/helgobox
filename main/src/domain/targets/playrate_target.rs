use crate::domain::{
    format_step_size_as_playback_speed_factor_without_unit,
    format_value_as_playback_speed_factor_without_unit, parse_step_size_from_playback_speed_factor,
    parse_value_from_playback_speed_factor, playback_speed_factor_span, playrate_unit_value,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitInstructionReturnValue,
    MappingCompartment, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{ChangeEvent, PlayRate, Project};
use reaper_medium::NormalizedPlayRate;

#[derive(Debug)]
pub struct UnresolvedPlayrateTarget;

impl UnresolvedReaperTargetDef for UnresolvedPlayrateTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::Playrate(PlayrateTarget {
            project: context.context().project_or_current_project(),
        })])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayrateTarget {
    pub project: Project,
}

impl RealearnTarget for PlayrateTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRoundable {
                rounding_step_size: UnitValue::new(1.0 / (playback_speed_factor_span() * 100.0)),
            },
            TargetCharacter::Continuous,
        )
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_playback_speed_factor(text)
    }

    fn parse_as_step_size(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_step_size_from_playback_speed_factor(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_playback_speed_factor_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue, _: ControlContext) -> String {
        format_step_size_as_playback_speed_factor_without_unit(step_size)
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "x"
    }

    fn step_size_unit(&self, _: ControlContext) -> &'static str {
        "x"
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let play_rate =
            PlayRate::from_normalized_value(NormalizedPlayRate::new(value.to_unit_value()?.get()));
        self.project.set_play_rate(play_rate);
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
            CompoundChangeEvent::Reaper(ChangeEvent::MasterPlayrateChanged(e))
                if e.project == self.project =>
            {
                (
                    true,
                    Some(AbsoluteValue::Continuous(playrate_unit_value(
                        PlayRate::from_playback_speed_factor(e.new_value),
                    ))),
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(format!(
            "{:.2}",
            self.playrate().playback_speed_factor().get()
        ))
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(
            self.playrate().playback_speed_factor().get(),
        ))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Playrate)
    }
}

impl PlayrateTarget {
    fn playrate(&self) -> PlayRate {
        self.project.play_rate()
    }
}

impl<'a> Target<'a> for PlayrateTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = playrate_unit_value(self.playrate());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const PLAYRATE_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Project: Set playrate",
    short_name: "Playrate",
    ..DEFAULT_TARGET
};
