use crate::domain::playtime_util::interpret_current_clip_slot_value;
use crate::domain::targets::playtime_timing_commons::{ClipTimingInfo, SectionVariable};
use crate::domain::{
    Backbone, CompartmentKind, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTargetType, ResolveError,
    TargetCharacter, UnresolvedPlaytimeSlotFactorTargetBase,
    UnresolvedPlaytimeSlotSectionTargetBase,
};
use anyhow::Context;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use helgobox_api::persistence::TimeUnit;
use playtime_api::persistence::{PositiveFactor, Section, SlotAddress};
use playtime_clip_engine::base::{ClipAddress, ClipMatrixEvent, Matrix};
use playtime_clip_engine::rt::supplier::TimePitchTransform;
use playtime_clip_engine::rt::{ClipChangeEvent, QualifiedClipChangeEvent};
use playtime_clip_engine::{Timeline, clip_timeline};
use reaper_common_types::{Bpm, DurationInSeconds, PositionInSeconds};
use reaper_medium::TimeSignature;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use typed_floats::{NonNaN, Positive};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotFactorTargetBase {
    pub slot_coordinates: SlotAddress,
    pub max_value: Option<Positive<f64>>,
    pub factor_variable: FactorVariable,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FactorVariable {
    TempoFactor,
}

impl PlaytimeSlotFactorTargetBase {
    pub fn new(
        unresolved: &UnresolvedPlaytimeSlotFactorTargetBase,
        factor_variable: FactorVariable,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Self, ResolveError> {
        let base = Self {
            slot_coordinates: unresolved.slot.resolve(context, compartment)?,
            max_value: unresolved
                .max_value_expression_evaluator
                .as_ref()
                .and_then(|e| e.evaluate().ok())
                .and_then(|v| v.try_into().ok()),
            factor_variable,
        };
        Ok(base)
    }

    fn get_effective_max_value(&self, context: ControlContext) -> Positive<f64> {
        self.max_value
            .unwrap_or_else(|| Positive::<f64>::new(4.0).unwrap())
    }

    pub fn get_time_pitch_transform(
        &self,
        context: ControlContext,
    ) -> anyhow::Result<TimePitchTransform> {
        Backbone::get()
            .with_clip_matrix(context.instance(), |matrix| {
                let clip = matrix.get_slot(self.slot_coordinates)?.get_first_clip()?;
                Ok(clip.time_pitch_transform())
            })
            .flatten()
    }

    fn get_factor(&self, time_pitch_transform: &TimePitchTransform) -> PositiveFactor {
        match self.factor_variable {
            FactorVariable::TempoFactor => time_pitch_transform.tempo_factor,
        }
    }

    fn convert_unit_value_to_factor(
        &self,
        unit_value: UnitValue,
        context: ControlContext,
    ) -> PositiveFactor {
        let max_value = self.get_effective_max_value(context);
        let raw_factor = max_value.get().powf(2.0 * unit_value.get() - 1.0);
        PositiveFactor::new(raw_factor).unwrap_or_default()
    }

    fn convert_factor_to_unit_value(&self, factor: f64, context: ControlContext) -> UnitValue {
        let max_value = self.get_effective_max_value(context);
        let raw_unit_value = (factor.log(max_value.get()) + 1.0) / 2.0;
        UnitValue::new_clamped(raw_unit_value)
    }
}

impl<'a> Target<'a> for PlaytimeSlotFactorTargetBase {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let time_pitch_transform = self.get_time_pitch_transform(context).ok()?;
        let factor = self.get_factor(&time_pitch_transform);
        let unit_value = self.convert_factor_to_unit_value(factor.get(), context);
        let val = AbsoluteValue::Continuous(unit_value);
        interpret_current_clip_slot_value(Some(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl RealearnTarget for PlaytimeSlotFactorTargetBase {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn clip_slot_address(&self) -> Option<SlotAddress> {
        Some(self.slot_coordinates)
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let factor: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
        let unit_value = self.convert_factor_to_unit_value(factor, context);
        Ok(unit_value)
    }

    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        let factor = self.convert_unit_value_to_factor(value, context);
        format!("{:.2}", factor.get())
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "x"
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_unit_value()?;
        Backbone::get()
            .with_clip_matrix_mut(
                context.control_context.instance(),
                |matrix| -> anyhow::Result<HitResponse> {
                    let factor = self.convert_unit_value_to_factor(value, context.control_context);
                    match self.factor_variable {
                        FactorVariable::TempoFactor => {
                            matrix.set_slot_tempo_factor(self.slot_coordinates, factor)?
                        }
                    };
                    Ok(HitResponse::processed_with_effect())
                },
            )
            .map_err(|_| "couldn't acquire matrix")?
            .map_err(|_| "couldn't set clip start position")
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
                    event: ClipChangeEvent::TimePitchTransform(_) | ClipChangeEvent::Everything,
                },
            )) if clip_address.slot_address == self.slot_coordinates => (true, None),
            _ => (false, None),
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        unimplemented!()
    }
}
