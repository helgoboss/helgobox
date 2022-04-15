use crate::domain::{
    format_value_as_on_off, BackboneState, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitInstructionReturnValue, MappingCompartment, MappingControlContext,
    RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipColumn, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use playtime_clip_engine::main::ClipMatrixEvent;
use playtime_clip_engine::rt::{ClipChangedEvent, QualifiedClipChangedEvent};
use realearn_api::schema::ClipColumnAction;
use reaper_high::ChangeEvent;
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
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipColumnTarget {
            column_index: self.column.resolve(context, compartment)?,
            action: self.action,
        };
        Ok(vec![ReaperTarget::ClipColumn(target)])
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        match self.action {
            ClipColumnAction::Stop => {
                if !value.is_on() {
                    return Ok(None);
                }
                BackboneState::get().with_clip_matrix_mut(
                    context.control_context.instance_state,
                    |matrix| {
                        matrix.stop_column(self.column_index)?;
                        Ok(None)
                    },
                )?
            }
            ClipColumnAction::Arm => BackboneState::get().with_clip_matrix(
                context.control_context.instance_state,
                |matrix| {
                    matrix.set_column_armed_for_recording(self.column_index, value.is_on())?;
                    Ok(None)
                },
            )?,
        }
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match self.action {
            ClipColumnAction::Stop => match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::AllClipsChanged) => (true, None),
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                    QualifiedClipChangedEvent {
                        slot_coordinates: sc,
                        event,
                    },
                )) if sc.column() == self.column_index => match event {
                    ClipChangedEvent::PlayState(_) => (true, None),
                    ClipChangedEvent::Removed => (true, None),
                    _ => (false, None),
                },
                _ => (false, None),
            },
            ClipColumnAction::Arm => match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::AllClipsChanged) => (true, None),
                CompoundChangeEvent::Reaper(ChangeEvent::TrackArmChanged(_)) => (true, None),
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
        BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| match self.action {
                ClipColumnAction::Stop => {
                    let is_stoppable = matrix.column_is_stoppable(self.column_index);
                    Some(AbsoluteValue::from_bool(is_stoppable))
                }
                ClipColumnAction::Arm => {
                    let is_armed = matrix.column_is_armed_for_recording(self.column_index);
                    Some(AbsoluteValue::from_bool(is_armed))
                }
            })
            .ok()?
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
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
            ClipColumnAction::Arm => Err("column arm not supported as real-time action"),
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
            ClipColumnAction::Arm => None,
        }
    }

    fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
        control_type_and_character(self.action).0
    }
}

pub const CLIP_COLUMN_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip column",
    short_name: "Clip column",
    ..DEFAULT_TARGET
};

fn control_type_and_character(action: ClipColumnAction) -> (ControlType, TargetCharacter) {
    use ClipColumnAction::*;
    match action {
        Stop => (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        ),
        Arm => (ControlType::AbsoluteContinuous, TargetCharacter::Switch),
    }
}
