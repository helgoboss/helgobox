use crate::domain::{
    get_effective_tracks, get_track_name, percentage_for_scoped_track_within_project,
    ChangeInstanceTrackArgs, CompartmentKind, ControlContext, ExtendedProcessorContext,
    HitResponse, InstanceTrackChangeRequest, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TagScope, TargetCharacter, TargetSection, TargetTypeDef, TrackDescriptor,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target};
use helgobox_api::persistence::{TrackScope, TrackToolAction};
use reaper_high::{Project, Track};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackToolTarget {
    pub track_descriptor: TrackDescriptor,
    pub action: TrackToolAction,
    pub scope: TagScope,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackToolTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let tracks = get_effective_tracks(context, &self.track_descriptor.track, compartment)
            .and_then(|tracks| {
                if tracks.is_empty() {
                    Err("resolved to zero tracks")
                } else {
                    Ok(tracks)
                }
            });
        let targets = match tracks {
            Ok(tracks) => tracks
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackTool(TrackToolTarget {
                        track: Some(track),
                        action: self.action,
                        scope: self.scope.clone(),
                    })
                })
                .collect(),
            Err(e) => {
                if self.action == TrackToolAction::SetAsUnitTrack {
                    // If we just want to *set* the (unresolved) track as instance track, we
                    // don't need a resolved target.
                    let target = ReaperTarget::TrackTool(TrackToolTarget {
                        track: None,
                        action: self.action,
                        scope: self.scope.clone(),
                    });
                    vec![target]
                } else {
                    // Otherwise we should classify the target as inactive.
                    return Err(e);
                }
            }
        };
        Ok(targets)
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        Some(&self.track_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackToolTarget {
    pub track: Option<Track>,
    pub action: TrackToolAction,
    pub scope: TagScope,
}

impl RealearnTarget for TrackToolTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn is_available(&self, _: ControlContext) -> bool {
        match &self.track {
            None => false,
            Some(t) => t.is_available(),
        }
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.as_ref()?.project())
    }

    fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(get_track_name(self.track.as_ref()?, TrackScope::AllTracks).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let position = match self.track.as_ref()?.index() {
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
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        let request = match self.action {
            TrackToolAction::DoNothing => return Ok(HitResponse::ignored()),
            TrackToolAction::SetAsUnitTrack => InstanceTrackChangeRequest::SetFromMapping(
                context.mapping_data.qualified_mapping_id(),
            ),
            TrackToolAction::PinAsUnitTrack => {
                let track = self.track.as_ref().ok_or("track could not be resolved")?;
                let guid = if track.is_master_track() {
                    None
                } else {
                    Some(*track.guid())
                };
                InstanceTrackChangeRequest::Pin(guid)
            }
        };
        let args = ChangeInstanceTrackArgs {
            common: context
                .control_context
                .instance_container_common_args(&self.scope),
            request,
        };
        context
            .control_context
            .unit_container
            .change_instance_track(args)?;
        Ok(HitResponse::processed_with_effect())
    }

    fn can_report_current_value(&self) -> bool {
        matches!(self.action, TrackToolAction::DoNothing)
    }
}

impl<'a> Target<'a> for TrackToolTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        match self.action {
            TrackToolAction::DoNothing => {
                let track_index = self.track.as_ref()?.index();
                let percentage = percentage_for_scoped_track_within_project(
                    self.track.as_ref()?.project(),
                    TrackScope::AllTracks,
                    track_index,
                );
                Some(percentage)
            }
            TrackToolAction::SetAsUnitTrack | TrackToolAction::PinAsUnitTrack => {
                // In future, we might support feedback here.
                None
            }
        }
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_TOOL_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Track,
    name: "Track",
    short_name: "Track",
    supports_track: true,
    supports_tags: true,
    ..DEFAULT_TARGET
};
