use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};

use realearn_api::persistence::PlaytimeMatrixAction;

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
        ) -> Result<(), &'static str> {
            Err("Playtime not available")
        }
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        format_value_as_on_off, Backbone, CompoundChangeEvent, ControlContext, HitResponse,
        MappingControlContext, PlaytimeMatrixActionTarget, RealTimeControlContext,
        RealTimePlaytimeMatrixTarget, RealTimeReaperTarget, RealearnTarget, ReaperTargetType,
        TargetCharacter,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};

    use playtime_api::persistence::{EvenQuantization, RecordLength};
    use playtime_clip_engine::base::{ClipMatrixEvent, Matrix, SequencerStatus};
    use playtime_clip_engine::rt::{QualifiedSlotChangeEvent, SlotChangeEvent};
    use realearn_api::persistence::PlaytimeMatrixAction;

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
                PlaytimeMatrixAction::SetRecordDurationToOpenEnd => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.set_record_duration(RecordLength::OpenEnd);
                }
                PlaytimeMatrixAction::SetRecordDurationToOneBar => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.set_record_duration(record_duration_in_bars(1));
                }
                PlaytimeMatrixAction::SetRecordDurationToTwoBars => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.set_record_duration(record_duration_in_bars(2));
                }
                PlaytimeMatrixAction::SetRecordDurationToFourBars => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.set_record_duration(record_duration_in_bars(4));
                }
                PlaytimeMatrixAction::SetRecordDurationToEightBars => {
                    if !value.is_on() {
                        return Ok(HitResponse::ignored());
                    }
                    matrix.set_record_duration(record_duration_in_bars(8));
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
                    matrix.trigger_smart_record()?;
                }
                PlaytimeMatrixAction::EnterSilenceModeOrPlayIgnited => {
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

        fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_on_off(value).to_string()
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
                PlaytimeMatrixAction::SetRecordDurationToOpenEnd
                | PlaytimeMatrixAction::SetRecordDurationToOneBar
                | PlaytimeMatrixAction::SetRecordDurationToTwoBars
                | PlaytimeMatrixAction::SetRecordDurationToFourBars
                | PlaytimeMatrixAction::SetRecordDurationToEightBars => match evt {
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
            if !matches!(
                self.action,
                PlaytimeMatrixAction::Stop | PlaytimeMatrixAction::SmartRecord
            ) {
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
                        PlaytimeMatrixAction::SetRecordDurationToOpenEnd => {
                            matrix.settings().clip_record_settings.duration == RecordLength::OpenEnd
                        }
                        PlaytimeMatrixAction::SetRecordDurationToOneBar => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(1)
                        }
                        PlaytimeMatrixAction::SetRecordDurationToTwoBars => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(2)
                        }
                        PlaytimeMatrixAction::SetRecordDurationToFourBars => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(4)
                        }
                        PlaytimeMatrixAction::SetRecordDurationToEightBars => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(8)
                        }
                        PlaytimeMatrixAction::ClickOnOffState => matrix.click_is_enabled(),
                        PlaytimeMatrixAction::MidiAutoQuantizationOnOffState => {
                            matrix.midi_auto_quantize_enabled()
                        }
                        PlaytimeMatrixAction::SmartRecord => {
                            return None;
                        }
                        PlaytimeMatrixAction::EnterSilenceModeOrPlayIgnited => {
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
            match self.action {
                // TODO-medium Making tempo tap rt-capable might also make sense!
                PlaytimeMatrixAction::Stop => {
                    if !value.is_on() {
                        return Ok(false);
                    }
                    let matrix = context.clip_matrix()?;
                    let matrix = matrix.lock();
                    matrix.stop();
                    Ok(false)
                }
                PlaytimeMatrixAction::SmartRecord => {
                    if !value.is_on() {
                        return Ok(false);
                    }
                    let matrix = context.clip_matrix()?;
                    let matrix = matrix.lock();
                    let forward_to_main_thread = !matrix.maybe_stop_tempo_detection_recording();
                    Ok(forward_to_main_thread)
                }
                _ => Err("only matrix stop has real-time target support"),
            }
        }
    }
    impl<'a> Target<'a> for RealTimePlaytimeMatrixTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            match self.action {
                PlaytimeMatrixAction::Stop => {
                    let matrix = context.clip_matrix().ok()?;
                    let matrix = matrix.lock();
                    let is_stoppable = matrix.is_stoppable();
                    Some(AbsoluteValue::from_bool(is_stoppable))
                }
                _ => None,
            }
        }

        fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
            control_type_and_character(self.action).0
        }
    }
    fn control_type_and_character(action: PlaytimeMatrixAction) -> (ControlType, TargetCharacter) {
        use PlaytimeMatrixAction::*;
        match action {
            SetRecordDurationToOpenEnd
            | SetRecordDurationToOneBar
            | SetRecordDurationToTwoBars
            | SetRecordDurationToFourBars
            | SetRecordDurationToEightBars
            | Stop
            | Undo
            | Redo
            | BuildScene
            | Panic
            | SmartRecord
            | TapTempo => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            ClickOnOffState
            | MidiAutoQuantizationOnOffState
            | SilenceModeOnOffState
            | SequencerRecordOnOffState
            | SequencerPlayOnOffState
            | EnterSilenceModeOrPlayIgnited => {
                (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
            }
        }
    }
    /// Panics if you pass zero.
    fn record_duration_in_bars(bars: u32) -> RecordLength {
        RecordLength::Quantized(EvenQuantization::new(bars, 1).unwrap())
    }
}
