use crate::base::blocking_lock_arc;
use crate::domain::pot::{pot_db, PresetId, RuntimePotUnit};
use crate::domain::{
    Compartment, ControlContext, ExtendedProcessorContext, HitResponse, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, SoundPlayer, TargetCharacter, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use derivative::Derivative;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Debug)]
pub struct UnresolvedPreviewPotPresetTarget {}

impl UnresolvedReaperTargetDef for UnresolvedPreviewPotPresetTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::PreviewPotPreset(
            PreviewPotPresetTarget {
                sound_player: SoundPlayer::new(),
            },
        )])
    }
}

#[derive(Clone, Debug, Derivative)]
#[derivative(Eq, PartialEq)]
pub struct PreviewPotPresetTarget {
    #[derivative(PartialEq = "ignore")]
    sound_player: SoundPlayer,
}

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
        if value.is_on() {
            let mut instance_state = context.control_context.instance_state.borrow_mut();
            let pot_unit = instance_state.pot_unit()?;
            let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from PreviewPotPresetTarget 1");
            let preset_id = self
                .current_preset_id(&pot_unit)
                .ok_or("no Pot preset selected")?;
            let preview_file = pot_db()
                .find_legacy_preview_file_by_preset_id(preset_id)
                .ok_or("couldn't find preset or build preset preview file")?;
            self.sound_player.load_file(&preview_file)?;
            self.sound_player.play()?;
            Ok(HitResponse::processed_with_effect())
        } else {
            self.sound_player.stop()?;
            Ok(HitResponse::processed_with_effect())
        }
    }

    fn is_available(&self, context: ControlContext) -> bool {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return false,
        };
        let pot_unit = blocking_lock_arc(&pot_unit, "PotUnit from PreviewPotPresetTarget 2");
        self.current_preset_id(&pot_unit).is_some()
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
    hint: "Highly experimental!!!",
    ..DEFAULT_TARGET
};
