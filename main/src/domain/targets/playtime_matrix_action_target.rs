use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};

use realearn_api::persistence::ClipMatrixAction;

#[derive(Debug)]
pub struct UnresolvedPlaytimeMatrixActionTarget {
    pub action: ClipMatrixAction,
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
    pub action: ClipMatrixAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealTimeClipMatrixTarget {
    action: ClipMatrixAction,
}

pub const PLAYTIME_MATRIX_TARGET: TargetTypeDef = TargetTypeDef {
    lua_only: true,
    section: TargetSection::Playtime,
    name: "Matrix action",
    short_name: "Playtime matrix action",
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{
        ControlContext, PlaytimeMatrixActionTarget, RealTimeClipMatrixTarget,
        RealTimeControlContext, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeMatrixActionTarget {}
    impl<'a> Target<'a> for PlaytimeMatrixActionTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimeClipMatrixTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimeClipMatrixTarget {
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
        MappingControlContext, PlaytimeMatrixActionTarget, RealTimeClipMatrixTarget,
        RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTargetType,
        TargetCharacter,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};

    use playtime_api::persistence::{EvenQuantization, RecordLength};
    use playtime_clip_engine::base::{ClipMatrixEvent, Matrix};
    use playtime_clip_engine::rt::{QualifiedSlotChangeEvent, SlotChangeEvent};
    use realearn_api::persistence::ClipMatrixAction;

    use std::borrow::Cow;

    impl PlaytimeMatrixActionTarget {
        fn trigger(&self, matrix: &mut Matrix) -> anyhow::Result<()> {
            match self.action {
                ClipMatrixAction::Stop => {
                    matrix.stop();
                }
                ClipMatrixAction::Undo => {
                    let _ = matrix.undo();
                }
                ClipMatrixAction::Redo => {
                    let _ = matrix.redo();
                }
                ClipMatrixAction::BuildScene => {
                    matrix.build_scene_in_first_empty_row()?;
                }
                ClipMatrixAction::SetRecordDurationToOpenEnd => {
                    matrix.set_record_duration(RecordLength::OpenEnd);
                }
                ClipMatrixAction::SetRecordDurationToOneBar => {
                    matrix.set_record_duration(record_duration_in_bars(1));
                }
                ClipMatrixAction::SetRecordDurationToTwoBars => {
                    matrix.set_record_duration(record_duration_in_bars(2));
                }
                ClipMatrixAction::SetRecordDurationToFourBars => {
                    matrix.set_record_duration(record_duration_in_bars(4));
                }
                ClipMatrixAction::SetRecordDurationToEightBars => {
                    matrix.set_record_duration(record_duration_in_bars(8));
                }
            }
            Ok(())
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
            if !value.is_on() {
                return Ok(HitResponse::ignored());
            }
            self.trigger(matrix)
                .map_err(|_| "couldn't carry out matrix action")?;
            Ok(HitResponse::processed_with_effect())
        }

        fn process_change_event(
            &self,
            evt: CompoundChangeEvent,
            _: ControlContext,
        ) -> (bool, Option<AbsoluteValue>) {
            match self.action {
                ClipMatrixAction::Stop | ClipMatrixAction::BuildScene => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::EverythingChanged) => {
                        (true, None)
                    }
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SlotChanged(
                        QualifiedSlotChangeEvent { event, .. },
                    )) => match event {
                        SlotChangeEvent::PlayState(_) => (true, None),
                        SlotChangeEvent::Clips(_) => (true, None),
                        _ => (false, None),
                    },
                    _ => (false, None),
                },
                ClipMatrixAction::Undo | ClipMatrixAction::Redo => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::EverythingChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
                ClipMatrixAction::SetRecordDurationToOpenEnd
                | ClipMatrixAction::SetRecordDurationToOneBar
                | ClipMatrixAction::SetRecordDurationToTwoBars
                | ClipMatrixAction::SetRecordDurationToFourBars
                | ClipMatrixAction::SetRecordDurationToEightBars => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::RecordDurationChanged) => {
                        (true, None)
                    }
                    _ => (false, None),
                },
            }
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeMatrixAction)
        }

        fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
            if !matches!(self.action, ClipMatrixAction::Stop) {
                return None;
            }
            let t = RealTimeClipMatrixTarget {
                action: self.action,
            };
            Some(RealTimeReaperTarget::ClipMatrix(t))
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
                        ClipMatrixAction::Stop | ClipMatrixAction::BuildScene => {
                            matrix.is_stoppable()
                        }
                        ClipMatrixAction::Undo => matrix.can_undo(),
                        ClipMatrixAction::Redo => matrix.can_redo(),
                        ClipMatrixAction::SetRecordDurationToOpenEnd => {
                            matrix.settings().clip_record_settings.duration == RecordLength::OpenEnd
                        }
                        ClipMatrixAction::SetRecordDurationToOneBar => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(1)
                        }
                        ClipMatrixAction::SetRecordDurationToTwoBars => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(2)
                        }
                        ClipMatrixAction::SetRecordDurationToFourBars => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(4)
                        }
                        ClipMatrixAction::SetRecordDurationToEightBars => {
                            matrix.settings().clip_record_settings.duration
                                == record_duration_in_bars(8)
                        }
                    };
                    Some(AbsoluteValue::from_bool(bool_value))
                })
                .ok()?
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }
    impl RealTimeClipMatrixTarget {
        pub fn hit(
            &mut self,
            value: ControlValue,
            context: RealTimeControlContext,
        ) -> Result<(), &'static str> {
            match self.action {
                ClipMatrixAction::Stop => {
                    if !value.is_on() {
                        return Ok(());
                    }
                    let matrix = context.clip_matrix()?;
                    let matrix = matrix.lock();
                    matrix.stop(None);
                    Ok(())
                }
                _ => Err("only matrix stop has real-time target support"),
            }
        }
    }
    impl<'a> Target<'a> for RealTimeClipMatrixTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            match self.action {
                ClipMatrixAction::Stop => {
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
    fn control_type_and_character(action: ClipMatrixAction) -> (ControlType, TargetCharacter) {
        use ClipMatrixAction::*;
        match action {
            SetRecordDurationToOpenEnd
            | SetRecordDurationToOneBar
            | SetRecordDurationToTwoBars
            | SetRecordDurationToFourBars
            | SetRecordDurationToEightBars
            | Stop
            | Undo
            | Redo
            | BuildScene => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
        }
    }
    /// Panics if you pass zero.
    fn record_duration_in_bars(bars: u32) -> RecordLength {
        RecordLength::Quantized(EvenQuantization::new(bars, 1).unwrap())
    }
}
