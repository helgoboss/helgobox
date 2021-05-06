use crate::domain::ui_util::{
    convert_bool_to_unit_value, fx_parameter_unit_value, parse_unit_value_from_percentage,
};
use crate::domain::{
    format_value_as_on_off, fx_enable_unit_value, AdditionalFeedbackEvent, BackboneState,
    ControlContext, FxDisplayType, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, FxParameter, FxParameterCharacter, Project, Track};
use reaper_medium::{FxChainVisibility, GetParameterStepSizesResult, ReaperNormalizedFxParamValue};
use std::convert::TryInto;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct LoadFxSnapshotTarget {
    pub fx: Fx,
    pub chunk: Rc<String>,
    pub chunk_hash: u64,
}

impl RealearnTarget for LoadFxSnapshotTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn format_value(&self, value: UnitValue) -> String {
        "".to_owned()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if !value.as_absolute()?.is_zero() {
            BackboneState::target_context()
                .borrow_mut()
                .load_fx_snapshot(self.fx.clone(), &self.chunk, self.chunk_hash)?
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.fx.is_available()
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

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            // We can't provide a value from the event itself because it's on/off depending on
            // the mappings which use the FX snapshot target with that FX and which chunk (hash)
            // their snapshot has.
            AdditionalFeedbackEvent::FxSnapshotLoaded(e) if e.fx == self.fx => (true, None),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for LoadFxSnapshotTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<UnitValue> {
        let is_loaded = BackboneState::target_context()
            .borrow()
            .current_fx_snapshot_chunk_hash(&self.fx)
            == Some(self.chunk_hash);
        Some(convert_bool_to_unit_value(is_loaded))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
