use crate::domain::{
    Compartment, CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    InstanceStateChanged, MappingControlContext, PotStateChangedEvent, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use base::blocking_lock_arc;
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use pot::{preview_exists, PresetId, RuntimePotUnit};
use reaper_high::Reaper;

#[derive(Debug)]
pub struct UnresolvedPreviewPotPresetTarget {}

impl UnresolvedReaperTargetDef for UnresolvedPreviewPotPresetTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::PreviewPotPreset(
            PreviewPotPresetTarget {},
        )])
    }
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Eq, PartialEq)]
pub struct PreviewPotPresetTarget {}

impl RealearnTarget for PreviewPotPresetTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let mut instance_state = context.control_context.unit.borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let mut pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from PreviewPotPresetTarget 1");
        if value.is_on() {
            let preset_id = self
                .current_preset_id(&pot_unit)
                .ok_or("no Pot preset selected")?;
            pot_unit.play_preview(preset_id)?;
            Ok(HitResponse::processed_with_effect())
        } else {
            pot_unit.stop_preview()?;
            Ok(HitResponse::processed_with_effect())
        }
    }

    fn is_available(&self, context: ControlContext) -> bool {
        let mut instance_state = context.unit.borrow_mut();
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return false,
        };
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from PreviewPotPresetTarget 2");
        match pot_unit.find_currently_selected_preset() {
            None => false,
            Some(p) => preview_exists(&p, &Reaper::get().resource_path()),
        }
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetChanged { .. },
            )) => (true, None),
            _ => (false, None),
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::PreviewPotPreset)
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for PreviewPotPresetTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl PreviewPotPresetTarget {
    fn current_preset_id(&self, pot_unit: &RuntimePotUnit) -> Option<PresetId> {
        pot_unit.preset_id()
    }
}
pub const PREVIEW_POT_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Pot: Preview preset",
    short_name: "Preview Pot preset",
    ..DEFAULT_TARGET
};
