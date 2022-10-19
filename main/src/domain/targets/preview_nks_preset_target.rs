use crate::domain::nks::{preset_db, PresetId};
use crate::domain::{
    nks::with_preset_db, BackboneState, Compartment, ControlContext, ExtendedProcessorContext,
    HitResponse, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    SoundPlayer, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Debug)]
pub struct UnresolvededPreviewNksPresetTarget {}

impl UnresolvedReaperTargetDef for UnresolvededPreviewNksPresetTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::PreviewNksPreset(
            PreviewNksPresetTarget {
                sound_player: SoundPlayer::new(),
            },
        )])
    }
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Eq, PartialEq)]
pub struct PreviewNksPresetTarget {
    #[derivative(PartialEq = "ignore")]
    sound_player: SoundPlayer,
}

impl RealearnTarget for PreviewNksPresetTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        if value.is_on() {
            let preset_id = self.current_preset_id().ok_or("no NKS preset selected")?;
            let preview_file = with_preset_db(|db| db.find_preset_preview_file(preset_id))?
                .ok_or("couldn't find preset or build preset preview file")?;
            self.sound_player.load_file(&preview_file)?;
            self.sound_player.play()?;
            Ok(HitResponse::processed_with_effect())
        } else {
            self.sound_player.stop()?;
            Ok(HitResponse::processed_with_effect())
        }
    }

    fn is_available(&self, _: ControlContext) -> bool {
        preset_db().is_ok() && self.current_preset_id().is_some()
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::PreviewNksPreset)
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for PreviewNksPresetTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl PreviewNksPresetTarget {
    fn current_preset_id(&self) -> Option<PresetId> {
        BackboneState::target_state()
            .borrow()
            .nks_state()
            .preset_id()
    }
}
pub const PREVIEW_NKS_PRESET_TARGET: TargetTypeDef = TargetTypeDef {
    name: "NKS: Preview preset",
    short_name: "Preview NKS preset",
    ..DEFAULT_TARGET
};
