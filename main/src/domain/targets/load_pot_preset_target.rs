use crate::domain::{
    Backbone, Compartment, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    FxDescriptor, HitResponse, InstanceStateChanged, MappingControlContext, PotStateChangedEvent,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use base::blocking_lock_arc;
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target};
use pot::{pot_db, Destination, LoadPresetOptions, PotPreset};
use reaper_high::{Fx, Project, Track};
use std::borrow::Cow;

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
        let mut instance_state = context.control_context.instance().borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let mut pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from LoadPotPresetTarget 3");
        let preset_id = pot_unit.preset_id().ok_or("no preset selected")?;
        let preset = pot_db()
            .find_preset_by_id(preset_id)
            .ok_or("preset not found")?;
        let fx_index = self.fx.index();
        pot_unit
            .load_preset_at(&preset, LoadPresetOptions::default(), &|_| {
                let dest = Destination {
                    chain: self.fx.chain().clone(),
                    fx_index,
                };
                Ok(dest)
            })
            .map_err(|_| "couldn't load preset")?;
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, context: ControlContext) -> bool {
        if !self.fx.is_available() {
            return false;
        }
        let mut instance_state = context.instance().borrow_mut();
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return false,
        };
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from LoadPotPresetTarget 1");
        match pot_unit.find_currently_selected_preset() {
            None => false,
            Some(p) => p.common.is_available && p.common.is_supported,
        }
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

    fn prop_value(&self, key: &str, _: ControlContext) -> Option<PropValue> {
        self.with_loaded_preset(|p| get_preset_property(p?, key))
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        if let PropValue::Text(text) = self.prop_value("preset.name", context)? {
            Some(text)
        } else {
            None
        }
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetLoaded | PotStateChangedEvent::PresetChanged { .. },
            )) => (true, None),
            _ => (false, None),
        }
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
    fn with_loaded_preset<R>(&self, f: impl FnOnce(Option<&PotPreset>) -> R) -> R {
        match Backbone::target_state()
            .borrow()
            .current_fx_preset(&self.fx)
        {
            None => f(None),
            Some(p) => f(Some(&p.preset)),
        }
    }
}

pub const LOAD_POT_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Pot,
    name: "Load preset",
    short_name: "Load Pot preset",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};

pub fn get_preset_property(p: &PotPreset, key: &str) -> Option<PropValue> {
    let value = match key {
        "preset.name" => p.common.name.clone().into(),
        "preset.product.name" => p.common.product_name.as_ref()?.clone().into(),
        "preset.file_ext" => p.kind.file_extension()?.to_string().into(),
        "preset.author" => p.common.metadata.author.as_ref()?.clone().into(),
        "preset.vendor" => p.common.metadata.vendor.as_ref()?.clone().into(),
        "preset.comment" => p.common.metadata.comment.as_ref()?.clone().into(),
        _ => return None,
    };
    Some(value)
}
