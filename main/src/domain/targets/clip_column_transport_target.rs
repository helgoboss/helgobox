use crate::domain::{
    format_value_as_on_off, BackboneState, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitInstructionReturnValue, MappingCompartment, MappingControlContext,
    RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipColumn, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use playtime_clip_engine::main::ClipMatrixEvent;
use playtime_clip_engine::rt::{ClipChangedEvent, QualifiedClipChangedEvent};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedClipColumnTransportTarget {
    pub column: VirtualClipColumn,
}

impl UnresolvedReaperTargetDef for UnresolvedClipColumnTransportTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipColumnTransportTarget {
            column_index: self.column.resolve(context, compartment)?,
        };
        Ok(vec![ReaperTarget::ClipColumnTransport(target)])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipColumnTransportTarget {
    pub column_index: usize,
}

impl RealearnTarget for ClipColumnTransportTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (CONTROL_TYPE, TARGET_CHARACTER)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
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

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
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
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipColumnTransport)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        let t = RealTimeClipColumnTransportTarget {
            column_index: self.column_index,
        };
        Some(RealTimeReaperTarget::ClipColumnTransport(t))
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }
}

impl<'a> Target<'a> for ClipColumnTransportTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                let is_playing_something = matrix.column_is_playing_something(self.column_index);
                Some(AbsoluteValue::from_bool(is_playing_something))
            })
            .ok()?
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeClipColumnTransportTarget {
    column_index: usize,
}

impl RealTimeClipColumnTransportTarget {
    pub fn hit(
        &mut self,
        value: ControlValue,
        context: RealTimeControlContext,
    ) -> Result<(), &'static str> {
        if !value.is_on() {
            return Ok(());
        }
        let matrix = context.clip_matrix()?;
        let matrix = matrix.lock();
        matrix.stop_column(self.column_index)
    }
}

impl<'a> Target<'a> for RealTimeClipColumnTransportTarget {
    type Context = RealTimeControlContext<'a>;

    fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
        let matrix = context.clip_matrix().ok()?;
        let matrix = matrix.lock();
        let column = matrix.column(self.column_index).ok()?;
        let column = column.lock();
        let is_playing_something = column.is_playing_something();
        Some(AbsoluteValue::from_bool(is_playing_something))
    }

    fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
        CONTROL_TYPE
    }
}

pub const CLIP_COLUMN_TRANSPORT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip column: Invoke stop action",
    short_name: "Clip column stop",
    ..DEFAULT_TARGET
};

const CONTROL_TYPE: ControlType = ControlType::AbsoluteContinuousRetriggerable;
const TARGET_CHARACTER: TargetCharacter = TargetCharacter::Trigger;
