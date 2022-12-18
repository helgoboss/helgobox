use crate::domain::{
    BackboneState, Compartment, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealTimeControlContext, RealTimeReaperTarget, RealearnClipMatrix,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualClipRow, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use realearn_api::persistence::ClipRowAction;

#[derive(Debug)]
pub struct UnresolvedClipRowTarget {
    pub row: VirtualClipRow,
    pub action: ClipRowAction,
}

impl UnresolvedReaperTargetDef for UnresolvedClipRowTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
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

impl ClipRowTarget {
    fn with_matrix<R>(
        &self,
        context: ControlContext,
        f: impl FnOnce(&mut RealearnClipMatrix) -> R,
    ) -> Result<R, &'static str> {
        BackboneState::get().with_clip_matrix_mut(context.instance_state, f)
    }
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
    ) -> Result<HitResponse, &'static str> {
        match self.basics.action {
            ClipRowAction::PlayScene => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context.control_context, |matrix| {
                    matrix.play_scene(self.basics.row_index);
                })?;
                Ok(HitResponse::processed_with_effect())
            }
            ClipRowAction::BuildScene => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context.control_context, |matrix| {
                    matrix.build_scene(self.basics.row_index)?;
                    Ok(HitResponse::processed_with_effect())
                })?
            }
            ClipRowAction::CopyOrPasteScene => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                let clips_in_scene: Vec<_> = self
                    .with_matrix(context.control_context, |matrix| {
                        matrix.all_clips_in_scene(self.basics.row_index).collect()
                    })?;
                if clips_in_scene.is_empty() {
                    // Row is empty. Check if there's something to paste.
                    let copied_clips_in_row = context
                        .control_context
                        .instance_state
                        .borrow()
                        .copied_clips_in_row()
                        .to_vec();
                    if copied_clips_in_row.is_empty() {
                        return Err("no clips to paste");
                    }
                    self.with_matrix(context.control_context, |matrix| {
                        matrix.fill_row_with_clips(self.basics.row_index, copied_clips_in_row)?;
                        Ok(HitResponse::processed_with_effect())
                    })?
                } else {
                    // Row has clips. Copy them.
                    let mut instance_state = context.control_context.instance_state.borrow_mut();
                    instance_state.copy_clips_in_row(clips_in_scene);
                    Ok(HitResponse::processed_with_effect())
                }
            }
            ClipRowAction::ClearScene => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context.control_context, |matrix| {
                    matrix.clear_scene(self.basics.row_index)?;
                    Ok(HitResponse::processed_with_effect())
                })?
            }
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipRow)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        if !matches!(self.basics.action, ClipRowAction::PlayScene) {
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
        match self.basics.action {
            ClipRowAction::PlayScene => false,
            ClipRowAction::BuildScene => false,
            ClipRowAction::CopyOrPasteScene => true,
            ClipRowAction::ClearScene => true,
        }
    }
}

impl<'a> Target<'a> for ClipRowTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        use ClipRowAction::*;
        match self.basics.action {
            PlayScene => None,
            BuildScene => None,
            CopyOrPasteScene | ClearScene => {
                let row_is_empty = self
                    .with_matrix(context, |matrix| {
                        matrix.scene_is_empty(self.basics.row_index)
                    })
                    .ok()?;
                Some(AbsoluteValue::from_bool(!row_is_empty))
            }
        }
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
            ClipRowAction::PlayScene => {
                if !value.is_on() {
                    return Ok(());
                }
                let matrix = context.clip_matrix()?;
                let matrix = matrix.lock();
                matrix.play_row(self.basics.row_index);
                Ok(())
            }
            _ => Err("only row-play is supported in real-time"),
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
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

fn control_type_and_character(_action: ClipRowAction) -> (ControlType, TargetCharacter) {
    (
        ControlType::AbsoluteContinuousRetriggerable,
        TargetCharacter::Trigger,
    )
}
