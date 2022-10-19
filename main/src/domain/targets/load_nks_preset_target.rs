use crate::domain::nks::{preset_db, with_preset_db, NksFile, Preset, PresetId};
use crate::domain::{
    BackboneState, Compartment, ControlContext, ExtendedProcessorContext, FxDescriptor,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use reaper_high::{Fx, Project, Reaper, Track};

#[derive(Debug)]
pub struct UnresolvededLoadNksPresetTarget {
    pub fx_descriptor: FxDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvededLoadNksPresetTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let fxs = self
            .fx_descriptor
            .resolve(context, compartment)?
            .into_iter()
            .map(|fx| ReaperTarget::LoadNksPreset(LoadNksPresetTarget { fx }))
            .collect();
        Ok(fxs)
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Eq, PartialEq)]
pub struct LoadNksPresetTarget {
    pub fx: Fx,
}

impl RealearnTarget for LoadNksPresetTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        let preset_id = self.current_preset_id().ok_or("no preset selected")?;
        let preset =
            with_preset_db(|db| db.find_preset_by_id(preset_id))?.ok_or("preset not found")?;
        match preset.file_ext.as_str() {
            "nksf" => {
                self.load_nksf(&preset)?;
            }
            _ => return Err("unsupported preset format"),
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        preset_db().is_ok() && self.current_preset_id().is_some() && self.fx.is_available()
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
        Some(ReaperTargetType::LoadNksPreset)
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for LoadNksPresetTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl LoadNksPresetTarget {
    fn current_preset_id(&self) -> Option<PresetId> {
        BackboneState::target_state()
            .borrow()
            .nks_state()
            .preset_id()
    }

    fn load_nksf(&self, preset: &Preset) -> Result<(), &'static str> {
        let nks_file = NksFile::load(&preset.file_name)?;
        let nks_content = nks_file.content()?;
        self.make_sure_fx_has_correct_type(nks_content.vst_magic_number)?;
        // Set VST chunk (this is beyond ugly)
        let fx = if self.fx.guid().is_some() {
            self.fx.clone()
        } else {
            let guid = self.fx.get_or_query_guid()?;
            self.fx.chain().fx_by_guid_and_index(&guid, self.fx.index())
        };
        fx.set_vst_chunk(nks_content.vst_chunk)?;
        Ok(())
    }

    fn make_sure_fx_has_correct_type(&self, vst_magic_number: u32) -> Result<(), &'static str> {
        if !self.fx.is_available() {
            return Err("FX not available");
        }
        let vst_file_name = Reaper::get()
            .find_vst_file_name_by_vst_magic_number(vst_magic_number)
            .ok_or("plug-in not installed (needs VST2 version)")?;
        let fx_info = self.fx.info()?;
        if fx_info.file_name != vst_file_name {
            // We don't have the right plug-in type. Remove FX and insert correct one.
            let chain = self.fx.chain();
            let fx_index = self.fx.index();
            chain.remove_fx(&self.fx)?;
            chain.insert_fx_by_name(fx_index, vst_file_name.to_string_lossy().as_ref());
        }
        Ok(())
    }
}

pub const LOAD_NKS_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    name: "NKS: Load preset",
    short_name: "Load NKS preset",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};
