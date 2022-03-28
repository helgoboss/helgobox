use crate::domain::{
    BackboneState, ControlContext, ExtendedProcessorContext, HitInstructionReturnValue,
    MappingCompartment, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipSlot, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
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
        compartment: MappingCompartment,
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

impl RealearnTarget for ClipManagementTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
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
                match self.action {
                    ClipManagementAction::ClearSlot => {
                        matrix.clear_slot(self.slot_coordinates)?;
                    }
                    ClipManagementAction::FillSlotWithSelectedItem => {
                        matrix.fill_slot_with_selected_item(self.slot_coordinates)?;
                    }
                }
                Ok(None)
            },
        )?
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipManagement)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }
}

impl<'a> Target<'a> for ClipManagementTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: ControlContext<'a>) -> Option<AbsoluteValue> {
        Some(AbsoluteValue::default())
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const CLIP_MANAGEMENT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Management",
    short_name: "Clip management",
    supports_slot: true,
    ..DEFAULT_TARGET
};
