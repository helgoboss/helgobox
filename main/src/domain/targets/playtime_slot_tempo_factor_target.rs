use crate::domain::targets::playtime_factor_commons::{
    FactorVariable, PlaytimeSlotFactorTargetBase,
};
use crate::domain::{
    CompartmentKind, DEFAULT_TARGET, ExtendedProcessorContext, ReaperTarget, TargetResolveResult,
    TargetSection, TargetTypeDef, UnresolvedPlaytimeSlotFactorTargetBase,
    UnresolvedReaperTargetDef, VirtualPlaytimeSlot,
};

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotTempoFactorTarget {
    pub base: UnresolvedPlaytimeSlotFactorTargetBase,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotTempoFactorTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> TargetResolveResult {
        let target = PlaytimeSlotTempoFactorTarget {
            base: PlaytimeSlotFactorTargetBase::new(
                &self.base,
                FactorVariable::TempoFactor,
                context,
                compartment,
            )?,
        };
        Ok(vec![ReaperTarget::PlaytimeSlotTempoFactor(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.base.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotTempoFactorTarget {
    base: PlaytimeSlotFactorTargetBase,
}

pub const PLAYTIME_SLOT_TEMPO_FACTOR_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Slot tempo factor",
    short_name: "Playtime slot tempo factor",
    supports_clip_slot: true,
    supports_max_value_expression: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeSlotTempoFactorTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeSlotTempoFactorTarget {}
    impl<'a> Target<'a> for PlaytimeSlotTempoFactorTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        CompoundChangeEvent, ControlContext, HitResponse, MappingControlContext,
        PlaytimeSlotTempoFactorTarget, RealearnTarget, ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{
        AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue,
    };
    use playtime_api::persistence::SlotAddress;
    use std::borrow::Cow;

    impl RealearnTarget for PlaytimeSlotTempoFactorTarget {
        fn control_type_and_character(
            &self,
            context: ControlContext,
        ) -> (ControlType, TargetCharacter) {
            self.base.control_type_and_character(context)
        }

        fn clip_slot_address(&self) -> Option<SlotAddress> {
            self.base.clip_slot_address()
        }

        fn parse_as_value(
            &self,
            text: &str,
            context: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            self.base.parse_as_value(text, context)
        }

        fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
            self.base.format_value_without_unit(value, context)
        }

        fn value_unit(&self, context: ControlContext) -> &'static str {
            self.base.value_unit(context)
        }

        fn format_value(&self, value: UnitValue, context: ControlContext) -> String {
            self.base.format_value(value, context)
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            self.base.hit(value, context)
        }

        fn is_available(&self, context: ControlContext) -> bool {
            self.base.is_available(context)
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            context: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            self.base.process_change_event(evt, context)
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            self.base.text_value(context)
        }

        fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
            self.base.numeric_value(context)
        }

        fn parse_as_step_size(
            &self,
            text: &str,
            context: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            self.base.parse_as_step_size(text, context)
        }

        fn format_step_size_without_unit(
            &self,
            unit_value: UnitValue,
            context: ControlContext,
        ) -> String {
            self.base.format_step_size_without_unit(unit_value, context)
        }

        fn step_size_unit(&self, context: ControlContext) -> &'static str {
            self.base.step_size_unit(context)
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeSlotTempoFactor)
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotTempoFactorTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            self.base.current_value(context)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.base.control_type(context)
        }
    }
}
