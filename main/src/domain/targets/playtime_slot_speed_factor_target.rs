use crate::domain::targets::playtime_factor_commons::{
    FactorVariable, PlaytimeSlotFactorTargetBase,
};
use crate::domain::{
    CompartmentKind, DEFAULT_TARGET, ExtendedProcessorContext, ReaperTarget, TargetResolveResult,
    TargetSection, TargetTypeDef, UnresolvedPlaytimeSlotFactorTargetBase,
    UnresolvedReaperTargetDef, VirtualPlaytimeSlot,
};

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotSpeedFactorTarget {
    pub base: UnresolvedPlaytimeSlotFactorTargetBase,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotSpeedFactorTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> TargetResolveResult {
        let target = PlaytimeSlotSpeedFactorTarget {
            base: PlaytimeSlotFactorTargetBase::new(
                &self.base,
                FactorVariable::SpeedFactor,
                context,
                compartment,
            )?,
        };
        Ok(vec![ReaperTarget::PlaytimeSlotSpeedFactor(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.base.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotSpeedFactorTarget {
    base: PlaytimeSlotFactorTargetBase,
}

pub const PLAYTIME_SLOT_SPEED_FACTOR_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Slot speed factor",
    short_name: "Playtime slot speed factor",
    supports_clip_slot: true,
    supports_max_value_expression: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeSlotSpeedFactorTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeSlotSpeedFactorTarget {}
    impl<'a> Target<'a> for PlaytimeSlotSpeedFactorTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        CompoundChangeEvent, ControlContext, HitResponse, MappingControlContext,
        PlaytimeSlotSpeedFactorTarget, RealearnTarget, ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{
        AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue,
    };
    use playtime_api::persistence::SlotAddress;
    use std::borrow::Cow;

    impl RealearnTarget for PlaytimeSlotSpeedFactorTarget {
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
            Some(ReaperTargetType::PlaytimeSlotSpeedFactor)
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotSpeedFactorTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            self.base.current_value(context)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.base.control_type(context)
        }
    }
}
