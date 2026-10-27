use crate::domain::targets::playtime_timing_commons::{
    PlaytimeSlotSectionTargetBase, SectionVariable,
};
use crate::domain::{
    CompartmentKind, DEFAULT_TARGET, ExtendedProcessorContext, ReaperTarget, TargetResolveResult,
    TargetSection, TargetTypeDef, UnresolvedPlaytimeSlotSectionTargetBase,
    UnresolvedReaperTargetDef, VirtualPlaytimeSlot,
};

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotLengthTarget {
    pub base: UnresolvedPlaytimeSlotSectionTargetBase,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotLengthTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> TargetResolveResult {
        let target = PlaytimeSlotLengthTarget {
            base: PlaytimeSlotSectionTargetBase::new(
                &self.base,
                SectionVariable::Length,
                context,
                compartment,
            )?,
        };
        Ok(vec![ReaperTarget::PlaytimeSlotLength(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.base.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotLengthTarget {
    base: PlaytimeSlotSectionTargetBase,
}

pub const PLAYTIME_SLOT_LENGTH_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Slot length",
    short_name: "Playtime slot length",
    supports_clip_slot: true,
    supports_time_unit: true,
    supports_max_value_expression: true,
    supports_step_size_expression: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeSlotLengthTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeSlotLengthTarget {}
    impl<'a> Target<'a> for PlaytimeSlotLengthTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        CompoundChangeEvent, ControlContext, HitResponse, MappingControlContext,
        PlaytimeSlotLengthTarget, RealearnTarget, ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{
        AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue,
    };
    use playtime_api::persistence::SlotAddress;
    use std::borrow::Cow;

    impl RealearnTarget for PlaytimeSlotLengthTarget {
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

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeSlotLength)
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotLengthTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            self.base.current_value(context)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.base.control_type(context)
        }
    }
}
