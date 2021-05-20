use crate::domain::{
    convert_count_to_step_size, convert_unit_value_to_preset_index, fx_preset_unit_value,
    ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};
use reaper_medium::FxPresetRef;

#[derive(Clone, Debug, PartialEq)]
pub struct FxPresetTarget {
    pub fx: Fx,
}

impl RealearnTarget for FxPresetTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        // `+ 1` because "<no preset>" is also a possible value.
        let preset_count = self.fx.preset_count().unwrap_or(0);
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(preset_count + 1),
            },
            TargetCharacter::Discrete,
        )
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text)
    }

    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text)
    }

    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
        let value = convert_unit_value_to_preset_index(&self.fx, input)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn format_value(&self, value: UnitValue) -> String {
        match convert_unit_value_to_preset_index(&self.fx, value) {
            None => "<No preset>".to_string(),
            Some(i) => (i + 1).to_string(),
        }
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        let preset_index = convert_unit_value_to_preset_index(&self.fx, value.as_unit_value()?);
        let preset_ref = match preset_index {
            None => FxPresetRef::FactoryPreset,
            Some(i) => FxPresetRef::Preset(i),
        };
        self.fx.activate_preset(preset_ref);
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
    ) -> (bool, Option<UnitValue>) {
        match evt {
            ChangeEvent::FxPresetChanged(e) if e.fx == self.fx => (true, None),
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(&self, value: u32) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        Ok(fx_preset_unit_value(&self.fx, index))
    }
}

impl<'a> Target<'a> for FxPresetTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        let value = fx_preset_unit_value(&self.fx, self.fx.preset_index().ok()?);
        Some(value)
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
