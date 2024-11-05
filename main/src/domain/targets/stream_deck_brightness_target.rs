use crate::domain::{
    Backbone, CompartmentKind, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target};
use std::fmt::Debug;

#[derive(Debug)]
pub struct UnresolvedStreamDeckBrightnessTarget {}

impl UnresolvedReaperTargetDef for UnresolvedStreamDeckBrightnessTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = StreamDeckBrightnessTarget {};
        Ok(vec![ReaperTarget::StreamDeckBrightness(target)])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamDeckBrightnessTarget {}

impl RealearnTarget for StreamDeckBrightnessTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let dev_id = context
            .control_context
            .stream_deck_dev_id
            .ok_or("no stream deck device selected")?;
        Backbone::get()
            .set_stream_deck_brightness(dev_id, value.to_unit_value()?)
            .map_err(|_| "couldn't set brightness")?;
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::StreamDeckBrightness)
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }

    fn can_report_current_value(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for StreamDeckBrightnessTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const STREAM_DECK_BRIGHTNESS_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::StreamDeck,
    name: "Brightness",
    short_name: "Brightness",
    ..DEFAULT_TARGET
};
