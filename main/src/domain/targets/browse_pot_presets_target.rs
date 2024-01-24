use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value_with_none,
    convert_unit_to_discrete_value_with_none, get_preset_property, CompartmentKind,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    InstanceStateChanged, MappingControlContext, PotStateChangedEvent, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use base::{blocking_lock, blocking_lock_arc};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, PropValue, Target, UnitValue,
};
use pot::{PotPreset, PresetId, RuntimePotUnit};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedBrowsePotPresetsTarget {}

impl UnresolvedReaperTargetDef for UnresolvedBrowsePotPresetsTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::BrowsePotPresets(
            BrowsePotPresetsTarget {},
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowsePotPresetsTarget {}

impl RealearnTarget for BrowsePotPresetsTarget {
    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        let mut instance_state = context.instance().borrow_mut();
        // `+ 1` because "<None>" is also a possible value.
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return (ControlType::AbsoluteContinuous, TargetCharacter::Continuous),
        };
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotFilterItemsTarget 9");
        let count = self.preset_count(&pot_unit) + 1;
        let atomic_step_size = convert_count_to_step_size(count);
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size,
                is_retriggerable: false,
            },
            TargetCharacter::Discrete,
        )
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        value: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        let mut instance_state = context.instance().borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotPresetsTarget 1");
        let value = self
            .convert_unit_value_to_preset_index(&pot_unit, value)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let mut instance_state = context.control_context.instance().borrow_mut();
        let shared_pot_unit = instance_state.pot_unit()?;
        let mut pot_unit =
            blocking_lock(&*shared_pot_unit, "PotUnit from BrowsePotPresetsTarget 2");
        let preset_index =
            self.convert_unit_value_to_preset_index(&pot_unit, value.to_unit_value()?);
        let preset_id = match preset_index {
            None => None,
            Some(i) => {
                let id = pot_unit
                    .find_preset_id_at_index(i)
                    .ok_or("no preset found for that index")?;
                Some(id)
            }
        };
        pot_unit.set_preset_id(preset_id);
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetChanged { id },
            )) => {
                let mut instance_state = context.instance().borrow_mut();
                let pot_unit = match instance_state.pot_unit() {
                    Ok(u) => u,
                    Err(_) => return (false, None),
                };
                let pot_unit =
                    blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotPresetsTarget 3");
                let value = self.convert_preset_id_to_absolute_value(&pot_unit, *id);
                (true, Some(value))
            }
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::IndexesRebuilt,
            )) => (true, None),
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        let mut instance_state = context.instance().borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotPresetsTarget 4");
        let uv = convert_discrete_to_unit_value_with_none(index, self.preset_count(&pot_unit));
        Ok(uv)
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        if let PropValue::Text(text) = self.prop_value("preset.name", context)? {
            Some(text)
        } else {
            None
        }
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let mut instance_state = context.instance().borrow_mut();
        let pot_unit = instance_state.pot_unit().ok()?;
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotPresetsTarget 5");
        let preset_index = pot_unit.find_index_of_preset(pot_unit.preset_id()?)?;
        Some(NumericValue::Discrete(preset_index as i32 + 1))
    }

    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        self.with_selected_preset(context, |p| get_preset_property(p?, key))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::BrowsePotPresets)
    }
}

impl<'a> Target<'a> for BrowsePotPresetsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let mut instance_state = context.instance().borrow_mut();
        let pot_unit = instance_state.pot_unit().ok()?;
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotPresetsTarget 6");
        let preset_id = pot_unit.preset_id();
        Some(self.convert_preset_id_to_absolute_value(&pot_unit, preset_id))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl BrowsePotPresetsTarget {
    fn convert_preset_id_to_absolute_value(
        &self,
        pot_unit: &RuntimePotUnit,
        preset_id: Option<PresetId>,
    ) -> AbsoluteValue {
        let preset_index = preset_id.and_then(|id| pot_unit.find_index_of_preset(id));
        let actual = match preset_index {
            None => 0,
            Some(i) => i + 1,
        };
        let max = self.preset_count(pot_unit);
        AbsoluteValue::Discrete(Fraction::new(actual, max))
    }

    fn preset_count(&self, pot_unit: &RuntimePotUnit) -> u32 {
        pot_unit.preset_count()
    }

    fn convert_unit_value_to_preset_index(
        &self,
        pot_unit: &RuntimePotUnit,
        value: UnitValue,
    ) -> Option<u32> {
        convert_unit_to_discrete_value_with_none(value, self.preset_count(pot_unit))
    }

    fn with_selected_preset<R>(
        &self,
        context: ControlContext,
        f: impl FnOnce(Option<&PotPreset>) -> R,
    ) -> R {
        let mut instance_state = context.instance().borrow_mut();
        if let Ok(pot_unit) = instance_state.pot_unit() {
            let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from BrowsePotPresetsTarget 4");
            let preset = pot_unit.find_currently_selected_preset();
            f(preset.as_ref())
        } else {
            f(None)
        }
    }
}

pub const BROWSE_POT_PRESETS_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Pot,
    name: "Browse presets",
    short_name: "Browse Pot presets",
    ..DEFAULT_TARGET
};
