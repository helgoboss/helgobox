use crate::domain::{
    get_effective_tracks, get_track_name, percentage_for_track_within_project, Compartment,
    ControlContext, DomainEvent, ExtendedProcessorContext, HitInstruction, HitInstructionContext,
    HitInstructionReturnValue, InstanceTrackChangeRequestedEvent, MappingControlContext,
    MappingControlResult, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, TrackDescriptor, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target};
use realearn_api::persistence::TrackToolAction;
use reaper_high::{Project, Track};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackToolTarget {
    pub track_descriptor: TrackDescriptor,
    pub action: TrackToolAction,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackToolTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackTool(TrackToolTarget {
                        track,
                        action: self.action,
                    })
                })
                .collect(),
        )
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        Some(&self.track_descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackToolTarget {
    pub track: Track,
    pub action: TrackToolAction,
}

impl RealearnTarget for TrackToolTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.track.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(get_track_name(&self.track).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let position = match self.track.index() {
            None => 0,
            Some(i) => i + 1,
        };
        Some(NumericValue::Discrete(position as _))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackTool)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if !value.is_on() {
            return Ok(None);
        }
        struct UpdateInstanceTrack {
            event: InstanceTrackChangeRequestedEvent,
        }
        impl HitInstruction for UpdateInstanceTrack {
            fn execute(
                self: Box<Self>,
                context: HitInstructionContext,
            ) -> Vec<MappingControlResult> {
                context
                    .domain_event_handler
                    .handle_event(DomainEvent::InstanceTrackChangeRequested(self.event));
                vec![]
            }
        }
        let event = match self.action {
            TrackToolAction::DoNothing => return Ok(None),
            TrackToolAction::SetAsInstanceTrack => {
                InstanceTrackChangeRequestedEvent::SetFromMapping(
                    context.mapping_data.qualified_mapping_id(),
                )
            }
            TrackToolAction::PinAsInstanceTrack => {
                InstanceTrackChangeRequestedEvent::Pin(*self.track.guid())
            }
        };
        let instruction = UpdateInstanceTrack { event };
        Ok(Some(Box::new(instruction)))
    }
}

impl<'a> Target<'a> for TrackToolTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let track_index = self.track.index();
        Some(percentage_for_track_within_project(
            self.track.project(),
            track_index,
        ))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_TOOL_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Track",
    short_name: "Track",
    supports_track: true,
    ..DEFAULT_TARGET
};
