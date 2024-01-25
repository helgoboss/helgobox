use crate::domain::{
    Backbone, CompartmentKind, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealTimeControlContext, RealTimeReaperTarget, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualPlaytimeRow, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use playtime_api::persistence::RowAddress;
use realearn_api::persistence::ClipRowAction;

#[derive(Debug)]
pub struct UnresolvedPlaytimeRowActionTarget {
    pub row: VirtualPlaytimeRow,
    pub action: ClipRowAction,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeRowActionTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = PlaytimeRowActionTarget {
            basics: ClipRowTargetBasics {
                row_index: self.row.resolve(context, compartment)?,
                action: self.action,
            },
        };
        Ok(vec![ReaperTarget::PlaytimeRowAction(target)])
    }

    fn clip_row_descriptor(&self) -> Option<&VirtualPlaytimeRow> {
        Some(&self.row)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaytimeRowActionTarget {
    pub basics: ClipRowTargetBasics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipRowTargetBasics {
    pub row_index: usize,
    pub action: ClipRowAction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeClipRowTarget {
    basics: ClipRowTargetBasics,
}

pub const PLAYTIME_ROW_TARGET: TargetTypeDef = TargetTypeDef {
    lua_only: true,
    section: TargetSection::Playtime,
    name: "Row action",
    short_name: "Playtime row action",
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {

    use crate::domain::{
        ControlContext, PlaytimeColumnActionTarget, PlaytimeRowActionTarget,
        PlaytimeSlotTransportTarget, RealTimeClipColumnTarget, RealTimeClipRowTarget,
        RealTimeControlContext, RealTimeSlotTransportTarget, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeRowActionTarget {}
    impl<'a> Target<'a> for PlaytimeRowActionTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimeClipRowTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimeClipRowTarget {
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
        Backbone, CompartmentKind, ControlContext, ExtendedProcessorContext, HitResponse,
        MappingControlContext, PlaytimeRowActionTarget, RealTimeClipRowTarget,
        RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTarget,
        ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef,
        VirtualPlaytimeRow, DEFAULT_TARGET,
    };
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
    use playtime_api::persistence::RowAddress;
    use realearn_api::persistence::ClipRowAction;

    impl PlaytimeRowActionTarget {
        fn hit_internal(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> anyhow::Result<HitResponse> {
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
                    self.with_matrix(context.control_context, |matrix| {
                        if matrix.scene_is_empty(self.basics.row_index) {
                            matrix.paste_scene(self.basics.row_index)?;
                        } else {
                            matrix.copy_scene(self.basics.row_index)?;
                        }
                        Ok(HitResponse::processed_with_effect())
                    })?
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

        fn with_matrix<R>(
            &self,
            context: ControlContext,
            f: impl FnOnce(&mut playtime_clip_engine::base::Matrix) -> R,
        ) -> anyhow::Result<R> {
            Backbone::get().with_clip_matrix_mut(&context.instance(), f)
        }
    }

    impl RealearnTarget for PlaytimeRowActionTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            control_type_and_character(self.basics.action)
        }

        fn clip_row_address(&self) -> Option<RowAddress> {
            Some(RowAddress::new(self.basics.row_index))
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            self.hit_internal(value, context)
                .map_err(|_| "couldn't carry out row action")
        }

        fn reaper_target_type(&self) -> Option<ReaperTargetType> {
            Some(ReaperTargetType::PlaytimeRowAction)
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

    impl<'a> Target<'a> for PlaytimeRowActionTarget {
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
                    matrix.play_scene(self.basics.row_index);
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

    fn control_type_and_character(_action: ClipRowAction) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }
}
