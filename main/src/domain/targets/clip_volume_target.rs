use crate::domain::ui_util::{
    db_unit_value, format_value_as_db, format_value_as_db_without_unit, parse_value_from_db,
    volume_unit_value,
};
use crate::domain::{
    interpret_current_clip_slot_value, BackboneState, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitInstructionReturnValue, MappingCompartment, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualClipSlot, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use playtime_clip_engine::main::{ClipMatrixEvent, ClipSlotCoordinates};
use playtime_clip_engine::rt::{ClipChangedEvent, QualifiedClipChangedEvent};
use reaper_high::Volume;
use reaper_medium::Db;

#[derive(Debug)]
pub struct UnresolvedClipVolumeTarget {
    pub slot: VirtualClipSlot,
}

impl UnresolvedReaperTargetDef for UnresolvedClipVolumeTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::ClipVolume(ClipVolumeTarget {
            slot_coordinates: self.slot.resolve(context, compartment)?,
        })])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualClipSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipVolumeTarget {
    pub slot_coordinates: ClipSlotCoordinates,
}

impl RealearnTarget for ClipVolumeTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db_without_unit(value)
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "dB"
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let volume = Volume::try_from_soft_normalized_value(value.to_unit_value()?.get())
            .unwrap_or_default();
        let db = volume.db();
        let api_db = playtime_api::Db::new(db.get())?;
        BackboneState::get().with_clip_matrix_mut(
            context.control_context.instance_state,
            |matrix| {
                matrix.set_clip_volume(self.slot_coordinates, api_db)?;
                Ok(None)
            },
        )?
    }

    fn is_available(&self, _: ControlContext) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        true
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                QualifiedClipChangedEvent {
                    slot_coordinates: si,
                    event: ClipChangedEvent::ClipVolume(new_value),
                },
            )) if *si == self.slot_coordinates => (
                true,
                Some(AbsoluteValue::Continuous(db_unit_value(Db::new(
                    new_value.get(),
                )))),
            ),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(self.volume(context)?.to_string())
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.volume(context)?.db().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipVolume)
    }
}

impl ClipVolumeTarget {
    fn volume(&self, context: ControlContext) -> Option<Volume> {
        BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                let db = matrix.clip_volume(self.slot_coordinates).ok()?;
                Some(Volume::from_db(Db::new(db.get())))
            })
            .ok()?
    }
}

impl<'a> Target<'a> for ClipVolumeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let val = self
            .volume(context)
            .map(volume_unit_value)
            .map(AbsoluteValue::Continuous);
        interpret_current_clip_slot_value(val)
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const CLIP_VOLUME_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Volume",
    short_name: "Clip volume",
    supports_slot: true,
    ..DEFAULT_TARGET
};
