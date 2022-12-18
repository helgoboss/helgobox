use crate::domain::{
    format_value_as_on_off, BackboneState, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealTimeControlContext,
    RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipColumn, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use playtime_clip_engine::base::ClipMatrixEvent;
use playtime_clip_engine::rt::{QualifiedSlotChangeEvent, SlotChangeEvent};
use realearn_api::persistence::ClipColumnAction;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedClipColumnTarget {
    pub column: VirtualClipColumn,
    pub action: ClipColumnAction,
}

impl UnresolvedReaperTargetDef for UnresolvedClipColumnTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipColumnTarget {
            column_index: self.column.resolve(context, compartment)?,
            action: self.action,
        };
        Ok(vec![ReaperTarget::ClipColumn(target)])
    }

    fn clip_column_descriptor(&self) -> Option<&VirtualClipColumn> {
        Some(&self.column)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipColumnTarget {
    pub column_index: usize,
    pub action: ClipColumnAction,
}

impl RealearnTarget for ClipColumnTarget {
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
        let response = BackboneState::get().with_clip_matrix(
            context.control_context.instance_state,
            |matrix| -> Result<HitResponse, &'static str> {
                match self.action {
                    ClipColumnAction::Stop => {
                        if !value.is_on() {
                            return Ok(HitResponse::ignored());
                        }
                        matrix.stop_column(self.column_index)?;
                    }
                }
                Ok(HitResponse::processed_with_effect())
            },
        )??;
        Ok(response)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match self.action {
            ClipColumnAction::Stop => match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::EverythingChanged) => (true, None),
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
        Some(ReaperTargetType::ClipColumn)
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

impl<'a> Target<'a> for ClipColumnTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let is_on = BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| match self.action {
                ClipColumnAction::Stop => matrix.column_is_stoppable(self.column_index),
            })
            .ok()?;
        Some(AbsoluteValue::from_bool(is_on))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealTimeClipColumnTarget {
    column_index: usize,
    action: ClipColumnAction,
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
                matrix.stop_column(self.column_index)
            }
        }
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

pub const CLIP_COLUMN_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip column",
    short_name: "Clip column",
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

fn control_type_and_character(action: ClipColumnAction) -> (ControlType, TargetCharacter) {
    use ClipColumnAction::*;
    match action {
        Stop => (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        ),
    }
}
