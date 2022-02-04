use crate::domain::ui_util::{
    format_value_as_db, format_value_as_db_without_unit, parse_value_from_db,
    reaper_volume_unit_value, volume_unit_value,
};
use crate::domain::{
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitInstructionReturnValue,
    InstanceStateChanged, MappingCompartment, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use playtime_clip_engine::ClipChangedEvent;
use reaper_high::Volume;

#[derive(Debug)]
pub struct UnresolvedClipVolumeTarget {
    pub slot_index: usize,
}

impl UnresolvedReaperTargetDef for UnresolvedClipVolumeTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::ClipVolume(ClipVolumeTarget {
            slot_index: self.slot_index,
        })])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipVolumeTarget {
    pub slot_index: usize,
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
        let volume = Volume::try_from_soft_normalized_value(value.to_unit_value()?.get());
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        instance_state.clip_matrix_mut().set_clip_volume_legacy(
            self.slot_index,
            volume.unwrap_or(Volume::MIN).reaper_value(),
        )?;
        Ok(None)
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
            CompoundChangeEvent::Instance(InstanceStateChanged::Clip {
                slot_index: si,
                event: ClipChangedEvent::ClipVolume(new_value),
            }) if *si == self.slot_index => (
                true,
                Some(AbsoluteValue::Continuous(reaper_volume_unit_value(
                    *new_value,
                ))),
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
        let instance_state = context.instance_state.borrow();
        let reaper_volume = instance_state
            .clip_matrix()
            .with_slot_legacy(self.slot_index, |slot| Ok(slot.clip()?.volume()))
            .ok()?;
        Some(Volume::from_reaper_value(reaper_volume))
    }
}

impl<'a> Target<'a> for ClipVolumeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let volume = self.volume(context)?;
        Some(AbsoluteValue::Continuous(volume_unit_value(volume)))
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
