use crate::domain::{
    CompartmentKind, DEFAULT_TARGET, ExpressionEvaluator, ExtendedProcessorContext, ReaperTarget,
    TargetResolveResult, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef,
    VirtualPlaytimeSlot,
};
use typed_floats::Positive;

use playtime_api::persistence::SlotAddress;

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotPitchShiftTarget {
    pub slot: VirtualPlaytimeSlot,
    pub max_value_expression_evaluator: Option<ExpressionEvaluator>,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotPitchShiftTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> TargetResolveResult {
        let target = PlaytimeSlotPitchShiftTarget {
            slot_coordinates: self.slot.resolve(context, compartment)?,
            max_value: self
                .max_value_expression_evaluator
                .as_ref()
                .and_then(|e| e.evaluate().ok())
                .and_then(|v| v.try_into().ok()),
        };
        Ok(vec![ReaperTarget::PlaytimeSlotPitchShift(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotPitchShiftTarget {
    pub slot_coordinates: SlotAddress,
    /// Max positive value in semitones.
    pub max_value: Option<Positive<f64>>,
}

pub const PLAYTIME_SLOT_PITCH_SHIFT_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Slot pitch shift",
    short_name: "Playtime slot pitch shift",
    supports_clip_slot: true,
    supports_max_value_expression: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{ControlContext, PlaytimeSlotPitchShiftTarget, RealearnTarget};
    use helgoboss_learn::Target;

    impl RealearnTarget for PlaytimeSlotPitchShiftTarget {}
    impl<'a> Target<'a> for PlaytimeSlotPitchShiftTarget {
        type Context = ControlContext<'a>;
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::playtime_util::interpret_current_clip_slot_value;
    use crate::domain::targets::playtime_common::{ClipTimingInfo, SectionVariable, TimeUnitValue};
    use crate::domain::ui_util::{
        db_unit_value, format_value_as_db, format_value_as_db_without_unit, parse_value_from_db,
    };
    use crate::domain::{
        Backbone, CompoundChangeEvent, ControlContext, HitResponse, MappingControlContext,
        PlaytimeSlotPitchShiftTarget, RealearnTarget, ReaperTargetType, TargetCharacter,
        format_step_size_as_bpm_without_unit, format_value_as_bpm_without_unit,
        parse_step_size_from_bpm,
    };
    use helgoboss_learn::{
        AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue,
    };
    use playtime_api::persistence::SlotAddress;
    use playtime_clip_engine::{
        base::ClipMatrixEvent,
        rt::{ClipChangeEvent, QualifiedClipChangeEvent},
    };
    use reaper_common_types::Semitones;
    use std::borrow::Cow;
    use typed_floats::Positive;

    impl RealearnTarget for PlaytimeSlotPitchShiftTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            let max_value = self.get_effective_max_value();
            let raw_step_size = 1.0 / (max_value.get() * 2.0);
            (
                ControlType::AbsoluteContinuousRoundable {
                    rounding_step_size: UnitValue::new_clamped(raw_step_size),
                },
                TargetCharacter::Continuous,
            )
        }

        fn clip_slot_address(&self) -> Option<SlotAddress> {
            Some(self.slot_coordinates)
        }

        fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
            let semitones: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
            Ok(self.convert_semitones_to_unit_value(semitones))
        }

        fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
            let semitones = self.convert_unit_value_to_semitones(value);
            format!("{:.2}", semitones)
        }

        fn value_unit(&self, _: ControlContext) -> &'static str {
            "st"
        }
        fn parse_as_step_size(
            &self,
            text: &str,
            _: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            let semitones: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
            let max_value = self.get_effective_max_value();
            let raw_unit_value = semitones / (2.0 * max_value.get());
            Ok(UnitValue::new_clamped(raw_unit_value))
        }

        fn format_step_size_without_unit(
            &self,
            unit_value: UnitValue,
            _: ControlContext,
        ) -> String {
            let max_value = self.get_effective_max_value();
            let semitones = unit_value.get() * max_value.get() * 2.0;
            format!("{semitones:.2}")
        }

        // fn hide_formatted_value(&self, _: ControlContext) -> bool {
        //     true
        // }
        //
        // fn hide_formatted_step_size(&self, _: ControlContext) -> bool {
        //     true
        // }
        //
        fn step_size_unit(&self, _: ControlContext) -> &'static str {
            "st"
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            let unit_value = value.to_unit_value()?;
            let semitones = self.convert_unit_value_to_semitones(unit_value);
            Backbone::get()
                .with_clip_matrix_mut(
                    context.control_context.instance(),
                    |matrix| -> anyhow::Result<HitResponse> {
                        matrix.set_slot_pitch_shift(self.slot_coordinates, semitones)?;
                        Ok(HitResponse::processed_with_effect())
                    },
                )
                .map_err(|_| "couldn't acquire matrix")?
                .map_err(|_| "couldn't set pitch shift")
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
                        event: ClipChangeEvent::Everything | ClipChangeEvent::PitchShift(_),
                    },
                )) if clip_address.slot_address == self.slot_coordinates => (true, None),
                _ => (false, None),
            }
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            Some(self.get_pitch_shift(context).ok()?.to_string().into())
        }

        fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
            Some(NumericValue::Decimal(
                self.get_pitch_shift(context).ok()?.get(),
            ))
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeSlotPitchShift)
        }
    }

    impl PlaytimeSlotPitchShiftTarget {
        fn get_effective_max_value(&self) -> Positive<f64> {
            self.max_value
                .unwrap_or_else(|| Positive::<f64>::new(24.0).unwrap())
        }

        fn convert_semitones_to_unit_value(&self, semitones: f64) -> UnitValue {
            let max_value = self.get_effective_max_value();
            let raw_unit_value = (semitones + max_value.get()) / (2.0 * max_value.get());
            UnitValue::new_clamped(raw_unit_value)
        }

        fn convert_unit_value_to_semitones(&self, unit_value: UnitValue) -> Semitones {
            let max_value = self.get_effective_max_value();
            let raw_semitones = unit_value.get() * max_value.get() * 2.0 - max_value.get();
            Semitones::new_panic(raw_semitones)
        }

        fn get_pitch_shift(&self, context: ControlContext) -> anyhow::Result<Semitones> {
            Backbone::get()
                .with_clip_matrix(context.instance(), |matrix| {
                    let semitones = matrix
                        .get_slot(self.slot_coordinates)?
                        .get_first_clip()?
                        .time_pitch_transform()
                        .pitch_shift;
                    Ok(semitones)
                })
                .flatten()
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotPitchShiftTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let val = self
                .get_pitch_shift(context)
                .ok()
                .map(|v| self.convert_semitones_to_unit_value(v.get()))
                .map(AbsoluteValue::Continuous);
            interpret_current_clip_slot_value(val)
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }
}
