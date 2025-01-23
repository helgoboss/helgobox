use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgobox_api::persistence::PlaytimeMatrixAction;

#[derive(Debug)]
pub struct UnresolvedPlaytimeMatrixActionTarget {
    pub action: PlaytimeMatrixAction,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeMatrixActionTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = PlaytimeMatrixActionTarget {
            action: self.action,
        };
        Ok(vec![ReaperTarget::PlaytimeMatrixAction(target)])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeMatrixActionTarget {
    pub action: PlaytimeMatrixAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealTimePlaytimeMatrixTarget {
    action: PlaytimeMatrixAction,
}

pub const PLAYTIME_MATRIX_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Matrix action",
    short_name: "Playtime matrix action",
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{
        ControlContext, PlaytimeMatrixActionTarget, RealTimeControlContext,
        RealTimePlaytimeMatrixTarget, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeMatrixActionTarget {}
    impl<'a> Target<'a> for PlaytimeMatrixActionTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimePlaytimeMatrixTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimePlaytimeMatrixTarget {
        pub fn hit(
            &mut self,
            _value: ControlValue,
            _context: RealTimeControlContext,
        ) -> Result<bool, &'static str> {
            Err("Playtime not available")
        }
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        convert_count_to_step_size, format_value_as_on_off, Backbone, CompoundChangeEvent,
        ControlContext, HitResponse, MappingControlContext, PlaytimeMatrixActionTarget,
        RealTimeControlContext, RealTimePlaytimeMatrixTarget, RealTimeReaperTarget, RealearnTarget,
        ReaperTargetType, TargetCharacter,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};

    use helgobox_api::persistence::PlaytimeMatrixAction;
    use playtime_api::persistence::{EvenQuantization, RecordLengthMode};
    use playtime_clip_engine::base::{ClipMatrixEvent, Matrix, SequencerStatus};
    use playtime_clip_engine::rt::{QualifiedSlotChangeEvent, SlotChangeEvent};

    use std::borrow::Cow;

    impl PlaytimeMatrixActionTarget {
        fn invoke(&self, matrix: &mut Matrix, value: ControlValue) -> anyhow::Result<HitResponse> {
            match self.action {
                PlaytimeMatrixAction::Stop => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.stop();
                }
                PlaytimeMatrixAction::Undo => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    let _ = matrix.undo();
                }
                PlaytimeMatrixAction::Redo => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    let _ = matrix.redo();
                }
                PlaytimeMatrixAction::BuildScene => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.build_scene_in_first_empty_row()?;
                }
                PlaytimeMatrixAction::SetRecordLengthMode => {
                    let mode = convert_control_value_to_record_length_mode(value)?;
                    matrix.set_record_length_mode(mode);
                }
                PlaytimeMatrixAction::SetCustomRecordLengthInBars => {
                    let value = value
                        .to_discrete_value(RECORD_LENGTH_BARS_COUNT)
                        .map_err(anyhow::Error::msg)?;
                    let quantization = EvenQuantization::new(value.actual() + 1, 1)?;
                    matrix.set_custom_record_length(quantization);
                }
                PlaytimeMatrixAction::ClickOnOffState => {
                    matrix.set_click_enabled(value.is_on());
                }
                PlaytimeMatrixAction::MidiAutoQuantizationOnOffState => {
                    matrix.set_midi_auto_quantize_enabled(value.is_on());
                }
                PlaytimeMatrixAction::SmartRecord => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.trigger_smart_record(true)?;
                }
                PlaytimeMatrixAction::StartOrStopPlayback => {
                    if value.is_on() {
                        matrix.play_all_ignited();
                    } else {
                        matrix.enter_silence_mode();
                    }
                }
                PlaytimeMatrixAction::SilenceModeOnOffState => {
                    if value.is_on() {
                        matrix.enter_silence_mode();
                    } else {
                        matrix.leave_silence_mode();
                    }
                }
                PlaytimeMatrixAction::Panic => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.panic();
                }
                PlaytimeMatrixAction::SequencerRecordOnOffState => {
                    if value.is_on() {
                        matrix.record_new_sequence();
                    } else {
                        matrix.stop_sequencer();
                    }
                }
                PlaytimeMatrixAction::SequencerPlayOnOffState => {
                    if value.is_on() {
                        matrix.play_active_sequence()?;
                    } else {
                        matrix.stop_sequencer();
                    }
                }
                PlaytimeMatrixAction::TapTempo => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.tap_tempo();
                }
            }
            Ok(HitResponse::processed_with_effect())
        }
    }

    impl RealearnTarget for PlaytimeMatrixActionTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            control_type_and_character(self.action)
        }

        fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
            match self.action {
                PlaytimeMatrixAction::SetRecordLengthMode => {
                    convert_control_value_to_record_length_mode(ControlValue::AbsoluteContinuous(
                        value,
                    ))
                    .map(|m| serde_plain::to_string(&m).unwrap())
                    .unwrap_or_else(|_| "<Reserved>".to_string())
                }
                PlaytimeMatrixAction::SetCustomRecordLengthInBars => {
                    self.format_as_discrete_or_percentage(value, context)
                }
                _ => format_value_as_on_off(value).to_string(),
            }
        }

        fn convert_unit_value_to_discrete_value(
            &self,
            value: UnitValue,
            _context: ControlContext,
        ) -> Result<u32, &'static str> {
            match self.action {
                PlaytimeMatrixAction::SetRecordLengthMode => {
                    Ok(value.to_discrete(RECORD_LENGTH_MODES_COUNT - 1))
                }
                PlaytimeMatrixAction::SetCustomRecordLengthInBars => {
                    Ok(value.to_discrete(RECORD_LENGTH_BARS_COUNT - 1) + 1)
                }
                _ => Err("not supported"),
            }
        }

        fn convert_discrete_value_to_unit_value(
            &self,
            value: u32,
            _context: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            match self.action {
                PlaytimeMatrixAction::SetRecordLengthMode => {
                    let uv = value as f64 / (RECORD_LENGTH_MODES_COUNT - 1) as f64;
                    Ok(UnitValue::new_clamped(uv))
                }
                PlaytimeMatrixAction::SetCustomRecordLengthInBars => {
                    convert_record_length_numerator_to_unit_value(value)
                }
                _ => Err("not supported"),
            }
        }

        fn parse_as_value(
            &self,
            text: &str,
            _context: ControlContext,
        ) -> Result<UnitValue, &'static str> {
            match self.action {
                PlaytimeMatrixAction::SetRecordLengthMode => {
                    let mode: RecordLengthMode =
                        serde_plain::from_str(text).map_err(|_| "invalid mode")?;
                    let uv = UnitValue::new_clamped(
                        mode as usize as f64 / (RECORD_LENGTH_MODES_COUNT - 1) as f64,
                    );
                    Ok(uv)
                }
                PlaytimeMatrixAction::SetCustomRecordLengthInBars => {
                    let numerator = text.parse().map_err(|_| "no valid number")?;
                    convert_record_length_numerator_to_unit_value(numerator)
                }
                _ => Err("not supported"),
            }
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            let mut instance = context.control_context.instance().borrow_mut();
            let matrix = instance
                .clip_matrix_mut()
                .ok_or("couldn't acquire matrix")?;
            let response = self
                .invoke(matrix, value)
                .map_err(|_| "couldn't carry out matrix action")?;
            Ok(response)
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            _: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            if matches!(
                evt,
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::EverythingChanged)
            ) {
                return (true, None);
            }
            match self.action {
                PlaytimeMatrixAction::Stop | PlaytimeMatrixAction::BuildScene => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SlotChanged(
                        QualifiedSlotChangeEvent { event, .. },
                    )) => match event {
                        SlotChangeEvent::PlayState(_) => (true, None),
                        SlotChangeEvent::Clips(_) => (true, None),
                        _ => (false, None),
                    },
                    _ => (false, None),
                },
                PlaytimeMatrixAction::SetRecordLengthMode
                | PlaytimeMatrixAction::SetCustomRecordLengthInBars => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::MatrixSettingsChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
                PlaytimeMatrixAction::ClickOnOffState => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClickEnabledChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
                PlaytimeMatrixAction::MidiAutoQuantizationOnOffState => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::MatrixSettingsChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
                PlaytimeMatrixAction::SmartRecord => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SlotChanged(
                        QualifiedSlotChangeEvent {
                            event: SlotChangeEvent::PlayState(_),
                            ..
                        },
                    )) => (true, None),
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SequencerPlayStateChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
                PlaytimeMatrixAction::StartOrStopPlayback
                | PlaytimeMatrixAction::SilenceModeOnOffState => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SilenceModeChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
                _ => (false, None),
            }
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeMatrixAction)
        }

        fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
            use PlaytimeMatrixAction::*;
            let supports_rt_control = matches!(
                self.action,
                Stop | SmartRecord
                    | StartOrStopPlayback
                    | SilenceModeOnOffState
                    | Panic
                    | TapTempo
            );
            if !supports_rt_control {
                return None;
            }
            let t = RealTimePlaytimeMatrixTarget {
                action: self.action,
            };
            Some(RealTimeReaperTarget::PlaytimeMatrix(t))
        }

        fn is_available(&self, _: ControlContext) -> bool {
            true
        }

        fn value_unit(&self, context: ControlContext) -> &'static str {
            match self.action {
                PlaytimeMatrixAction::SetCustomRecordLengthInBars => "bars",
                _ => self.value_unit_default(context),
            }
        }
    }
    impl<'a> Target<'a> for PlaytimeMatrixActionTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            Backbone::get()
                .with_clip_matrix(context.instance(), |matrix| {
                    let bool_value = match self.action {
                        PlaytimeMatrixAction::Stop | PlaytimeMatrixAction::BuildScene => {
                            matrix.is_stoppable()
                        }
                        PlaytimeMatrixAction::Undo => matrix.can_undo(),
                        PlaytimeMatrixAction::Redo => matrix.can_redo(),
                        PlaytimeMatrixAction::SetRecordLengthMode => {
                            let value = matrix.settings().clip_record_settings.length_mode as usize;
                            let fraction = Fraction::new(value as _, RECORD_LENGTH_MODES_COUNT - 1);
                            return Some(AbsoluteValue::Discrete(fraction));
                        }
                        PlaytimeMatrixAction::SetCustomRecordLengthInBars => {
                            let custom_length =
                                matrix.settings().clip_record_settings.custom_length;
                            if custom_length.denominator() != 1 {
                                return None;
                            }
                            let fraction = Fraction::new(
                                custom_length.numerator() - 1,
                                RECORD_LENGTH_BARS_COUNT - 1,
                            );
                            return Some(AbsoluteValue::Discrete(fraction));
                        }
                        PlaytimeMatrixAction::ClickOnOffState => matrix.click_is_enabled(),
                        PlaytimeMatrixAction::MidiAutoQuantizationOnOffState => {
                            matrix.midi_auto_quantize_enabled()
                        }
                        PlaytimeMatrixAction::SmartRecord => {
                            matrix.sequencer().status() == SequencerStatus::Recording
                                || matrix.num_really_recording_clips() > 0
                        }
                        PlaytimeMatrixAction::StartOrStopPlayback => {
                            !matrix.is_in_silence_mode()
                        }
                        PlaytimeMatrixAction::SilenceModeOnOffState => matrix.is_in_silence_mode(),
                        PlaytimeMatrixAction::Panic => {
                            return None;
                        }
                        PlaytimeMatrixAction::SequencerRecordOnOffState => {
                            matrix.sequencer().status() == SequencerStatus::Recording
                        }
                        PlaytimeMatrixAction::SequencerPlayOnOffState => {
                            matrix.sequencer().status() == SequencerStatus::Playing
                        }
                        PlaytimeMatrixAction::TapTempo => return None,
                    };
                    Some(AbsoluteValue::from_bool(bool_value))
                })
                .ok()?
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }
    impl RealTimePlaytimeMatrixTarget {
        /// This returns `Ok(true)` if the event should still be forwarded to the main thread because there's still
        /// something to do, but it can only be done in the main thread (e.g. when using smart record and there's
        /// no tempo detection recording to be stopped).
        pub fn hit(
            &mut self,
            value: ControlValue,
            context: RealTimeControlContext,
        ) -> Result<bool, &'static str> {
            use PlaytimeMatrixAction::*;
            let matrix = context.clip_matrix()?;
            let mut matrix = matrix.lock();
            match self.action {
                Stop => {
                    if !value.is_on() {
                        return Ok(false);
                    }
                    matrix.stop();
                    Ok(false)
                }
                SmartRecord => {
                    // We only have a real-time shortcut here for the "tempo detection stop" case.
                    // And this one is quite useful because it results in a low latency recording stop.
                    if !value.is_on() {
                        return Ok(false);
                    }
                    let forward_to_main_thread = !matrix.maybe_stop_tempo_detection_recording();
                    Ok(forward_to_main_thread)
                }
                StartOrStopPlayback => {
                    if value.is_on() {
                        matrix.play_all_ignited();
                    } else {
                        matrix.enter_silence_mode();
                    }
                    Ok(false)
                }
                SilenceModeOnOffState => {
                    if value.is_on() {
                        matrix.enter_silence_mode();
                    } else {
                        matrix.leave_silence_mode();
                    }
                    Ok(false)
                }
                Panic => {
                    if value.is_on() {
                        return Ok(false);
                    }
                    matrix.panic(false);
                    Ok(false)
                }
                TapTempo => {
                    if !value.is_on() {
                        return Ok(false);
                    }
                    matrix.tap_tempo();
                    Ok(false)
                }
                _ => Err("this action doesn't have real-time target support"),
            }
        }
    }
    impl<'a> Target<'a> for RealTimePlaytimeMatrixTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            // The following is NOT necessary for feedback (ReaLearn always uses the non-rt-target for feedback)
            use PlaytimeMatrixAction::*;
            let matrix = context.clip_matrix().ok()?;
            let matrix = matrix.lock();
            let bool_value = match self.action {
                Stop => matrix.is_stoppable(),
                StartOrStopPlayback => !matrix.is_in_silence_mode(),
                SilenceModeOnOffState => matrix.is_in_silence_mode(),
                SmartRecord => {
                    // Only relevant for feedback
                    return None;
                }
                Panic | TapTempo => {
                    // Not relevant in general
                    return None;
                }
                _ => return None,
            };
            Some(AbsoluteValue::from_bool(bool_value))
        }

        fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
            control_type_and_character(self.action).0
        }
    }
    fn control_type_and_character(action: PlaytimeMatrixAction) -> (ControlType, TargetCharacter) {
        use PlaytimeMatrixAction::*;
        match action {
            SetRecordLengthMode => (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(RECORD_LENGTH_MODES_COUNT),
                    is_retriggerable: false,
                },
                TargetCharacter::Discrete,
            ),
            SetCustomRecordLengthInBars => (
                ControlType::AbsoluteDiscrete {
                    atomic_step_size: convert_count_to_step_size(RECORD_LENGTH_BARS_COUNT),
                    is_retriggerable: false,
                },
                TargetCharacter::Discrete,
            ),
            Stop | Undo | Redo | BuildScene | Panic | SmartRecord | TapTempo => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            ClickOnOffState
            | MidiAutoQuantizationOnOffState
            | SilenceModeOnOffState
            | SequencerRecordOnOffState
            | SequencerPlayOnOffState
            | StartOrStopPlayback => {
                (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
            }
        }
    }

    fn convert_control_value_to_record_length_mode(
        value: ControlValue,
    ) -> anyhow::Result<RecordLengthMode> {
        let value = value
            .to_discrete_value(RECORD_LENGTH_MODES_COUNT)
            .map_err(anyhow::Error::msg)?;
        let mode = RecordLengthMode::try_from(value.actual() as usize)?;
        Ok(mode)
    }

    fn convert_record_length_numerator_to_unit_value(
        value: u32,
    ) -> Result<UnitValue, &'static str> {
        let shifted_value = value
            .checked_sub(1)
            .ok_or("length of 0 bars is not possible")?;
        let uv = shifted_value as f64 / (RECORD_LENGTH_BARS_COUNT - 1) as f64;
        Ok(UnitValue::new_clamped(uv))
    }

    const RECORD_LENGTH_MODES_COUNT: u32 = 10;
    const RECORD_LENGTH_BARS_COUNT: u32 = 64;
}
