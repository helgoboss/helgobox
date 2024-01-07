use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    Backbone, Compartment, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipSlot, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target};
use playtime_api::persistence::SlotAddress;
use playtime_clip_engine::base::ClipAddress;
use realearn_api::persistence::ClipManagementAction;

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
            action: self.action.clone(),
        };
        Ok(vec![ReaperTarget::ClipManagement(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualClipSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipManagementTarget {
    pub slot_coordinates: SlotAddress,
    pub action: ClipManagementAction,
}

impl ClipManagementTarget {
    fn hit_internal(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> anyhow::Result<HitResponse> {
        use ClipManagementAction as A;
        match &self.action {
            A::ClearSlot => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context, |matrix| {
                    matrix.clear_slot(self.slot_coordinates)?;
                    Ok(HitResponse::processed_with_effect())
                })?
            }
            A::FillSlotWithSelectedItem => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context, |matrix| {
                    matrix.import_selected_items(self.slot_coordinates)?;
                    Ok(HitResponse::processed_with_effect())
                })?
            }
            A::EditClip => self.with_matrix(context, |matrix| {
                let clip_address = ClipAddress::new(self.slot_coordinates, 0);
                if value.is_on() {
                    matrix.start_editing_clip(clip_address)?;
                } else {
                    matrix.stop_editing_clip(clip_address)?;
                }
                Ok(HitResponse::processed_with_effect())
            })?,
            A::AdjustClipSectionLength(a) => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context, |matrix| {
                    matrix.adjust_slot_dynamic_section_length(self.slot_coordinates, a.factor)?;
                    Ok(HitResponse::processed_with_effect())
                })?
            }
            A::CopyOrPasteClip => {
                if !value.is_on() {
                    return Ok(HitResponse::ignored());
                }
                self.with_matrix(context, |matrix| {
                    if matrix.slot_is_empty(self.slot_coordinates) {
                        matrix.paste_slot(self.slot_coordinates)?;
                    } else {
                        matrix.copy_slot(self.slot_coordinates)?;
                    }
                    Ok(HitResponse::processed_with_effect())
                })?
            }
        }
    }

    fn with_matrix<R>(
        &self,
        context: MappingControlContext,
        f: impl FnOnce(&mut playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        Backbone::get().with_clip_matrix_mut(&context.control_context.instance(), f)
    }
}

impl RealearnTarget for ClipManagementTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        use ClipManagementAction as A;
        match self.action {
            A::ClearSlot
            | A::FillSlotWithSelectedItem
            | A::CopyOrPasteClip
            | A::AdjustClipSectionLength(_) => (
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
    ) -> Result<HitResponse, &'static str> {
        self.hit_internal(value, context)
            .map_err(|_| "couldn't carry out clip management action")
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipManagement)
    }

    // TODO-high-clip-engine Return clip as result of clip() function for all clip targets (just like track())
    //  and make this property available in all clip targets.
    // TODO-high-clip-engine Also add a "Clip" target, just like "Track" target
    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        match key {
            "clip.name" => Backbone::get()
                .with_clip_matrix_mut(&context.instance(), |matrix| {
                    let clip = matrix.find_slot(self.slot_coordinates)?.clips().next()?;
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

    fn clip_slot_address(&self) -> Option<SlotAddress> {
        Some(self.slot_coordinates)
    }
}

impl<'a> Target<'a> for ClipManagementTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        use ClipManagementAction as A;
        match self.action {
            A::ClearSlot
            | A::FillSlotWithSelectedItem
            | A::CopyOrPasteClip
            | A::AdjustClipSectionLength(_) => Some(AbsoluteValue::default()),
            A::EditClip => Backbone::get()
                .with_clip_matrix(&context.instance(), |matrix| {
                    let clip_address = ClipAddress::new(self.slot_coordinates, 0);
                    let is_editing = matrix.is_editing_clip(clip_address);
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
    lua_only: true,
    section: TargetSection::Playtime,
    name: "Clip - Management",
    short_name: "Clip management",
    supports_clip_slot: true,
    ..DEFAULT_TARGET
};
