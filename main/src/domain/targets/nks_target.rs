use crate::domain::nks::Sound;
use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value, convert_unit_to_discrete_value,
    nks::sound_db, nks::with_sound_db, AdditionalFeedbackEvent, BackboneState, Compartment,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, NksStateChangedEvent, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvededNksTarget {}

impl UnresolvedReaperTargetDef for UnresolvededNksTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::Nks(NksTarget {})])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NksTarget {}

impl RealearnTarget for NksTarget {
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
        Ok(self.convert_unit_value_to_sound_index(value))
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let sound_index = match value.to_absolute_value()? {
            AbsoluteValue::Continuous(v) => self.convert_unit_value_to_sound_index(v),
            AbsoluteValue::Discrete(f) => f.actual(),
        };
        self.set_sound_index(sound_index);
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        sound_db().is_ok()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::NksStateChanged(
                NksStateChangedEvent::SoundIndexChanged { index },
            )) => (true, Some(self.as_absolute_value(*index))),
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        Ok(convert_discrete_to_unit_value(value, self.sound_count()))
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        let sound = self.current_sound()?;
        Some(sound.name.into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let index = self.current_sound_index();
        Some(NumericValue::Discrete(index as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Nks)
    }
}

impl<'a> Target<'a> for NksTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let index = self.current_sound_index();
        Some(self.as_absolute_value(index))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl NksTarget {
    fn as_absolute_value(&self, index: u32) -> AbsoluteValue {
        let fraction = Fraction::new(index, self.sound_count().saturating_sub(1));
        AbsoluteValue::Discrete(fraction)
    }

    fn sound_count(&self) -> u32 {
        with_sound_db(|db| db.count_sounds()).unwrap_or(0)
    }

    fn step_size(&self) -> UnitValue {
        let count = self.sound_count();
        convert_count_to_step_size(count)
    }

    fn convert_unit_value_to_sound_index(&self, value: UnitValue) -> u32 {
        convert_unit_to_discrete_value(value, self.sound_count())
    }

    fn current_sound(&self) -> Option<Sound> {
        let index = self.current_sound_index();
        with_sound_db(|db| db.sound_by_index(index)).ok().flatten()
    }

    fn current_sound_index(&self) -> u32 {
        let target_state = BackboneState::target_state().borrow();
        let nks_state = target_state.nks_state();
        nks_state.sound_index()
    }

    fn set_sound_index(&self, index: u32) {
        let mut target_state = BackboneState::target_state().borrow_mut();
        target_state.set_sound_index(index);
    }
}

pub const NKS_TARGET: TargetTypeDef = TargetTypeDef {
    name: "FX: NKS",
    short_name: "NKS",
    ..DEFAULT_TARGET
};
