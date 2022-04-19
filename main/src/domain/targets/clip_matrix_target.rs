use crate::domain::{
    format_value_as_on_off, BackboneState, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitInstructionReturnValue, MappingCompartment, MappingControlContext,
    RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use playtime_clip_engine::main::ClipMatrixEvent;
use playtime_clip_engine::rt::{ClipChangedEvent, QualifiedClipChangedEvent};
use realearn_api::schema::ClipMatrixAction;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedClipMatrixTarget {
    pub action: ClipMatrixAction,
}

impl UnresolvedReaperTargetDef for UnresolvedClipMatrixTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipMatrixTarget {
            action: self.action,
        };
        Ok(vec![ReaperTarget::ClipMatrix(target)])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipMatrixTarget {
    pub action: ClipMatrixAction,
}

impl RealearnTarget for ClipMatrixTarget {
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
        BackboneState::get().with_clip_matrix_mut(
            context.control_context.instance_state,
            |matrix| {
                if !value.is_on() {
                    return Ok(None);
                }
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
                }
                Ok(None)
            },
        )?
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match self.action {
            ClipMatrixAction::Stop => match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::AllClipsChanged) => (true, None),
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                    QualifiedClipChangedEvent { event, .. },
                )) => match event {
                    ClipChangedEvent::PlayState(_) => (true, None),
                    ClipChangedEvent::Removed => (true, None),
                    _ => (false, None),
                },
                _ => (false, None),
            },
            ClipMatrixAction::Undo | ClipMatrixAction::Redo => match evt {
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::AllClipsChanged) => (true, None),
                _ => (false, None),
            },
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipMatrix)
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

impl<'a> Target<'a> for ClipMatrixTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                let bool_value = match self.action {
                    ClipMatrixAction::Stop => matrix.is_stoppable(),
                    ClipMatrixAction::Undo => matrix.can_undo(),
                    ClipMatrixAction::Redo => matrix.can_redo(),
                };
                Some(AbsoluteValue::from_bool(bool_value))
            })
            .ok()?
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeClipMatrixTarget {
    action: ClipMatrixAction,
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
                matrix.stop();
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

pub const CLIP_MATRIX_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip matrix",
    short_name: "Clip matrix",
    ..DEFAULT_TARGET
};

fn control_type_and_character(action: ClipMatrixAction) -> (ControlType, TargetCharacter) {
    use ClipMatrixAction::*;
    match action {
        Stop | Undo | Redo => (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        ),
    }
}
