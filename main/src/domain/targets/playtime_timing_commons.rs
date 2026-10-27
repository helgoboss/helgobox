use crate::domain::playtime_util::interpret_current_clip_slot_value;
use crate::domain::{
    Backbone, CompartmentKind, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTargetType, ResolveError,
    TargetCharacter, UnresolvedPlaytimeSlotSectionTargetBase,
};
use anyhow::Context;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use helgobox_api::persistence::TimeUnit;
use playtime_api::persistence::{Section, SlotAddress};
use playtime_clip_engine::base::{ClipAddress, ClipMatrixEvent, Matrix};
use playtime_clip_engine::rt::{ClipChangeEvent, QualifiedClipChangeEvent};
use playtime_clip_engine::{Timeline, clip_timeline};
use reaper_common_types::{Bpm, DurationInSeconds, PositionInSeconds};
use reaper_medium::TimeSignature;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use typed_floats::{NonNaN, Positive};

pub struct ClipTimingInfo {
    pub dynamic_section: Section,
    pub fixed_source_length: DurationInSeconds,
    pub source_tempo: Bpm,
}

impl ClipTimingInfo {
    pub fn from_slot(matrix: &Matrix, slot_address: SlotAddress) -> anyhow::Result<Self> {
        let clip_employment = matrix.get_slot(slot_address)?.get_first_clip_employment()?;
        let runtime_data = &clip_employment.get_online_data()?.runtime_data;
        let timing_info = ClipTimingInfo {
            dynamic_section: clip_employment.clip.dynamic_section(),
            fixed_source_length: runtime_data
                .source_material_info
                .fixed_source_length(&clip_employment.clip.rt_settings()),
            source_tempo: runtime_data
                .source_material_info
                .neutral_playback_tempo()
                .context("missing clip tempo")?,
        };
        Ok(timing_info)
    }

    pub fn get_start_position(
        &self,
        time_unit: TimeUnit,
        project_time_signature: TimeSignature,
    ) -> NonNaN<f64> {
        self.seconds_to_time_unit(
            self.dynamic_section.start_pos,
            time_unit,
            project_time_signature,
        )
    }

    pub fn get_length(
        &self,
        time_unit: TimeUnit,
        project_time_signature: TimeSignature,
    ) -> NonNaN<f64> {
        self.seconds_to_time_unit(
            self.dynamic_section
                .length
                .map(PositionInSeconds::from)
                .unwrap_or_default(),
            time_unit,
            project_time_signature,
        )
    }

    pub fn get_fixed_source_length(
        &self,
        time_unit: TimeUnit,
        project_time_signature: TimeSignature,
    ) -> Positive<f64> {
        self.seconds_to_time_unit(
            self.fixed_source_length.into(),
            time_unit,
            project_time_signature,
        )
        .try_into()
        .expect("get_fixed_source_length")
    }

    fn seconds_to_time_unit(
        &self,
        seconds: PositionInSeconds,
        time_unit: TimeUnit,
        project_time_signature: TimeSignature,
    ) -> NonNaN<f64> {
        match time_unit {
            TimeUnit::Seconds => seconds_to_non_nan(seconds),
            TimeUnit::WholeNotes => seconds_to_whole_notes(seconds, self.source_tempo),
            TimeUnit::Bars => seconds_to_bars(seconds, self.source_tempo, project_time_signature),
        }
    }
}

fn seconds_to_non_nan(seconds: PositionInSeconds) -> NonNaN<f64> {
    seconds.get().try_into().expect("seconds_to_non_nan")
}

fn non_nan_to_seconds(v: NonNaN<f64>) -> PositionInSeconds {
    v.get().try_into().expect("non_nan_to_seconds")
}

fn seconds_to_whole_notes(seconds: PositionInSeconds, bpm: Bpm) -> NonNaN<f64> {
    NonNaN::<f64>::new(seconds.get() * whole_notes_per_second(bpm).get())
        .expect("seconds_to_whole_notes")
}

fn whole_notes_to_seconds(whole_notes: NonNaN<f64>, bpm: Bpm) -> PositionInSeconds {
    PositionInSeconds::new(whole_notes.get() / whole_notes_per_second(bpm).get())
        .expect("whole_notes_to_seconds")
}

fn seconds_to_bars(
    seconds: PositionInSeconds,
    bpm: Bpm,
    time_signature: TimeSignature,
) -> NonNaN<f64> {
    NonNaN::<f64>::new(seconds.get() * bars_per_second(bpm, time_signature).get())
        .expect("seconds_to_bars")
}

fn bars_to_seconds(
    bars: NonNaN<f64>,
    bpm: Bpm,
    time_signature: TimeSignature,
) -> PositionInSeconds {
    PositionInSeconds::new(bars.get() / bars_per_second(bpm, time_signature).get())
        .expect("bars_to_seconds")
}

fn whole_notes_per_second(bpm: Bpm) -> Positive<f64> {
    Positive::<f64>::new(bpm.get() / 60.0 / 4.0).expect("whole_notes_per_second")
}

pub fn bars_per_second(bpm: Bpm, time_signature: TimeSignature) -> Positive<f64> {
    let num = time_signature.numerator.get() as f64;
    let denom = time_signature.denominator.get() as f64;
    let qn_per_bar = num * 4.0 / denom;
    let qn_per_second = bpm.get() / 60.0;
    Positive::<f64>::new(qn_per_second / qn_per_bar).expect("bars_per_second")
}

pub fn get_project_time_signature(context: ControlContext) -> TimeSignature {
    let project = context.processor_context.project_or_current_project();
    let timeline = clip_timeline(project, false);
    timeline.time_signature_at(timeline.cursor_pos().get())
}

pub struct TimeUnitValue {
    pub value: NonNaN<f64>,
    pub time_unit: TimeUnit,
}

impl TimeUnitValue {
    pub fn new(value: NonNaN<f64>, time_unit: TimeUnit) -> Self {
        Self { value, time_unit }
    }

    pub fn to_seconds(&self, bpm: Bpm, time_signature: TimeSignature) -> PositionInSeconds {
        match self.time_unit {
            TimeUnit::Seconds => non_nan_to_seconds(self.value),
            TimeUnit::WholeNotes => whole_notes_to_seconds(self.value, bpm),
            TimeUnit::Bars => bars_to_seconds(self.value, bpm, time_signature),
        }
    }
}

impl Display for TimeUnitValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2} {}", self.value, self.time_unit.suffix())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeSlotSectionTargetBase {
    pub slot_coordinates: SlotAddress,
    pub time_unit: TimeUnit,
    /// Max value in time units.
    pub max_value: Option<Positive<f64>>,
    /// Step size in time units.
    pub step_size: Option<Positive<f64>>,
    pub section_variable: SectionVariable,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SectionVariable {
    StartPosition,
    Length,
}

impl PlaytimeSlotSectionTargetBase {
    pub fn new(
        unresolved: &UnresolvedPlaytimeSlotSectionTargetBase,
        section_variable: SectionVariable,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Self, ResolveError> {
        let base = Self {
            slot_coordinates: unresolved.slot.resolve(context, compartment)?,
            time_unit: unresolved.time_unit,
            max_value: unresolved
                .max_value_expression_evaluator
                .as_ref()
                .and_then(|e| e.evaluate().ok())
                .and_then(|v| v.try_into().ok()),
            step_size: unresolved
                .step_size_expression_evaluator
                .as_ref()
                .and_then(|e| e.evaluate().ok())
                .and_then(|v| v.try_into().ok()),
            section_variable,
        };
        Ok(base)
    }

    fn get_effective_max_value(
        &self,
        context: ControlContext,
        clip_timing_info: &ClipTimingInfo,
    ) -> Positive<f64> {
        self.max_value
            .unwrap_or_else(|| self.get_fixed_source_length(context, &clip_timing_info))
    }

    fn get_fixed_source_length(
        &self,
        context: ControlContext,
        clip_timing_info: &ClipTimingInfo,
    ) -> Positive<f64> {
        let project = context.processor_context.project_or_current_project();
        let timeline = clip_timeline(project, false);
        let time_signature = timeline.time_signature_at(timeline.cursor_pos().get());
        clip_timing_info.get_fixed_source_length(self.time_unit, time_signature)
    }

    pub fn get_clip_timing_info(&self, context: ControlContext) -> anyhow::Result<ClipTimingInfo> {
        Backbone::get()
            .with_clip_matrix(context.instance(), |matrix| {
                ClipTimingInfo::from_slot(matrix, self.slot_coordinates)
            })
            .flatten()
    }

    fn convert_unit_value_to_time_unit_value(
        &self,
        unit_value: UnitValue,
        context: ControlContext,
        clip_timing_info: &ClipTimingInfo,
    ) -> TimeUnitValue {
        let max_value = self.get_effective_max_value(context, &clip_timing_info);
        let raw_time_unit_value = match self.section_variable {
            SectionVariable::StartPosition => {
                unit_value.get() * max_value.get() * 2.0 - max_value.get()
            }
            SectionVariable::Length => unit_value.get() * max_value.get(),
        };
        TimeUnitValue::new(
            raw_time_unit_value
                .try_into()
                .expect("convert_unit_value_to_time_unit_value"),
            self.time_unit,
        )
    }

    fn convert_time_unit_value_to_unit_value(
        &self,
        time_unit_value: f64,
        context: ControlContext,
        clip_timing_info: &ClipTimingInfo,
    ) -> UnitValue {
        let max_value = self.get_effective_max_value(context, &clip_timing_info);
        let raw_unit_value = match self.section_variable {
            SectionVariable::StartPosition => {
                (time_unit_value + max_value.get()) / (2.0 * max_value.get())
            }
            SectionVariable::Length => time_unit_value / max_value.get(),
        };
        UnitValue::new_clamped(raw_unit_value)
    }

    fn get_time_unit_value(
        &self,
        clip_timing_info: &ClipTimingInfo,
        time_signature: TimeSignature,
    ) -> NonNaN<f64> {
        match self.section_variable {
            SectionVariable::StartPosition => {
                clip_timing_info.get_start_position(self.time_unit, time_signature)
            }
            SectionVariable::Length => clip_timing_info.get_length(self.time_unit, time_signature),
        }
    }
}

impl<'a> Target<'a> for PlaytimeSlotSectionTargetBase {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let clip_timing_info = self.get_clip_timing_info(context).ok()?;
        let time_signature = get_project_time_signature(context);
        let time_unit_value = self.get_time_unit_value(&clip_timing_info, time_signature);
        let unit_value = self.convert_time_unit_value_to_unit_value(
            time_unit_value.get(),
            context,
            &clip_timing_info,
        );
        let val = AbsoluteValue::Continuous(unit_value);
        interpret_current_clip_slot_value(Some(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl RealearnTarget for PlaytimeSlotSectionTargetBase {
    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        if let Some(step_size) = self.step_size
            && let Ok(clip_timing_info) = self.get_clip_timing_info(context)
        {
            let max_value = self.get_effective_max_value(context, &clip_timing_info);
            let raw_step_size = match self.section_variable {
                SectionVariable::StartPosition => step_size.get() / (max_value.get() * 2.0),
                SectionVariable::Length => step_size.get() / max_value.get(),
            };
            let control_type = ControlType::AbsoluteDiscrete {
                atomic_step_size: UnitValue::new_clamped(raw_step_size),
                is_retriggerable: false,
                absolute_snap_to_grid: true,
            };
            return (control_type, TargetCharacter::Discrete);
        }
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
        let clip_timing_info = self.get_clip_timing_info(context).map_err(|_| "no clip")?;
        let time_unit_value: f64 = text.parse().map_err(|_| "not a valid decimal value")?;
        let unit_value =
            self.convert_time_unit_value_to_unit_value(time_unit_value, context, &clip_timing_info);
        Ok(unit_value)
    }

    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        let Ok(clip_timing_info) = self.get_clip_timing_info(context) else {
            return "-".to_string();
        };
        let time_unit_value =
            self.convert_unit_value_to_time_unit_value(value, context, &clip_timing_info);
        format!("{:.2}", time_unit_value.value)
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        self.time_unit.suffix()
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
                    let clip_timing_info =
                        ClipTimingInfo::from_slot(matrix, self.slot_coordinates)?;
                    let time_unit_value = self.convert_unit_value_to_time_unit_value(
                        value,
                        context.control_context,
                        &clip_timing_info,
                    );
                    let time_signature = get_project_time_signature(context.control_context);
                    let second_value =
                        time_unit_value.to_seconds(clip_timing_info.source_tempo, time_signature);
                    let new_section = match self.section_variable {
                        SectionVariable::StartPosition => Section {
                            start_pos: second_value.into(),
                            length: clip_timing_info.dynamic_section.length,
                        },
                        SectionVariable::Length => Section {
                            start_pos: clip_timing_info.dynamic_section.start_pos,
                            length: Some(second_value.try_into()?),
                        },
                    };
                    matrix.set_slot_dynamic_section(self.slot_coordinates, new_section)?;
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
                    event: ClipChangeEvent::DynamicSection(_) | ClipChangeEvent::Everything,
                },
            )) if clip_address.slot_address == self.slot_coordinates => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        let clip_timing_info = self.get_clip_timing_info(context).ok()?;
        let time_signature = get_project_time_signature(context);
        let time_unit_value = self.get_time_unit_value(&clip_timing_info, time_signature);
        Some(time_unit_value.to_string().into())
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let clip_timing_info = self.get_clip_timing_info(context).ok()?;
        let time_signature = get_project_time_signature(context);
        let time_unit_value = self.get_time_unit_value(&clip_timing_info, time_signature);
        Some(NumericValue::Decimal(time_unit_value.get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        unimplemented!()
    }
}
