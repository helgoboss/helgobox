use crate::domain::pot::{preset_db, with_preset_db, Preset, PresetId};
use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value_with_none,
    convert_unit_to_discrete_value_with_none, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, InstanceState, InstanceStateChanged,
    MappingControlContext, PotStateChangedEvent, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvededNavigateWithinPotPresetsTarget {}

impl UnresolvedReaperTargetDef for UnresolvededNavigateWithinPotPresetsTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::NavigateWithinPotPresets(
            NavigateWithinPotPresetsTarget {},
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigateWithinPotPresetsTarget {}

impl RealearnTarget for NavigateWithinPotPresetsTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: self.step_size(),
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
        _: ControlContext,
    ) -> Result<u32, &'static str> {
        let value = self
            .convert_unit_value_to_preset_index(value)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let preset_index = self.convert_unit_value_to_preset_index(value.to_unit_value()?);
        let preset_id = match preset_index {
            None => None,
            Some(i) => {
                let id = with_preset_db(|db| db.find_preset_id_at_index(i))?
                    .ok_or("no preset found for that index")?;
                Some(id)
            }
        };
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        instance_state.set_pot_preset_id(preset_id);
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        preset_db().is_ok()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetChanged { id },
            )) => (true, Some(self.convert_preset_id_to_absolute_value(*id))),
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        let uv = convert_discrete_to_unit_value_with_none(index, self.preset_count());
        Ok(uv)
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        let instance_state = context.instance_state.borrow();
        let preset_id = match self.current_preset_id(&instance_state) {
            None => return Some("<None>".into()),
            Some(id) => id,
        };
        let preset = match find_preset_by_id(preset_id) {
            None => return Some("<Not found>".into()),
            Some(p) => p,
        };
        Some(preset.name.into())
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let instance_state = context.instance_state.borrow();
        let preset_id = self.current_preset_id(&instance_state)?;
        let preset_index = self.find_index_of_preset(preset_id)?;
        Some(NumericValue::Discrete(preset_index as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::NavigateWithinPotPresets)
    }
}

impl<'a> Target<'a> for NavigateWithinPotPresetsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        let preset_id = self.current_preset_id(&instance_state);
        Some(self.convert_preset_id_to_absolute_value(preset_id))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl NavigateWithinPotPresetsTarget {
    fn convert_preset_id_to_absolute_value(&self, preset_id: Option<PresetId>) -> AbsoluteValue {
        let preset_index = preset_id.and_then(|id| self.find_index_of_preset(id));
        let actual = match preset_index {
            None => 0,
            Some(i) => i + 1,
        };
        let max = self.preset_count();
        AbsoluteValue::Discrete(Fraction::new(actual, max))
    }

    fn preset_count(&self) -> u32 {
        with_preset_db(|db| db.count_presets()).unwrap_or(0)
    }

    fn step_size(&self) -> UnitValue {
        // `+ 1` because "<None>" is also a possible value.
        let count = self.preset_count() + 1;
        convert_count_to_step_size(count)
    }

    fn convert_unit_value_to_preset_index(&self, value: UnitValue) -> Option<u32> {
        convert_unit_to_discrete_value_with_none(value, self.preset_count())
    }

    fn current_preset_id(&self, instance_state: &InstanceState) -> Option<PresetId> {
        instance_state.pot_state().preset_id()
    }

    fn find_index_of_preset(&self, id: PresetId) -> Option<u32> {
        with_preset_db(|db| db.find_index_of_preset(id))
            .ok()
            .flatten()
    }
}

pub const NAVIGATE_WITHIN_POT_PRESETS_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Pot: Navigate within presets",
    short_name: "Navigate within Pot presets",
    ..DEFAULT_TARGET
};

fn find_preset_by_id(id: PresetId) -> Option<Preset> {
    with_preset_db(|db| db.find_preset_by_id(id)).ok().flatten()
}
