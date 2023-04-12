use crate::base::blocking_lock_arc;
use crate::domain::pot::{preset_db, with_preset_db, PresetId, RuntimePotUnit};
use crate::domain::{
    pot, BackboneState, Compartment, ControlContext, ExtendedProcessorContext, FxDescriptor,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use reaper_high::{Fx, Project, Track};

#[derive(Debug)]
pub struct UnresolvedLoadPotPresetTarget {
    pub fx_descriptor: FxDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedLoadPotPresetTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let fxs = self
            .fx_descriptor
            .resolve(context, compartment)?
            .into_iter()
            .map(|fx| ReaperTarget::LoadPotPreset(LoadPotPresetTarget { fx }))
            .collect();
        Ok(fxs)
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Eq, PartialEq)]
pub struct LoadPotPresetTarget {
    pub fx: Fx,
}

impl RealearnTarget for LoadPotPresetTarget {
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
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let pot_unit = blocking_lock_arc(&pot_unit);
        let preset_id = self
            .current_preset_id(&pot_unit)
            .ok_or("no preset selected")?;
        let preset =
            with_preset_db(|db| db.find_preset_by_id(preset_id))?.ok_or("preset not found")?;
        let current_preset = pot::load_preset(&preset, &self.fx)?;
        BackboneState::target_state()
            .borrow_mut()
            .set_current_fx_preset(self.fx.clone(), current_preset);
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, context: ControlContext) -> bool {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return false,
        };
        let pot_unit = blocking_lock_arc(&pot_unit);
        preset_db().is_ok() && self.current_preset_id(&pot_unit).is_some() && self.fx.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.fx.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx.track()
    }

    fn fx(&self) -> Option<&Fx> {
        Some(&self.fx)
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::LoadPotPreset)
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for LoadPotPresetTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl LoadPotPresetTarget {
    fn current_preset_id(&self, pot_unit: &RuntimePotUnit) -> Option<PresetId> {
        pot_unit.preset_id()
    }
}

pub const LOAD_POT_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Pot: Load preset",
    short_name: "Load Pot preset",
    hint: "Highly experimental!!!",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};
