use crate::domain::pot::nks::NksFile;
use crate::domain::pot::{preset_db, with_preset_db, Preset, PresetId, RuntimePotUnit};
use crate::domain::{
    BackboneState, Compartment, ControlContext, ExtendedProcessorContext, FxDescriptor,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use reaper_high::{Fx, Project, Reaper, Track};
use reaper_medium::InsertMediaMode;
use std::path::Path;

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
        let preset_id = self
            .current_preset_id(&pot_unit)
            .ok_or("no preset selected")?;
        let preset =
            with_preset_db(|db| db.find_preset_by_id(preset_id))?.ok_or("preset not found")?;
        match preset.file_ext.as_str() {
            "wav" | "aif" => {
                self.load_audio(&preset)?;
            }
            "nksf" => {
                self.load_nksf(&preset)?;
            }
            _ => return Err("unsupported preset format"),
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, context: ControlContext) -> bool {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return false,
        };
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

    fn load_nksf(&self, preset: &Preset) -> Result<(), &'static str> {
        let nks_file = NksFile::load(&preset.file_name)?;
        let nks_content = nks_file.content()?;
        // self.make_sure_fx_has_correct_type(nks_content.vst_magic_number)?;
        self.fx.set_vst_chunk(nks_content.vst_chunk)?;
        BackboneState::target_state()
            .borrow_mut()
            .set_current_fx_preset(self.fx.clone(), nks_content.current_preset);
        Ok(())
    }

    fn load_audio(&self, preset: &Preset) -> Result<(), &'static str> {
        const RS5K_VST_ID: u32 = 1920167789;
        self.make_sure_fx_has_correct_type(RS5K_VST_ID)?;
        let window_is_open_before = self.fx.window_is_open();
        if window_is_open_before {
            if !self.fx.window_has_focus() {
                self.fx.hide_floating_window();
                self.fx.show_in_floating_window();
            }
        } else {
            self.fx.show_in_floating_window();
        }
        load_media_in_last_focused_rs5k(&preset.file_name)?;
        if !window_is_open_before {
            self.fx.hide_floating_window();
        }
        Ok(())
    }

    fn make_sure_fx_has_correct_type(&self, vst_magic_number: u32) -> Result<(), &'static str> {
        if !self.fx.is_available() {
            return Err("FX not available");
        }
        let fx_info = self.fx.info()?;
        if fx_info.id != vst_magic_number.to_string() {
            // We don't have the right plug-in type. Remove FX and insert correct one.
            let chain = self.fx.chain();
            let fx_index = self.fx.index();
            chain.remove_fx(&self.fx)?;
            // Need to put some random string in front of "<" due to bug in REAPER < 6.69,
            // otherwise loading by VST2 magic number doesn't work.
            chain.insert_fx_by_name(fx_index, format!("i7zh34z<{}", vst_magic_number));
        }
        Ok(())
    }
}

pub const LOAD_POT_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Pot: Load preset",
    short_name: "Load Pot preset",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};

fn load_media_in_last_focused_rs5k(path: &Path) -> Result<(), &'static str> {
    Reaper::get().medium_reaper().insert_media(
        path,
        InsertMediaMode::CurrentReasamplomatic,
        Default::default(),
    )?;
    Ok(())
}
