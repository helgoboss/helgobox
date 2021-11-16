use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_value_as_on_off, AdditionalFeedbackEvent, BackboneState, CompoundChangeEvent,
    ControlContext, HitInstructionReturnValue, MappingControlContext, RealearnTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Fx, Project, Track};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct LoadFxSnapshotTarget {
    pub fx: Fx,
    pub chunk: Rc<String>,
    pub chunk_hash: u64,
}

impl RealearnTarget for LoadFxSnapshotTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn format_value(&self, _: UnitValue, _: ControlContext) -> String {
        "".to_owned()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if !value.to_unit_value()?.is_zero() {
            BackboneState::target_context()
                .borrow_mut()
                .load_fx_snapshot(self.fx.clone(), &self.chunk, self.chunk_hash)?
        }
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
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

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            // We can't provide a value from the event itself because it's on/off depending on
            // the mappings which use the FX snapshot target with that FX and which chunk (hash)
            // their snapshot has.
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::FxSnapshotLoaded(e))
                if e.fx == self.fx =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::LoadFxSnapshot)
    }
}

impl<'a> Target<'a> for LoadFxSnapshotTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let is_loaded = BackboneState::target_context()
            .borrow()
            .current_fx_snapshot_chunk_hash(&self.fx)
            == Some(self.chunk_hash);
        Some(AbsoluteValue::Continuous(convert_bool_to_unit_value(
            is_loaded,
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const LOAD_FX_SNAPSHOT_TARGET: TargetTypeDef = TargetTypeDef {
    short_name: "Load FX snapshot",
    supports_track: true,
    supports_fx: true,
    ..DEFAULT_TARGET
};
