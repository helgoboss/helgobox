use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, FxPropValues, RealearnAutomationMode,
    RealearnTrackArea, TargetCategory, TrackPropValues, VirtualFxType, VirtualTrackType,
};
use crate::domain::{ActionInvocationType, ReaperTargetType, SeekOptions};
use crate::infrastructure::api::convert::to_data::{
    convert_control_element_id, convert_control_element_type,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::{
    serialize_track, BookmarkData, FxData, TargetModelData, TrackData,
};
use crate::{application, domain};
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
            transport_action: convert_transport_action(d.action),
            ..init(d.commons)
        },
        Target::CycleThroughTracks(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::SelectedTrack,
            scroll_arrange_view: d.scroll_arrange_view.unwrap_or_default(),
            scroll_mixer: d.scroll_mixer.unwrap_or_default(),
            ..init(d.commons)
        },
        Target::Seek(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::Seek,
            seek_options: SeekOptions {
                use_time_selection: d.use_time_selection.unwrap_or(false),
                use_loop_points: d.use_loop_points.unwrap_or(false),
                use_regions: d.use_regions.unwrap_or(false),
                use_project: d.use_project.unwrap_or(true),
                move_view: d.move_view.unwrap_or(true),
                seek_play: d.seek_play.unwrap_or(true),
                feedback_resolution: convert_feedback_resolution(
                    d.feedback_resolution.unwrap_or_default(),
                ),
            },
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
            bookmark_data: {
                match d.bookmark {
                    BookmarkDescriptor::Marker(r) => {
                        let (anchor, r#ref) = convert_bookmark_ref(r);
                        BookmarkData {
                            anchor,
                            r#ref,
                            is_region: false,
                        }
                    }
                    BookmarkDescriptor::Region(r) => {
                        let (anchor, r#ref) = convert_bookmark_ref(r);
                        BookmarkData {
                            anchor,
                            r#ref,
                            is_region: true,
                        }
                    }
                }
            },
            seek_options: SeekOptions {
                use_time_selection: d.set_time_selection.unwrap_or(false),
                use_loop_points: d.set_loop_points.unwrap_or(false),
                ..Default::default()
            },
            ..init(d.commons)
        },
        Target::TrackArmState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackArm,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                ..init(d.commons)
            }
        }
        Target::AllTrackFxOnOffState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::AllTrackFxEnable,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                poll_for_feedback: d.poll_for_feedback.unwrap_or(true),
                ..init(d.commons)
            }
        }
        Target::TrackMuteState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackMute,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                ..init(d.commons)
            }
        }
        Target::TrackPeak(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackPeak,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                ..init(d.commons)
            }
        }
        Target::TrackPhase(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackPhase,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                poll_for_feedback: d.poll_for_feedback.unwrap_or(true),
                ..init(d.commons)
            }
        }
        Target::TrackSelectionState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSelection,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                scroll_arrange_view: d.scroll_arrange_view.unwrap_or_default(),
                scroll_mixer: d.scroll_mixer.unwrap_or_default(),
                ..init(d.commons)
            }
        }
        Target::TrackAutomationMode(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackAutomationMode,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                track_automation_mode: convert_automation_mode(d.mode),
                ..init(d.commons)
            }
        }
        Target::TrackAutomationTouchState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::AutomationTouchState,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                touched_parameter_type: {
                    use domain::TouchedParameterType as T;
                    use TouchedParameter::*;
                    match d.touched_parameter {
                        Volume => T::Volume,
                        Pan => T::Pan,
                        Width => T::Width,
                    }
                },
                ..init(d.commons)
            }
        }
        Target::TrackPan(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackPan,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                ..init(d.commons)
            }
        }
        Target::TrackWidth(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackWidth,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                ..init(d.commons)
            }
        }
        Target::TrackVolume(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackVolume,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                ..init(d.commons)
            }
        }
        Target::TrackVisibility(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackShow,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                poll_for_feedback: d.poll_for_feedback.unwrap_or(true),
                track_area: {
                    match d.area {
                        TrackArea::Tcp => RealearnTrackArea::Tcp,
                        TrackArea::Mcp => RealearnTrackArea::Mcp,
                    }
                },
                ..init(d.commons)
            }
        }
        Target::TrackSoloState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSolo,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_data: track_desc.track_data,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                solo_behavior: {
                    use domain::SoloBehavior as T;
                    use SoloBehavior::*;
                    let v = match d.behavior.unwrap_or_default() {
                        InPlace => T::InPlace,
                        IgnoreRouting => T::IgnoreRouting,
                        ReaperPreference => T::ReaperPreference,
                    };
                    Some(v)
                },
                ..init(d.commons)
            }
        }
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
            control_element_type: convert_control_element_type(d.kind.unwrap_or_default()),
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

struct FxDesc {
    track_desc: TrackDesc,
    fx_data: FxData,
    fx_must_have_focus: bool,
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

fn convert_fx_desc(t: FxDescriptor) -> ConversionResult<FxDesc> {
    use FxDescriptor::*;
    let (track_descriptor, props, fx_must_have_focused) = match t {
        Focused => {}
        This { commons } => (
            FxPropValues {
                r#type: VirtualFxType::This,
                ..Default::default()
            },
            commons.fx_must_have_focus.unwrap_or_default(),
        ),
        Dynamic {
            commons,
            chain,
            expression,
        } => (
            TrackPropValues {
                r#type: VirtualTrackType::Dynamic,
                expression,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        ById { commons, chain, id } => (
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
        ByIndex {
            commons,
            chain,
            index,
        } => (
            TrackPropValues {
                r#type: VirtualTrackType::ByIndex,
                index,
                ..Default::default()
            },
            commons.track_must_be_selected.unwrap_or_default(),
        ),
        ByName {
            commons,
            chain,
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
    let desc = FxDesc {
        track_data: serialize_track(props),
        track_must_be_selected,
    };
    Ok(desc)
}

fn convert_transport_action(transport_action: TransportAction) -> domain::TransportAction {
    use domain::TransportAction as T;
    use TransportAction::*;
    match transport_action {
        PlayStop => T::PlayStop,
        PlayPause => T::PlayPause,
        Stop => T::Stop,
        Pause => T::Pause,
        Record => T::Record,
        Repeat => T::Repeat,
    }
}

fn convert_feedback_resolution(r: FeedbackResolution) -> domain::FeedbackResolution {
    use domain::FeedbackResolution as T;
    use FeedbackResolution::*;
    match r {
        Beat => T::Beat,
        High => T::High,
    }
}

fn convert_bookmark_ref(r: BookmarkRef) -> (BookmarkAnchorType, u32) {
    use BookmarkAnchorType as T;
    match r {
        BookmarkRef::ById { id } => (T::Id, id),
        BookmarkRef::ByIndex { index } => (T::Index, index),
    }
}

fn convert_track_exclusivity(exclusivity: Option<TrackExclusivity>) -> domain::TrackExclusivity {
    use domain::TrackExclusivity as T;
    use TrackExclusivity::*;
    match exclusivity {
        None => T::NonExclusive,
        Some(e) => match e {
            WithinProject => T::ExclusiveWithinProject,
            WithinFolder => T::ExclusiveWithinFolder,
            WithinProjectOnOnly => T::ExclusiveWithinProjectOnOnly,
            WithinFolderOnOnly => T::ExclusiveWithinFolderOnOnly,
        },
    }
}
