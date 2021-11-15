use crate::domain::ui_util::{
    format_as_double_percentage_without_unit, format_as_symmetric_percentage_without_unit,
    parse_from_double_percentage, parse_from_symmetric_percentage,
};
use crate::domain::{
    width_unit_value, CompoundChangeEvent, ControlContext, HitInstructionReturnValue,
    MappingControlContext, PanExt, RealearnTarget, ReaperTargetType, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{AvailablePanValue, ChangeEvent, Project, Track, Width};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackWidthTarget {
    pub track: Track,
}

impl RealearnTarget for TrackWidthTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_from_symmetric_percentage(text)
    }

    fn parse_as_step_size(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_from_double_percentage(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_as_symmetric_percentage_without_unit(value)
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue, _: ControlContext) -> String {
        format_as_double_percentage_without_unit(step_size)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.track.is_available()
    }

    fn hide_formatted_value(&self, _: ControlContext) -> bool {
        true
    }

    fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let width = Width::from_normalized_value(value.to_unit_value()?.get());
        self.track.set_width(width);
        Ok(None)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::TrackPanChanged(e))
                if e.track == self.track =>
            {
                (
                    true,
                    match e.new_value {
                        AvailablePanValue::Complete(v) => v.width().map(|width| {
                            AbsoluteValue::Continuous(width_unit_value(Width::from_reaper_value(
                                width,
                            )))
                        }),
                        AvailablePanValue::Incomplete(_) => None,
                    },
                )
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(format!("{:.2}", self.width().reaper_value().get()))
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.width().reaper_value().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackWidth)
    }
}

impl TrackWidthTarget {
    fn width(&self) -> Width {
        self.track.width()
    }
}

impl<'a> Target<'a> for TrackWidthTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = width_unit_value(self.width());
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}
