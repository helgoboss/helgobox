use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualPlaytimeColumn, DEFAULT_TARGET,
};

use helgobox_api::persistence::PlaytimeColumnAction;

pub const PLAYTIME_COLUMN_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Playtime,
    name: "Column action",
    short_name: "Playtime column action",
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

#[derive(Debug)]
pub struct UnresolvedPlaytimeColumnActionTarget {
    pub column: VirtualPlaytimeColumn,
    pub action: PlaytimeColumnAction,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeColumnActionTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = PlaytimeColumnActionTarget {
            column_index: self.column.resolve(context, compartment)?,
            action: self.action,
        };
        Ok(vec![ReaperTarget::PlaytimeColumnAction(target)])
    }

    fn clip_column_descriptor(&self) -> Option<&VirtualPlaytimeColumn> {
        Some(&self.column)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaytimeColumnActionTarget {
    pub column_index: usize,
    pub action: PlaytimeColumnAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealTimePlaytimeColumnTarget {
    column_index: usize,
    action: PlaytimeColumnAction,
}

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{
        ControlContext, PlaytimeColumnActionTarget, RealTimeControlContext,
        RealTimePlaytimeColumnTarget, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeColumnActionTarget {}
    impl<'a> Target<'a> for PlaytimeColumnActionTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimePlaytimeColumnTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimePlaytimeColumnTarget {
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
        MappingControlContext, PlaytimeColumnActionTarget, RealTimeControlContext,
        RealTimePlaytimeColumnTarget, RealTimeReaperTarget, RealearnTarget, ReaperTargetType,
        TargetCharacter,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
    use helgobox_api::persistence::PlaytimeColumnAction;
    use playtime_api::persistence::ColumnAddress;
    use playtime_api::runtime::CellAddress;
    use playtime_clip_engine::base::ClipMatrixEvent;
    use playtime_clip_engine::rt::{QualifiedSlotChangeEvent, SlotChangeEvent};
    use std::borrow::Cow;

    impl<'a> Target<'a> for PlaytimeColumnActionTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let is_on = Backbone::get()
                .with_clip_matrix(context.instance(), |matrix| match self.action {
                    PlaytimeColumnAction::Stop => matrix.column_is_stoppable(self.column_index),
                    PlaytimeColumnAction::ArmState | PlaytimeColumnAction::ArmStateExclusive => {
                        matrix.column_is_armed_for_recording(self.column_index)
                    }
                    PlaytimeColumnAction::Activate => {
                        matrix.active_cell().column_index == Some(self.column_index)
                    }
                })
                .ok()?;
            Some(AbsoluteValue::from_bool(is_on))
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }

    impl<'a> Target<'a> for RealTimePlaytimeColumnTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            match self.action {
                PlaytimeColumnAction::Stop => {
                    let matrix = context.clip_matrix().ok()?;
                    let matrix = matrix.lock();
                    let is_stoppable = matrix.column_is_stoppable(self.column_index);
                    Some(AbsoluteValue::from_bool(is_stoppable))
                }
                PlaytimeColumnAction::ArmState
                | PlaytimeColumnAction::ArmStateExclusive
                | PlaytimeColumnAction::Activate => None,
            }
        }

        fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
            control_type_and_character(self.action).0
        }
    }

    impl RealearnTarget for PlaytimeColumnActionTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            control_type_and_character(self.action)
        }

        fn clip_column_address(&self) -> Option<ColumnAddress> {
            Some(ColumnAddress::new(self.column_index))
        }

        fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_on_off(value).to_string()
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            let response = Backbone::get()
                .with_clip_matrix_mut(
                    context.control_context.instance(),
                    |matrix| -> anyhow::Result<HitResponse> {
                        match self.action {
                            PlaytimeColumnAction::Stop => {
                                if !value.is_on() {
                                    return Ok(HitResponse::ignored());
                                }
                                matrix.stop_column(self.column_index);
                            }
                            PlaytimeColumnAction::ArmState => {
                                matrix.set_column_armed(self.column_index, value.is_on())?;
                            }
                            PlaytimeColumnAction::ArmStateExclusive => {
                                matrix.set_column_armed_exclusively(
                                    self.column_index,
                                    value.is_on(),
                                )?;
                            }
                            PlaytimeColumnAction::Activate => {
                                if !value.is_on() {
                                    return Ok(HitResponse::ignored());
                                }
                                matrix.activate_cell(CellAddress::new(
                                    Some(self.column_index),
                                    None,
                                ))?;
                            }
                        }
                        Ok(HitResponse::processed_with_effect())
                    },
                )
                .map_err(|_| "couldn't acquire matrix")?
                .map_err(|_| "couldn't carry out column action")?;
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
                PlaytimeColumnAction::Stop => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SlotChanged(
                        QualifiedSlotChangeEvent {
                            slot_address: sc,
                            event,
                        },
                    )) if sc.column() == self.column_index => match event {
                        SlotChangeEvent::PlayState(_) => (true, None),
                        SlotChangeEvent::Clips(_) => (true, None),
                        _ => (false, None),
                    },
                    _ => (false, None),
                },
                PlaytimeColumnAction::ArmState | PlaytimeColumnAction::ArmStateExclusive => {
                    match evt {
                        CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::TrackChanged(_)) => {
                            (true, None)
                        }
                        _ => (false, None),
                    }
                }
                PlaytimeColumnAction::Activate => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ActiveCellChanged) => {
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
            Some(ReaperTargetType::PlaytimeColumnAction)
        }

        fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
            if !matches!(self.action, PlaytimeColumnAction::Stop) {
                return None;
            }
            let t = RealTimePlaytimeColumnTarget {
                column_index: self.column_index,
                action: self.action,
            };
            Some(RealTimeReaperTarget::PlaytimeColumn(t))
        }

        fn is_available(&self, _: ControlContext) -> bool {
            true
        }
    }

    fn control_type_and_character(action: PlaytimeColumnAction) -> (ControlType, TargetCharacter) {
        use PlaytimeColumnAction::*;
        match action {
            Stop | Activate => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            ArmState | ArmStateExclusive => {
                (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
            }
        }
    }

    impl RealTimePlaytimeColumnTarget {
        pub fn hit(
            &mut self,
            value: ControlValue,
            context: RealTimeControlContext,
        ) -> Result<(), &'static str> {
            match self.action {
                PlaytimeColumnAction::Stop => {
                    if !value.is_on() {
                        return Ok(());
                    }
                    let matrix = context.clip_matrix()?;
                    let matrix = matrix.lock();
                    matrix.stop_column(self.column_index)
                }
                PlaytimeColumnAction::ArmState
                | PlaytimeColumnAction::ArmStateExclusive
                | PlaytimeColumnAction::Activate => Err("real-time control not supported"),
            }
        }
    }
}
