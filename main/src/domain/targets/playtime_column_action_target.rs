use crate::domain::{
    format_value_as_on_off, Backbone, CompartmentKind, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealTimeControlContext,
    RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, VirtualPlaytimeColumn, DEFAULT_TARGET,
};
use helgoboss_learn::Target;
use realearn_api::persistence::ClipColumnAction;

pub const PLAYTIME_COLUMN_TARGET: TargetTypeDef = TargetTypeDef {
    lua_only: true,
    section: TargetSection::Playtime,
    name: "Column action",
    short_name: "Playtime column action",
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

#[derive(Debug)]
pub struct UnresolvedPlaytimeColumnActionTarget {
    pub column: VirtualPlaytimeColumn,
    pub action: ClipColumnAction,
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
    pub action: ClipColumnAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealTimeClipColumnTarget {
    column_index: usize,
    action: ClipColumnAction,
}

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{
        ControlContext, PlaytimeColumnActionTarget, RealTimeClipColumnTarget,
        RealTimeControlContext, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeColumnActionTarget {}
    impl<'a> Target<'a> for PlaytimeColumnActionTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimeClipColumnTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimeClipColumnTarget {
        pub fn hit(
            &mut self,
            value: ControlValue,
            context: RealTimeControlContext,
        ) -> Result<(), &'static str> {
            Err("Playtime not available")
        }
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        format_value_as_on_off, Backbone, CompoundChangeEvent, ControlContext, HitResponse,
        MappingControlContext, PlaytimeColumnActionTarget, RealTimeClipColumnTarget,
        RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTargetType,
        TargetCharacter,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
    use playtime_api::persistence::ColumnAddress;
    use playtime_clip_engine::base::ClipMatrixEvent;
    use playtime_clip_engine::rt::{QualifiedSlotChangeEvent, SlotChangeEvent};
    use realearn_api::persistence::ClipColumnAction;
    use std::borrow::Cow;

    impl<'a> Target<'a> for PlaytimeColumnActionTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let is_on = Backbone::get()
                .with_clip_matrix(&context.instance(), |matrix| match self.action {
                    ClipColumnAction::Stop => matrix.column_is_stoppable(self.column_index),
                })
                .ok()?;
            Some(AbsoluteValue::from_bool(is_on))
        }

        fn control_type(&self, context: Self::Context) -> ControlType {
            self.control_type_and_character(context).0
        }
    }

    impl<'a> Target<'a> for RealTimeClipColumnTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            match self.action {
                ClipColumnAction::Stop => {
                    let matrix = context.clip_matrix().ok()?;
                    let matrix = matrix.lock();
                    let is_stoppable = matrix.column_is_stoppable(self.column_index);
                    Some(AbsoluteValue::from_bool(is_stoppable))
                }
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
                .with_clip_matrix(
                    &context.control_context.instance(),
                    |matrix| -> anyhow::Result<HitResponse> {
                        match self.action {
                            ClipColumnAction::Stop => {
                                if !value.is_on() {
                                    return Ok(HitResponse::ignored());
                                }
                                matrix.stop_column(self.column_index, None)?;
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
            match self.action {
                ClipColumnAction::Stop => match evt {
                    CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::EverythingChanged) => {
                        (true, None)
                    }
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
            }
        }

        fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
            Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeColumnAction)
        }

        fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
            if !matches!(self.action, ClipColumnAction::Stop) {
                return None;
            }
            let t = RealTimeClipColumnTarget {
                column_index: self.column_index,
                action: self.action,
            };
            Some(RealTimeReaperTarget::ClipColumn(t))
        }

        fn is_available(&self, _: ControlContext) -> bool {
            true
        }
    }

    fn control_type_and_character(action: ClipColumnAction) -> (ControlType, TargetCharacter) {
        use ClipColumnAction::*;
        match action {
            Stop => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
        }
    }

    impl RealTimeClipColumnTarget {
        pub fn hit(
            &mut self,
            value: ControlValue,
            context: RealTimeControlContext,
        ) -> Result<(), &'static str> {
            match self.action {
                ClipColumnAction::Stop => {
                    if !value.is_on() {
                        return Ok(());
                    }
                    let matrix = context.clip_matrix()?;
                    let matrix = matrix.lock();
                    matrix.stop_column(self.column_index, None)
                }
            }
        }
    }
}
