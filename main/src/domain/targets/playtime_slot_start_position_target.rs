use crate::domain::{
    CompartmentKind, DEFAULT_TARGET, ExpressionEvaluator, ExtendedProcessorContext, ReaperTarget,
    TargetResolveResult, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef,
    VirtualPlaytimeSlot,
};
use helgobox_api::persistence::TimeUnit;
use ordered_float::NotNan;

use playtime_api::persistence::SlotAddress;

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotStartPositionTarget {
    pub slot: VirtualPlaytimeSlot,
    pub time_unit: TimeUnit,
    pub max_value_expression_evaluator: ExpressionEvaluator,
    pub step_size_expression_evaluator: ExpressionEvaluator,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotStartPositionTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> TargetResolveResult {
        let target = PlaytimeSlotStartPositionTarget {
            slot_coordinates: self.slot.resolve(context, compartment)?,
            time_unit: self.time_unit,
            max_value: self.max_value_expression_evaluator.evaluate()?,
            step_size: self.step_size_expression_evaluator.evaluate()?,
        };
        Ok(vec![ReaperTarget::PlaytimeSlotStartPosition(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotStartPositionTarget {
    pub slot_coordinates: SlotAddress,
    pub time_unit: TimeUnit,
    pub max_value: NotNan<f64>,
    pub step_size: NotNan<f64>,
}

pub const PLAYTIME_SLOT_START_POSITION_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Slot start position",
    short_name: "Playtime start pos",
    supports_clip_slot: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeSlotStartPositionTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeSlotStartPositionTarget {}
    impl<'a> Target<'a> for PlaytimeSlotStartPositionTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::playtime_util::interpret_current_clip_slot_value;
    use crate::domain::ui_util::{
        db_unit_value, format_value_as_db, format_value_as_db_without_unit, parse_value_from_db,
        volume_unit_value,
    };
    use crate::domain::{
        Backbone, CompoundChangeEvent, ControlContext, HitResponse, MappingControlContext,
        PlaytimeSlotStartPositionTarget, RealearnTarget, ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{
        AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue,
    };
    use playtime_api::persistence::SlotAddress;
    use playtime_clip_engine::{
        base::ClipMatrixEvent,
        rt::{ClipChangeEvent, QualifiedClipChangeEvent},
    };
    use reaper_high::SliderVolume;
    use std::borrow::Cow;

    impl RealearnTarget for PlaytimeSlotStartPositionTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
        }

        fn clip_slot_address(&self) -> Option<SlotAddress> {
            Some(self.slot_coordinates)
        }

        fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
            parse_value_from_db(text)
        }

        fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_db_without_unit(value)
        }

        fn value_unit(&self, _: ControlContext) -> &'static str {
            "dB"
        }

        fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_db(value)
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            let volume =
                SliderVolume::try_from_normalized_slider_value(value.to_unit_value()?.get())
                    .unwrap_or_default();
            let db = volume.db();
            Backbone::get()
                .with_clip_matrix_mut(
                    context.control_context.instance(),
                    |matrix| -> anyhow::Result<HitResponse> {
                        matrix.set_slot_volume(self.slot_coordinates, db)?;
                        Ok(HitResponse::processed_with_effect())
                    },
                )
                .map_err(|_| "couldn't acquire matrix")?
                .map_err(|_| "couldn't carry out volume action")
        }

        fn is_available(&self, _: ControlContext) -> bool {
            // TODO-medium With clip targets we should check the control context (instance state) if
            //  slot filled.
            true
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            _: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                    QualifiedClipChangeEvent {
                        clip_address,
                        event: ClipChangeEvent::Volume(new_value),
                    },
                )) if clip_address.slot_address == self.slot_coordinates => (
                    true,
                    Some(AbsoluteValue::Continuous(db_unit_value(*new_value))),
                ),
                _ => (false, None),
            }
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            Some(self.volume(context)?.to_string().into())
        }

        fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
            Some(NumericValue::Decimal(self.volume(context)?.db().get()))
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            todo!()
            // Some(ReaperTargetType::PlaytimeSlotStartPosition)
        }
    }

    impl PlaytimeSlotStartPositionTarget {
        fn volume(&self, context: ControlContext) -> Option<SliderVolume> {
            Backbone::get()
                .with_clip_matrix(context.instance(), |matrix| {
                    let db = matrix.find_slot(self.slot_coordinates)?.volume().ok()?;
                    Some(SliderVolume::from_db(db))
                })
                .ok()?
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotStartPositionTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let val = self
                .volume(context)
                .map(volume_unit_value)
                .map(AbsoluteValue::Continuous);
            interpret_current_clip_slot_value(val)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }
}
