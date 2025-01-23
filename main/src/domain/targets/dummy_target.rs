use crate::domain::{
    CompartmentKind, ControlContext, ExtendedProcessorContext, HitResponse, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};

#[derive(Debug)]
pub struct UnresolvedDummyTarget;

impl UnresolvedReaperTargetDef for UnresolvedDummyTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::Dummy(DummyTarget::new())])
    }

    fn can_be_affected_by_change_events(&self) -> bool {
        // We don't want to be refreshed because we maintain an artificial value.
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct DummyTarget {
    // For making basic toggle/relative control possible.
    artificial_value: AbsoluteValue,
}

impl DummyTarget {
    pub fn new() -> Self {
        Self::default()
    }

    fn control_type_and_character_simple(&self) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Continuous,
        )
    }
}

impl RealearnTarget for DummyTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        self.control_type_and_character_simple()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_absolute_value()?;
        self.artificial_value = value;
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Dummy)
    }
}

impl Target<'_> for DummyTarget {
    type Context = ();

    fn current_value(&self, _context: ()) -> Option<AbsoluteValue> {
        Some(self.artificial_value)
    }

    fn control_type(&self, _: Self::Context) -> ControlType {
        self.control_type_and_character_simple().0
    }
}

pub const DUMMY_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::ReaLearn,
    name: "Dummy target",
    short_name: "Dummy",
    ..DEFAULT_TARGET
};
