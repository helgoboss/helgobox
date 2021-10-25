use crate::application;
use crate::application::{
    AutomationModeOverrideType, RealearnAutomationMode, TargetCategory, TrackPropValues,
    VirtualTrackType,
};
use crate::domain::{ActionInvocationType, ReaperTargetType};
use crate::infrastructure::api::convert::to_data::{
    convert_control_element_id, convert_control_element_type,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::{serialize_track, TargetModelData, TrackData, TrackRouteData};
use reaper_high::Guid;

pub fn convert_target(t: Target) -> ConversionResult<TargetModelData> {
    let data = match t {
        Target::LastTouched(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::LastTouched,
            ..init(d.commons)
        },
        Target::AutomationModeOverride(d) => {
            let (t, m): (AutomationModeOverrideType, RealearnAutomationMode) = {
                use AutomationModeOverrideType as T;
                match d.r#override {
                    None => (T::None, Default::default()),
                    Some(o) => match o {
                        AutomationModeOverride::Bypass => (T::Bypass, Default::default()),
                        AutomationModeOverride::Mode { mode } => {
                            (T::Override, convert_automation_mode(mode))
                        }
                    },
                }
            };
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::AutomationModeOverride,
                automation_mode_override_type: t,
                track_automation_mode: m,
                ..init(d.commons)
            }
        }
        Target::ReaperAction(d) => {
            let track_desc = if let Some(td) = d.track {
                Some(convert_track_desc(td)?)
            } else {
                None
            };
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::Action,
                command_name: d.command.map(|cmd| match cmd {
                    ReaperCommand::Id(id) => id.to_string(),
                    ReaperCommand::Name(n) => n,
                }),
                invocation_type: {
                    use ActionInvocationKind::*;
                    use ActionInvocationType as T;
                    match d.invocation.unwrap_or_default() {
                        Trigger => T::Trigger,
                        Absolute => T::Absolute,
                        Relative => T::Relative,
                    }
                },
                with_track: track_desc.is_some(),
                enable_only_if_track_is_selected: track_desc
                    .as_ref()
                    .map(|d| d.track_must_be_selected)
                    .unwrap_or_default(),
                track_data: track_desc.map(|d| d.track_data).unwrap_or_default(),
                ..init(d.commons)
            }
        }
        Target::TransportAction(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::Transport,
            ..init(d.commons)
        },
        Target::CycleThroughTracks(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::SelectedTrack,
            ..init(d.commons)
        },
        Target::Seek(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::Seek,
            ..init(d.commons)
        },
        Target::PlayRate(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::Playrate,
            ..init(d.commons)
        },
        Target::Tempo(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::Tempo,
            ..init(d.commons)
        },
        Target::GoToBookmark(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::GoToBookmark,
            ..init(d.commons)
        },
        Target::TrackArmState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackArm,
            ..init(d.commons)
        },
        Target::AllTrackFxOnOffState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::AllTrackFxEnable,
            ..init(d.commons)
        },
        Target::TrackMuteState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackMute,
            ..init(d.commons)
        },
        Target::TrackPeak(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackPeak,
            ..init(d.commons)
        },
        Target::TrackPhase(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackPhase,
            ..init(d.commons)
        },
        Target::TrackSelectionState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSelection,
            ..init(d.commons)
        },
        Target::TrackAutomationMode(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackAutomationMode,
            ..init(d.commons)
        },
        Target::TrackAutomationTouchState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::AutomationTouchState,
            ..init(d.commons)
        },
        Target::TrackPan(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackPan,
            ..init(d.commons)
        },
        Target::TrackWidth(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackWidth,
            ..init(d.commons)
        },
        Target::TrackVolume(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackVolume,
            ..init(d.commons)
        },
        Target::TrackVisibility(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackShow,
            ..init(d.commons)
        },
        Target::TrackSoloState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSolo,
            ..init(d.commons)
        },
        Target::CycleThroughFx(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::FxNavigate,
            ..init(d.commons)
        },
        Target::FxOnOffState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::FxEnable,
            ..init(d.commons)
        },
        Target::LoadFxSnapshot(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::LoadFxSnapshot,
            ..init(d.commons)
        },
        Target::CycleThroughFxPresets(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::FxPreset,
            ..init(d.commons)
        },
        Target::FxVisibility(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::FxOpen,
            ..init(d.commons)
        },
        Target::FxParameterValue(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::FxParameter,
            ..init(d.commons)
        },
        Target::SendAutomationMode(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSendAutomationMode,
            ..init(d.commons)
        },
        Target::SendMonoState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSendMono,
            ..init(d.commons)
        },
        Target::SendMuteState(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSendMute,
            ..init(d.commons)
        },
        Target::SendPhase(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSendPhase,
            ..init(d.commons)
        },
        Target::SendPan(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSendPan,
            ..init(d.commons)
        },
        Target::SendVolume(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::TrackSendVolume,
            ..init(d.commons)
        },
        Target::ClipTransportAction(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::ClipTransport,
            ..init(d.commons)
        },
        Target::ClipSeek(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::ClipSeek,
            ..init(d.commons)
        },
        Target::ClipVolume(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::ClipVolume,
            ..init(d.commons)
        },
        Target::SendMidi(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::SendMidi,
            ..init(d.commons)
        },
        Target::SendOsc(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::SendOsc,
            ..init(d.commons)
        },
        Target::EnableInstances(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::EnableInstances,
            ..init(d.commons)
        },
        Target::EnableMappings(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::EnableMappings,
            ..init(d.commons)
        },
        Target::LoadMappingSnapshots(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::LoadMappingSnapshot,
            ..init(d.commons)
        },
        Target::CycleThroughGroupMappings(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::NavigateWithinGroup,
            ..init(d.commons)
        },
        Target::Virtual(d) => TargetModelData {
            category: TargetCategory::Virtual,
            control_element_type: convert_control_element_type(d.kind),
            control_element_index: convert_control_element_id(d.id.clone()),
            ..Default::default()
        },
    };
    Ok(data)
}

fn init(commons: TargetCommons) -> TargetModelData {
    TargetModelData {
        unit: {
            use application::TargetUnit as T;
            use TargetUnit::*;
            match commons.unit.unwrap_or_default() {
                Native => T::Native,
                Percent => T::Percent,
            }
        },
        ..Default::default()
    }
}

fn convert_automation_mode(mode: AutomationMode) -> RealearnAutomationMode {
    use AutomationMode::*;
    use RealearnAutomationMode as T;
    match mode {
        TrimRead => T::TrimRead,
        Read => T::Read,
        Touch => T::Touch,
        Write => T::Write,
        Latch => T::Latch,
        LatchPreview => T::LatchPreview,
    }
}

struct TrackDesc {
    track_data: TrackData,
    track_must_be_selected: bool,
}

fn convert_track_desc(t: TrackDescriptor) -> ConversionResult<TrackDesc> {
    use TrackDescriptor::*;
    let (props, track_must_be_selected) = match t {
        This { commons } => (
            TrackPropValues {
                r#type: VirtualTrackType::This,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        Master { commons } => (
            TrackPropValues {
                r#type: VirtualTrackType::Master,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        Selected { allow_multiple } => (
            TrackPropValues {
                r#type: if allow_multiple.unwrap_or_default() {
                    VirtualTrackType::AllSelected
                } else {
                    VirtualTrackType::Selected
                },
                ..Default::default()
            },
            false,
        ),
        Dynamic {
            commons,
            expression,
        } => (
            TrackPropValues {
                r#type: VirtualTrackType::Dynamic,
                expression,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        ById { commons, id } => (
            TrackPropValues {
                r#type: VirtualTrackType::ById,
                id: if let Some(id) = id {
                    Some(Guid::from_string_without_braces(&id)?)
                } else {
                    None
                },
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        ByIndex { commons, index } => (
            TrackPropValues {
                r#type: VirtualTrackType::ByIndex,
                index,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        ByName {
            commons,
            name,
            allow_multiple,
        } => (
            TrackPropValues {
                r#type: if allow_multiple.unwrap_or_default() {
                    VirtualTrackType::AllByName
                } else {
                    VirtualTrackType::ByName
                },
                name,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
    };
    let desc = TrackDesc {
        track_data: serialize_track(props),
        track_must_be_selected,
    };
    Ok(desc)
}
