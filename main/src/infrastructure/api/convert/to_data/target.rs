use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, FxParameterPropValues, FxPropValues,
    RealearnAutomationMode, RealearnTrackArea, TargetCategory, TrackPropValues,
    TrackRoutePropValues, TrackRouteSelectorType, VirtualFxParameterType, VirtualFxType,
    VirtualTrackType,
};
use crate::domain::{
    ActionInvocationType, Exclusivity, FxDisplayType, ReaperTargetType, SeekOptions,
    SendMidiDestination, TrackRouteType,
};
use crate::infrastructure::api::convert::to_data::{
    convert_control_element_id, convert_control_element_type, convert_osc_arg_type, convert_tags,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::{
    serialize_fx, serialize_fx_parameter, serialize_track, serialize_track_route, BookmarkData,
    FxData, FxParameterData, TargetModelData, TrackData, TrackRouteData,
};
use crate::{application, domain};
use realearn_api::schema::*;
use reaper_high::Guid;
use std::rc::Rc;

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
                    .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
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
        Target::AnyOn(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::AnyOn,
            any_on_parameter: convert_any_on_parameter(d.parameter),
            ..init(d.commons)
        },
        Target::CycleThroughTracks(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::SelectedTrack,
            scroll_arrange_view: d
                .scroll_arrange_view
                .unwrap_or(defaults::TARGET_TRACK_SELECTION_SCROLL_ARRANGE_VIEW),
            scroll_mixer: d
                .scroll_mixer
                .unwrap_or(defaults::TARGET_TRACK_SELECTION_SCROLL_MIXER),
            ..init(d.commons)
        },
        Target::Seek(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::Seek,
            seek_options: SeekOptions {
                use_time_selection: d
                    .use_time_selection
                    .unwrap_or(defaults::TARGET_SEEK_USE_TIME_SELECTION),
                use_loop_points: d
                    .use_loop_points
                    .unwrap_or(defaults::TARGET_SEEK_USE_LOOP_POINTS),
                use_regions: d.use_regions.unwrap_or(defaults::TARGET_SEEK_USE_REGIONS),
                use_project: d.use_project.unwrap_or(defaults::TARGET_SEEK_USE_PROJECT),
                move_view: d.move_view.unwrap_or(defaults::TARGET_SEEK_MOVE_VIEW),
                seek_play: d.seek_play.unwrap_or(defaults::TARGET_SEEK_SEEK_PLAY),
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
                use_time_selection: d
                    .set_time_selection
                    .unwrap_or(defaults::TARGET_BOOKMARK_SET_TIME_SELECTION),
                use_loop_points: d
                    .set_loop_points
                    .unwrap_or(defaults::TARGET_BOOKMARK_SET_LOOP_POINTS),
                ..Default::default()
            },
            ..init(d.commons)
        },
        Target::TrackArmState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackArm,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                ..init(d.commons)
            }
        }
        Target::AllTrackFxOnOffState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::AllTrackFxEnable,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
                ..init(d.commons)
            }
        }
        Target::TrackMuteState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackMute,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                ..init(d.commons)
            }
        }
        Target::TrackPeak(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackPeak,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                ..init(d.commons)
            }
        }
        Target::TrackPhase(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackPhase,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
                ..init(d.commons)
            }
        }
        Target::TrackSelectionState(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSelection,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                scroll_arrange_view: d
                    .scroll_arrange_view
                    .unwrap_or(defaults::TARGET_TRACK_SELECTION_SCROLL_ARRANGE_VIEW),
                scroll_mixer: d
                    .scroll_mixer
                    .unwrap_or(defaults::TARGET_TRACK_SELECTION_SCROLL_MIXER),
                ..init(d.commons)
            }
        }
        Target::TrackAutomationMode(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackAutomationMode,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
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
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
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
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                ..init(d.commons)
            }
        }
        Target::TrackWidth(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackWidth,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                ..init(d.commons)
            }
        }
        Target::TrackVolume(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackVolume,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                ..init(d.commons)
            }
        }
        Target::TrackInfo(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackVolume,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                ..init(d.commons)
            }
        }
        Target::TrackVisibility(d) => {
            let track_desc = convert_track_desc(d.track.unwrap_or_default())?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackShow,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_exclusivity: convert_track_exclusivity(d.exclusivity),
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
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
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
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
        Target::CycleThroughFx(d) => {
            let chain_desc = convert_chain_desc(d.chain)?;
            let track_desc = chain_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::FxNavigate,
                fx_display_type: convert_fx_display_kind(d.display_kind.unwrap_or_default()),
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                fx_data: FxData {
                    is_input_fx: chain_desc.is_input_fx,
                    ..Default::default()
                },
                ..init(d.commons)
            }
        }
        Target::FxOnOffState(d) => {
            let fx_desc = convert_fx_desc(d.fx.unwrap_or_default())?;
            let track_desc = fx_desc.chain_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::FxEnable,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                fx_data: fx_desc.fx_data,
                enable_only_if_fx_has_focus: fx_desc.fx_must_have_focus,
                ..init(d.commons)
            }
        }
        Target::LoadFxSnapshot(d) => {
            let fx_desc = convert_fx_desc(d.fx.unwrap_or_default())?;
            let track_desc = fx_desc.chain_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::LoadFxSnapshot,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                fx_data: fx_desc.fx_data,
                enable_only_if_fx_has_focus: fx_desc.fx_must_have_focus,
                fx_snapshot: d.snapshot.map(|s| application::FxSnapshot {
                    fx_type: s.fx_kind.unwrap_or_default(),
                    fx_name: s.fx_name.unwrap_or_default(),
                    preset_name: s.preset_name,
                    chunk: match s.content {
                        FxSnapshotContent::Chunk { chunk } => Rc::new(chunk),
                    },
                }),
                ..init(d.commons)
            }
        }
        Target::CycleThroughFxPresets(d) => {
            let fx_desc = convert_fx_desc(d.fx.unwrap_or_default())?;
            let track_desc = fx_desc.chain_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::FxPreset,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                fx_data: fx_desc.fx_data,
                enable_only_if_fx_has_focus: fx_desc.fx_must_have_focus,
                ..init(d.commons)
            }
        }
        Target::FxVisibility(d) => {
            let fx_desc = convert_fx_desc(d.fx.unwrap_or_default())?;
            let track_desc = fx_desc.chain_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::FxOpen,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                fx_data: fx_desc.fx_data,
                enable_only_if_fx_has_focus: fx_desc.fx_must_have_focus,
                fx_display_type: convert_fx_display_kind(d.display_kind.unwrap_or_default()),
                ..init(d.commons)
            }
        }
        Target::FxParameterValue(d) => {
            let fx_parameter_desc = convert_fx_parameter_desc(d.parameter)?;
            let fx_desc = fx_parameter_desc.fx_desc;
            let track_desc = fx_desc.chain_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::FxParameter,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                fx_data: fx_desc.fx_data,
                enable_only_if_fx_has_focus: fx_desc.fx_must_have_focus,
                fx_parameter_data: fx_parameter_desc.fx_parameter_data,
                ..init(d.commons)
            }
        }
        Target::RouteAutomationMode(d) => {
            let route_desc = convert_route_desc(d.route)?;
            let track_desc = route_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSendAutomationMode,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_route_data: route_desc.track_route_data,
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
                track_automation_mode: convert_automation_mode(d.mode),
                ..init(d.commons)
            }
        }
        Target::RouteMonoState(d) => {
            let route_desc = convert_route_desc(d.route)?;
            let track_desc = route_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSendMono,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_route_data: route_desc.track_route_data,
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
                ..init(d.commons)
            }
        }
        Target::RouteMuteState(d) => {
            let route_desc = convert_route_desc(d.route)?;
            let track_desc = route_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSendMute,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_route_data: route_desc.track_route_data,
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
                ..init(d.commons)
            }
        }
        Target::RoutePhase(d) => {
            let route_desc = convert_route_desc(d.route)?;
            let track_desc = route_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSendPhase,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_route_data: route_desc.track_route_data,
                poll_for_feedback: d
                    .poll_for_feedback
                    .unwrap_or(defaults::TARGET_POLL_FOR_FEEDBACK),
                ..init(d.commons)
            }
        }
        Target::RoutePan(d) => {
            let route_desc = convert_route_desc(d.route)?;
            let track_desc = route_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSendPan,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_route_data: route_desc.track_route_data,
                ..init(d.commons)
            }
        }
        Target::RouteVolume(d) => {
            let route_desc = convert_route_desc(d.route)?;
            let track_desc = route_desc.track_desc;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::TrackSendVolume,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                track_route_data: route_desc.track_route_data,
                ..init(d.commons)
            }
        }
        Target::ClipTransportAction(d) => {
            let clip_desc = convert_clip_desc(d.clip)?;
            let track_desc = match d.output.unwrap_or_default() {
                ClipOutput::Track { track } => convert_track_desc(track.unwrap_or_default())?,
            };
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::ClipTransport,
                track_data: track_desc.track_data,
                enable_only_if_track_is_selected: track_desc.track_must_be_selected,
                slot_index: clip_desc.slot_index,
                transport_action: convert_transport_action(d.action),
                next_bar: d.next_bar.unwrap_or(defaults::TARGET_CLIP_NEXT_BAR),
                buffered: d.buffered.unwrap_or(defaults::TARGET_CLIP_BUFFERED),
                ..init(d.commons)
            }
        }
        Target::ClipSeek(d) => {
            let clip_desc = convert_clip_desc(d.clip)?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::ClipSeek,
                slot_index: clip_desc.slot_index,
                seek_options: SeekOptions {
                    feedback_resolution: convert_feedback_resolution(
                        d.feedback_resolution.unwrap_or_default(),
                    ),
                    ..Default::default()
                },
                ..init(d.commons)
            }
        }
        Target::ClipVolume(d) => {
            let clip_desc = convert_clip_desc(d.clip)?;
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::ClipVolume,
                slot_index: clip_desc.slot_index,
                ..init(d.commons)
            }
        }
        Target::SendMidi(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::SendMidi,
            raw_midi_pattern: d.message.unwrap_or_default(),
            send_midi_destination: match d.destination.unwrap_or_default() {
                MidiDestination::FxOutput => SendMidiDestination::FxOutput,
                MidiDestination::FeedbackOutput => SendMidiDestination::FeedbackOutput,
            },
            ..init(d.commons)
        },
        Target::SendOsc(d) => {
            let (osc_arg_index, osc_arg_type) = if let Some(a) = d.argument {
                (
                    Some(a.index.unwrap_or(defaults::OSC_ARG_INDEX)),
                    convert_osc_arg_type(a.kind.unwrap_or_default()),
                )
            } else {
                (None, Default::default())
            };
            TargetModelData {
                category: TargetCategory::Reaper,
                r#type: ReaperTargetType::SendOsc,
                osc_address_pattern: d.address.unwrap_or_default(),
                osc_arg_index,
                osc_arg_type,
                osc_dev_id: match d.destination.unwrap_or_default() {
                    OscDestination::FeedbackOutput => None,
                    OscDestination::Device { id } => Some(id.parse()?),
                },
                ..init(d.commons)
            }
        }
        Target::EnableInstances(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::EnableInstances,
            tags: convert_tags(d.tags.unwrap_or_default())?,
            exclusivity: {
                use Exclusivity as T;
                use InstanceExclusivity::*;
                match d.exclusivity {
                    None => T::NonExclusive,
                    Some(Exclusive) => T::Exclusive,
                    Some(ExclusiveOnOnly) => T::ExclusiveOnOnly,
                }
            },
            ..init(d.commons)
        },
        Target::EnableMappings(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::EnableMappings,
            tags: convert_tags(d.tags.unwrap_or_default())?,
            exclusivity: {
                use Exclusivity as T;
                use MappingExclusivity::*;
                match d.exclusivity {
                    None => T::NonExclusive,
                    Some(Exclusive) => T::Exclusive,
                    Some(ExclusiveOnOnly) => T::ExclusiveOnOnly,
                }
            },
            ..init(d.commons)
        },
        Target::LoadMappingSnapshots(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::LoadMappingSnapshot,
            tags: convert_tags(d.tags.unwrap_or_default())?,
            active_mappings_only: d
                .active_mappings_only
                .unwrap_or(defaults::TARGET_LOAD_MAPPING_SNAPSHOT_ACTIVE_MAPPINGS_ONLY),
            ..init(d.commons)
        },
        Target::CycleThroughGroupMappings(d) => TargetModelData {
            category: TargetCategory::Reaper,
            r#type: ReaperTargetType::NavigateWithinGroup,
            exclusivity: {
                use Exclusivity as T;
                use GroupMappingExclusivity::*;
                match d.exclusivity {
                    None => T::NonExclusive,
                    Some(Exclusive) => T::Exclusive,
                }
            },
            group_id: d.group.map(|g| g.into()).unwrap_or_default(),
            ..init(d.commons)
        },
        Target::Virtual(d) => TargetModelData {
            category: TargetCategory::Virtual,
            control_element_type: convert_control_element_type(d.character.unwrap_or_default()),
            control_element_index: convert_control_element_id(d.id),
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

#[derive(Default)]
struct TrackDesc {
    track_data: TrackData,
    track_must_be_selected: bool,
}

#[derive(Default)]
struct FxChainDesc {
    track_desc: TrackDesc,
    is_input_fx: bool,
}

#[derive(Default)]
struct RouteDesc {
    track_desc: TrackDesc,
    track_route_data: TrackRouteData,
}

#[derive(Default)]
struct ClipDesc {
    slot_index: usize,
}

#[derive(Default)]
struct FxDesc {
    chain_desc: FxChainDesc,
    fx_data: FxData,
    fx_must_have_focus: bool,
}

#[derive(Default)]
struct FxParameterDesc {
    fx_desc: FxDesc,
    fx_parameter_data: FxParameterData,
}

fn convert_clip_desc(t: ClipDescriptor) -> ConversionResult<ClipDesc> {
    let desc = match t {
        ClipDescriptor::Slot { index } => ClipDesc {
            slot_index: index as _,
        },
    };
    Ok(desc)
}

fn convert_track_desc(t: TrackDescriptor) -> ConversionResult<TrackDesc> {
    use TrackDescriptor::*;
    let (props, track_must_be_selected) = match t {
        This { commons } => (
            TrackPropValues {
                r#type: VirtualTrackType::This,
                ..Default::default()
            },
            commons
                .track_must_be_selected
                .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
        ),
        Master { commons } => (
            TrackPropValues {
                r#type: VirtualTrackType::Master,
                ..Default::default()
            },
            commons
                .track_must_be_selected
                .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
        ),
        Selected { allow_multiple } => (
            TrackPropValues {
                r#type: if allow_multiple.unwrap_or(defaults::TARGET_TRACK_SELECTED_ALLOW_MULTIPLE)
                {
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
            commons
                .track_must_be_selected
                .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
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
            commons
                .track_must_be_selected
                .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
        ),
        ByIndex { commons, index } => (
            TrackPropValues {
                r#type: VirtualTrackType::ByIndex,
                index,
                ..Default::default()
            },
            commons
                .track_must_be_selected
                .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
        ),
        ByName {
            commons,
            name,
            allow_multiple,
        } => (
            TrackPropValues {
                r#type: if allow_multiple.unwrap_or(defaults::TARGET_BY_NAME_ALLOW_MULTIPLE) {
                    VirtualTrackType::AllByName
                } else {
                    VirtualTrackType::ByName
                },
                name,
                ..Default::default()
            },
            commons
                .track_must_be_selected
                .unwrap_or(defaults::TARGET_TRACK_MUST_BE_SELECTED),
        ),
    };
    let desc = TrackDesc {
        track_data: serialize_track(props),
        track_must_be_selected,
    };
    Ok(desc)
}

fn convert_chain_desc(t: FxChainDescriptor) -> ConversionResult<FxChainDesc> {
    use FxChainDescriptor::*;
    let desc = match t {
        Track { track, chain } => FxChainDesc {
            track_desc: convert_track_desc(track.unwrap_or_default())?,
            is_input_fx: chain.unwrap_or_default() == TrackFxChain::Input,
        },
    };
    Ok(desc)
}

fn convert_route_desc(t: RouteDescriptor) -> ConversionResult<RouteDesc> {
    use RouteDescriptor::*;
    let (track_desc, props) = match t {
        Dynamic {
            commons,
            expression,
        } => (
            convert_track_desc(commons.track.unwrap_or_default())?,
            TrackRoutePropValues {
                selector_type: TrackRouteSelectorType::Dynamic,
                r#type: convert_route_kind(commons.kind.unwrap_or_default()),
                expression,
                ..Default::default()
            },
        ),
        ById { commons, id } => (
            convert_track_desc(commons.track.unwrap_or_default())?,
            TrackRoutePropValues {
                selector_type: TrackRouteSelectorType::ById,
                r#type: convert_route_kind(commons.kind.unwrap_or_default()),
                id: if let Some(id) = id {
                    Some(Guid::from_string_without_braces(&id)?)
                } else {
                    None
                },
                ..Default::default()
            },
        ),
        ByIndex { commons, index } => (
            convert_track_desc(commons.track.unwrap_or_default())?,
            TrackRoutePropValues {
                selector_type: TrackRouteSelectorType::ByIndex,
                r#type: convert_route_kind(commons.kind.unwrap_or_default()),
                index,
                ..Default::default()
            },
        ),
        ByName { commons, name } => (
            convert_track_desc(commons.track.unwrap_or_default())?,
            TrackRoutePropValues {
                selector_type: TrackRouteSelectorType::ByName,
                r#type: convert_route_kind(commons.kind.unwrap_or_default()),
                name,
                ..Default::default()
            },
        ),
    };
    let desc = RouteDesc {
        track_desc,
        track_route_data: serialize_track_route(props),
    };
    Ok(desc)
}

fn convert_route_kind(kind: TrackRouteKind) -> TrackRouteType {
    use TrackRouteKind::*;
    use TrackRouteType as T;
    match kind {
        Send => T::Send,
        Receive => T::Receive,
        HardwareOutput => T::HardwareOutput,
    }
}

fn convert_fx_desc(t: FxDescriptor) -> ConversionResult<FxDesc> {
    use FxDescriptor::*;
    let (chain_desc, props, fx_must_have_focus) = match t {
        Focused => (
            FxChainDesc::default(),
            FxPropValues {
                r#type: VirtualFxType::Focused,
                ..Default::default()
            },
            false,
        ),
        This { commons } => (
            FxChainDesc::default(),
            FxPropValues {
                r#type: VirtualFxType::This,
                ..Default::default()
            },
            commons
                .fx_must_have_focus
                .unwrap_or(defaults::TARGET_FX_MUST_HAVE_FOCUS),
        ),
        Dynamic {
            commons,
            chain,
            expression,
        } => (
            convert_chain_desc(chain)?,
            FxPropValues {
                r#type: VirtualFxType::Dynamic,
                expression,
                ..Default::default()
            },
            commons
                .fx_must_have_focus
                .unwrap_or(defaults::TARGET_FX_MUST_HAVE_FOCUS),
        ),
        ById { commons, chain, id } => (
            convert_chain_desc(chain)?,
            FxPropValues {
                r#type: VirtualFxType::ById,
                id: if let Some(id) = id {
                    Some(Guid::from_string_without_braces(&id)?)
                } else {
                    None
                },
                ..Default::default()
            },
            commons
                .fx_must_have_focus
                .unwrap_or(defaults::TARGET_FX_MUST_HAVE_FOCUS),
        ),
        ByIndex {
            commons,
            chain,
            index,
        } => (
            convert_chain_desc(chain)?,
            FxPropValues {
                r#type: VirtualFxType::ByIndex,
                index,
                ..Default::default()
            },
            commons
                .fx_must_have_focus
                .unwrap_or(defaults::TARGET_FX_MUST_HAVE_FOCUS),
        ),
        ByName {
            commons,
            chain,
            name,
            allow_multiple,
        } => (
            convert_chain_desc(chain)?,
            FxPropValues {
                r#type: if allow_multiple.unwrap_or(defaults::TARGET_BY_NAME_ALLOW_MULTIPLE) {
                    VirtualFxType::AllByName
                } else {
                    VirtualFxType::ByName
                },
                name,
                ..Default::default()
            },
            commons
                .fx_must_have_focus
                .unwrap_or(defaults::TARGET_FX_MUST_HAVE_FOCUS),
        ),
    };
    let desc = FxDesc {
        chain_desc,
        fx_data: serialize_fx(props),
        fx_must_have_focus,
    };
    Ok(desc)
}

fn convert_fx_parameter_desc(t: FxParameterDescriptor) -> ConversionResult<FxParameterDesc> {
    use FxParameterDescriptor::*;
    let (fx_desc, props) = match t {
        Dynamic { fx, expression } => (
            convert_fx_desc(fx.unwrap_or_default())?,
            FxParameterPropValues {
                r#type: VirtualFxParameterType::Dynamic,
                expression,
                ..Default::default()
            },
        ),
        ById { fx, index } => (
            convert_fx_desc(fx.unwrap_or_default())?,
            FxParameterPropValues {
                r#type: VirtualFxParameterType::ById,
                index,
                ..Default::default()
            },
        ),
        ByIndex { fx, index } => (
            convert_fx_desc(fx.unwrap_or_default())?,
            FxParameterPropValues {
                r#type: VirtualFxParameterType::ByIndex,
                index,
                ..Default::default()
            },
        ),
        ByName { fx, name } => (
            convert_fx_desc(fx.unwrap_or_default())?,
            FxParameterPropValues {
                r#type: VirtualFxParameterType::ByName,
                name,
                ..Default::default()
            },
        ),
    };
    let desc = FxParameterDesc {
        fx_desc,
        fx_parameter_data: serialize_fx_parameter(props),
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

fn convert_any_on_parameter(parameter: AnyOnParameter) -> domain::AnyOnParameter {
    use domain::AnyOnParameter as T;
    use AnyOnParameter::*;
    match parameter {
        TrackSolo => T::TrackSolo,
        TrackMute => T::TrackMute,
        TrackArm => T::TrackArm,
        TrackSelection => T::TrackSelection,
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

fn convert_fx_display_kind(display_kind: FxDisplayKind) -> FxDisplayType {
    use domain::FxDisplayType as T;
    use FxDisplayKind::*;
    match display_kind {
        FloatingWindow => T::FloatingWindow,
        Chain => T::Chain,
    }
}
