use crate::domain::{
    BackboneState, ControlContext, ExtendedProcessorContext, HitInstructionReturnValue,
    MappingCompartment, MappingControlContext, RealTimeControlContext, RealTimeReaperTarget,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualClipRow, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use realearn_api::schema::ClipRowAction;

#[derive(Debug)]
pub struct UnresolvedClipRowTarget {
    pub row: VirtualClipRow,
    pub action: ClipRowAction,
}

impl UnresolvedReaperTargetDef for UnresolvedClipRowTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipRowTarget {
            basics: ClipRowTargetBasics {
                row_index: self.row.resolve(context, compartment)?,
                action: self.action,
            },
        };
        Ok(vec![ReaperTarget::ClipRow(target)])
    }

    fn clip_row_descriptor(&self) -> Option<&VirtualClipRow> {
        Some(&self.row)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipRowTarget {
    basics: ClipRowTargetBasics,
}

#[derive(Clone, Debug, PartialEq)]
struct ClipRowTargetBasics {
    pub row_index: usize,
    pub action: ClipRowAction,
}

impl RealearnTarget for ClipRowTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        control_type_and_character(self.basics.action)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        BackboneState::get().with_clip_matrix(
            context.control_context.instance_state,
            |matrix| -> Result<(), &'static str> {
                match self.basics.action {
                    ClipRowAction::Play => {
                        if !value.is_on() {
                            return Ok(());
                        }
                        matrix.play_row(self.basics.row_index);
                    }
                }
                Ok(())
            },
        )??;
        Ok(None)
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipRow)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        if !matches!(self.basics.action, ClipRowAction::Play) {
            return None;
        }
        let t = RealTimeClipRowTarget {
            basics: self.basics.clone(),
        };
        Some(RealTimeReaperTarget::ClipRow(t))
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for ClipRowTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: ControlContext<'a>) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeClipRowTarget {
    basics: ClipRowTargetBasics,
}

impl RealTimeClipRowTarget {
    pub fn hit(
        &mut self,
        value: ControlValue,
        context: RealTimeControlContext,
    ) -> Result<(), &'static str> {
        match self.basics.action {
            ClipRowAction::Play => {
                if !value.is_on() {
                    return Ok(());
                }
                let matrix = context.clip_matrix()?;
                let matrix = matrix.lock();
                matrix.play_row(self.basics.row_index);
                Ok(())
            }
        }
    }
}

impl<'a> Target<'a> for RealTimeClipRowTarget {
    type Context = RealTimeControlContext<'a>;

    fn current_value(&self, _: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
        control_type_and_character(self.basics.action).0
    }
}

pub const CLIP_ROW_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip row",
    short_name: "Clip row",
    ..DEFAULT_TARGET
};

fn control_type_and_character(action: ClipRowAction) -> (ControlType, TargetCharacter) {
    use ClipRowAction::*;
    match action {
        Play => (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        ),
    }
}
