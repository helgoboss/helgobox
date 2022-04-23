use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    BackboneState, Compartment, ControlContext, ExtendedProcessorContext,
    HitInstructionReturnValue, MappingControlContext, RealearnClipMatrix, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef,
    VirtualClipSlot, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target};
use playtime_clip_engine::main::ClipSlotCoordinates;
use realearn_api::schema::ClipManagementAction;

#[derive(Debug)]
pub struct UnresolvedClipManagementTarget {
    pub slot: VirtualClipSlot,
    pub action: ClipManagementAction,
}

impl UnresolvedReaperTargetDef for UnresolvedClipManagementTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipManagementTarget {
            slot_coordinates: self.slot.resolve(context, compartment)?,
            action: self.action,
        };
        Ok(vec![ReaperTarget::ClipManagement(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualClipSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipManagementTarget {
    pub slot_coordinates: ClipSlotCoordinates,
    pub action: ClipManagementAction,
}

impl ClipManagementTarget {
    fn with_matrix<R>(
        &self,
        context: MappingControlContext,
        f: impl FnOnce(&mut RealearnClipMatrix) -> R,
    ) -> Result<R, &'static str> {
        BackboneState::get().with_clip_matrix_mut(context.control_context.instance_state, f)
    }
}

impl RealearnTarget for ClipManagementTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        use ClipManagementAction as A;
        match self.action {
            A::ClearSlot | A::FillSlotWithSelectedItem | A::CopyOrPasteClip => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            A::EditClip => (ControlType::AbsoluteContinuous, TargetCharacter::Switch),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        use ClipManagementAction as A;
        match self.action {
            A::ClearSlot => {
                if !value.is_on() {
                    return Ok(None);
                }
                self.with_matrix(context, |matrix| {
                    matrix.clear_slot(self.slot_coordinates)?;
                    Ok(None)
                })?
            }
            A::FillSlotWithSelectedItem => {
                if !value.is_on() {
                    return Ok(None);
                }
                self.with_matrix(context, |matrix| {
                    matrix.fill_slot_with_selected_item(self.slot_coordinates)?;
                    Ok(None)
                })?
            }
            A::EditClip => self.with_matrix(context, |matrix| {
                if value.is_on() {
                    matrix.start_editing_clip(self.slot_coordinates)?;
                } else {
                    matrix.stop_editing_clip(self.slot_coordinates)?;
                }
                Ok(None)
            })?,
            A::CopyOrPasteClip => {
                if !value.is_on() {
                    return Ok(None);
                }
                let clip_in_slot = self.with_matrix(context, |matrix| {
                    matrix
                        .slot(self.slot_coordinates)
                        .and_then(|s| s.clip())
                        .and_then(|clip| {
                            clip.save(context.control_context.processor_context.project())
                                .ok()
                        })
                })?;
                match clip_in_slot {
                    None => {
                        // No clip in that slot. Check if there's something to paste.
                        let copied_clip = context
                            .control_context
                            .instance_state
                            .borrow()
                            .copied_clip()
                            .ok_or("no clip available to paste")?
                            .clone();
                        self.with_matrix(context, |matrix| {
                            matrix.fill_slot_with_clip(self.slot_coordinates, copied_clip)?;
                            Ok(None)
                        })?
                    }
                    Some(api_clip) => {
                        // We have a clip in that slot. Copy it.
                        let mut instance_state =
                            context.control_context.instance_state.borrow_mut();
                        instance_state.copy_clip(api_clip);
                        Ok(None)
                    }
                }
            }
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipManagement)
    }

    // TODO-high Return clip as result of clip() function for all clip targets (just like track())
    //  and make this property available in all clip targets.
    // TODO-high Also add a "Clip" target, just like "Track" target
    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        match key {
            "clip.name" => BackboneState::get()
                .with_clip_matrix_mut(context.instance_state, |matrix| {
                    let slot = matrix.slot(self.slot_coordinates)?;
                    let clip = slot.clip()?;
                    let name = clip.name()?;
                    Some(PropValue::Text(name.to_string().into()))
                })
                .ok()?,
            _ => None,
        }
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }
}

impl<'a> Target<'a> for ClipManagementTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        use ClipManagementAction as A;
        match self.action {
            A::ClearSlot | A::FillSlotWithSelectedItem | A::CopyOrPasteClip => {
                Some(AbsoluteValue::default())
            }
            A::EditClip => BackboneState::get()
                .with_clip_matrix(context.instance_state, |matrix| {
                    let is_editing = matrix.is_editing_clip(self.slot_coordinates);
                    let value = convert_bool_to_unit_value(is_editing);
                    Some(AbsoluteValue::Continuous(value))
                })
                .ok()?,
        }
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const CLIP_MANAGEMENT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Management",
    short_name: "Clip management",
    supports_clip_slot: true,
    ..DEFAULT_TARGET
};
