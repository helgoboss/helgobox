use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, RealearnAutomationMode, RealearnTrackArea,
    TargetCategory, TargetUnit, TrackRouteSelectorType, VirtualFxParameterType, VirtualFxType,
    VirtualTrackType,
};
use crate::domain::{
    ActionInvocationType, AnyOnParameter, Exclusivity, FeedbackResolution, FxDisplayType,
    ReaperTargetType, SendMidiDestination, SoloBehavior, TouchedParameterType, TrackExclusivity,
    TrackRouteType, TransportAction,
};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_control_element_kind, convert_osc_argument, convert_tags,
    ConversionStyle,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::{
    deserialize_fx, deserialize_fx_parameter, deserialize_track, deserialize_track_route,
    TargetModelData, TrackData,
};
use realearn_api::schema;
use realearn_api::schema::{
    AllTrackFxOnOffStateTarget, AnyOnTarget, AutomationModeOverrideTarget, BookmarkDescriptor,
    BookmarkRef, ClipSeekTarget, ClipTransportActionTarget, ClipVolumeTarget,
    CycleThroughFxPresetsTarget, CycleThroughFxTarget, CycleThroughGroupMappingsTarget,
    CycleThroughTracksTarget, EnableInstancesTarget, EnableMappingsTarget, FxOnOffStateTarget,
    FxOnlineOfflineStateTarget, FxParameterValueTarget, FxVisibilityTarget, GoToBookmarkTarget,
    LastTouchedTarget, LoadFxSnapshotTarget, LoadMappingSnapshotsTarget, PlayRateTarget,
    ReaperActionTarget, RouteAutomationModeTarget, RouteMonoStateTarget, RouteMuteStateTarget,
    RoutePanTarget, RoutePhaseTarget, RouteVolumeTarget, SeekTarget, SendMidiTarget, SendOscTarget,
    TempoTarget, TrackArmStateTarget, TrackAutomationModeTarget, TrackAutomationTouchStateTarget,
    TrackMuteStateTarget, TrackPanTarget, TrackPeakTarget, TrackPhaseTarget,
    TrackSelectionStateTarget, TrackSoloStateTarget, TrackToolTarget, TrackVisibilityTarget,
    TrackVolumeTarget, TrackWidthTarget, TransportActionTarget,
};

pub fn convert_target(
    data: TargetModelData,
    style: ConversionStyle,
) -> ConversionResult<schema::Target> {
    use TargetCategory::*;
    match data.category {
        Reaper => convert_real_target(data, style),
        Virtual => Ok(convert_virtual_target(data, style)),
    }
}

fn convert_real_target(
    data: TargetModelData,
    style: ConversionStyle,
) -> ConversionResult<schema::Target> {
    use schema::Target as T;
    use ReaperTargetType::*;
    let commons = convert_commons(data.unit, style)?;
    let target = match data.r#type {
        LastTouched => T::LastTouched(LastTouchedTarget { commons }),
        AutomationModeOverride => {
            let t = AutomationModeOverrideTarget {
                commons,
                r#override: convert_automation_mode_override(
                    data.automation_mode_override_type,
                    data.track_automation_mode,
                ),
            };
            T::AutomationModeOverride(t)
        }
        Action => T::ReaperAction(ReaperActionTarget {
            commons,
            command: {
                if let Some(n) = data.command_name {
                    let v = match n.parse::<u32>() {
                        Ok(id) => schema::ReaperCommand::Id(id),
                        Err(_) => schema::ReaperCommand::Name(n),
                    };
                    Some(v)
                } else {
                    None
                }
            },
            invocation: {
                use schema::ActionInvocationKind as T;
                use ActionInvocationType::*;
                let v = match data.invocation_type {
                    Trigger => T::Trigger,
                    Absolute => T::Absolute,
                    Relative => T::Relative,
                };
                style.required_value(v)
            },
            track: if data.with_track {
                convert_track_descriptor(
                    data.track_data,
                    data.enable_only_if_track_is_selected,
                    style,
                )
            } else {
                None
            },
        }),
        Transport => T::TransportAction(TransportActionTarget {
            commons,
            action: convert_transport_action(data.transport_action),
        }),
        AnyOn => T::AnyOn(AnyOnTarget {
            commons,
            parameter: convert_any_on_parameter(data.any_on_parameter),
        }),
        GoToBookmark => T::GoToBookmark(GoToBookmarkTarget {
            commons,
            bookmark: {
                let bookmark_ref = match data.bookmark_data.anchor {
                    BookmarkAnchorType::Id => BookmarkRef::ById {
                        id: data.bookmark_data.r#ref,
                    },
                    BookmarkAnchorType::Index => BookmarkRef::ByIndex {
                        index: data.bookmark_data.r#ref,
                    },
                };
                if data.bookmark_data.is_region {
                    BookmarkDescriptor::Region(bookmark_ref)
                } else {
                    BookmarkDescriptor::Marker(bookmark_ref)
                }
            },
            set_time_selection: style.required_value_with_default(
                data.seek_options.use_time_selection,
                defaults::TARGET_BOOKMARK_SET_TIME_SELECTION,
            ),
            set_loop_points: style.required_value_with_default(
                data.seek_options.use_loop_points,
                defaults::TARGET_BOOKMARK_SET_LOOP_POINTS,
            ),
        }),
        TrackAutomationMode => T::TrackAutomationMode(TrackAutomationModeTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            mode: convert_automation_mode(data.track_automation_mode),
        }),
        AutomationTouchState => T::TrackAutomationTouchState(TrackAutomationTouchStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            touched_parameter: {
                use schema::TouchedParameter as T;
                use TouchedParameterType::*;
                match data.touched_parameter_type {
                    Volume => T::Volume,
                    Pan => T::Pan,
                    Width => T::Width,
                }
            },
        }),
        TrackShow => T::TrackVisibility(TrackVisibilityTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            area: {
                match data.track_area {
                    RealearnTrackArea::Tcp => schema::TrackArea::Tcp,
                    RealearnTrackArea::Mcp => schema::TrackArea::Mcp,
                }
            },
        }),
        FxNavigate => T::CycleThroughFx(CycleThroughFxTarget {
            commons,
            display_kind: convert_fx_display_kind(data.fx_display_type, style),
            chain: convert_fx_chain_descriptor(data, style),
        }),
        FxParameter => T::FxParameterValue(FxParameterValueTarget {
            commons,
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            parameter: convert_fx_parameter_descriptor(data, style),
        }),
        TrackSendAutomationMode => T::RouteAutomationMode(RouteAutomationModeTarget {
            commons,
            mode: convert_automation_mode(data.track_automation_mode),
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            route: convert_route_descriptor(data, style),
        }),
        TrackSendMono => T::RouteMonoState(RouteMonoStateTarget {
            commons,
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            route: convert_route_descriptor(data, style),
        }),
        TrackSendMute => T::RouteMuteState(RouteMuteStateTarget {
            commons,
            poll_for_feedback: Some(data.poll_for_feedback),
            route: convert_route_descriptor(data, style),
        }),
        TrackSendPhase => T::RoutePhase(RoutePhaseTarget {
            commons,
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            route: convert_route_descriptor(data, style),
        }),
        TrackSendPan => T::RoutePan(RoutePanTarget {
            commons,
            route: convert_route_descriptor(data, style),
        }),
        TrackSendVolume => T::RouteVolume(RouteVolumeTarget {
            commons,
            route: convert_route_descriptor(data, style),
        }),
        ClipTransport => T::ClipTransportAction(ClipTransportActionTarget {
            commons,
            slot: data.clip_slot.unwrap_or_default(),
            action: convert_transport_action(data.transport_action),
        }),
        ClipSeek => T::ClipSeek(ClipSeekTarget {
            commons,
            slot: data.clip_slot.unwrap_or_default(),
            feedback_resolution: convert_feedback_resolution(
                data.seek_options.feedback_resolution,
                style,
            ),
        }),
        ClipVolume => T::ClipVolume(ClipVolumeTarget {
            commons,
            slot: data.clip_slot.unwrap_or_default(),
        }),
        SendMidi => T::SendMidi(SendMidiTarget {
            commons,
            message: style.required_value(data.raw_midi_pattern),
            destination: {
                use schema::MidiDestination as T;
                use SendMidiDestination::*;
                let dest = match data.send_midi_destination {
                    FxOutput => T::FxOutput,
                    FeedbackOutput => T::FeedbackOutput,
                };
                style.required_value(dest)
            },
        }),
        SelectedTrack => T::CycleThroughTracks(CycleThroughTracksTarget {
            commons,
            scroll_arrange_view: style.required_value_with_default(
                data.scroll_arrange_view,
                defaults::TARGET_TRACK_SELECTION_SCROLL_ARRANGE_VIEW,
            ),
            scroll_mixer: style.required_value_with_default(
                data.scroll_mixer,
                defaults::TARGET_TRACK_SELECTION_SCROLL_MIXER,
            ),
        }),
        Seek => T::Seek(SeekTarget {
            commons,
            use_time_selection: style.required_value_with_default(
                data.seek_options.use_time_selection,
                defaults::TARGET_SEEK_USE_TIME_SELECTION,
            ),
            use_loop_points: style.required_value_with_default(
                data.seek_options.use_loop_points,
                defaults::TARGET_SEEK_USE_LOOP_POINTS,
            ),
            use_regions: style.required_value_with_default(
                data.seek_options.use_regions,
                defaults::TARGET_SEEK_USE_REGIONS,
            ),
            use_project: style.required_value_with_default(
                data.seek_options.use_project,
                defaults::TARGET_SEEK_USE_PROJECT,
            ),
            move_view: style.required_value_with_default(
                data.seek_options.move_view,
                defaults::TARGET_SEEK_MOVE_VIEW,
            ),
            seek_play: style.required_value_with_default(
                data.seek_options.seek_play,
                defaults::TARGET_SEEK_SEEK_PLAY,
            ),
            feedback_resolution: convert_feedback_resolution(
                data.seek_options.feedback_resolution,
                style,
            ),
        }),
        Playrate => T::PlayRate(PlayRateTarget { commons }),
        Tempo => T::Tempo(TempoTarget { commons }),
        TrackArm => T::TrackArmState(TrackArmStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
        }),
        AllTrackFxEnable => T::AllTrackFxOnOffState(AllTrackFxOnOffStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
        }),
        TrackMute => T::TrackMuteState(TrackMuteStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
        }),
        TrackPeak => T::TrackPeak(TrackPeakTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
        }),
        TrackPhase => T::TrackPhase(TrackPhaseTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
        }),
        TrackSelection => T::TrackSelectionState(TrackSelectionStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            scroll_arrange_view: style.required_value_with_default(
                data.scroll_arrange_view,
                defaults::TARGET_TRACK_SELECTION_SCROLL_ARRANGE_VIEW,
            ),
            scroll_mixer: style.required_value_with_default(
                data.scroll_mixer,
                defaults::TARGET_TRACK_SELECTION_SCROLL_MIXER,
            ),
        }),
        TrackPan => T::TrackPan(TrackPanTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
        }),
        TrackWidth => T::TrackWidth(TrackWidthTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
        }),
        TrackVolume => T::TrackVolume(TrackVolumeTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
        }),
        TrackTool => T::TrackTool(TrackToolTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
        }),
        TrackSolo => T::TrackSoloState(TrackSoloStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            behavior: {
                use schema::SoloBehavior as T;
                use SoloBehavior::*;
                let v = data.solo_behavior.map(|b| match b {
                    InPlace => T::InPlace,
                    IgnoreRouting => T::IgnoreRouting,
                    ReaperPreference => T::ReaperPreference,
                });
                style.optional_value(v)
            },
        }),
        FxEnable => T::FxOnOffState(FxOnOffStateTarget {
            commons,
            fx: convert_fx_descriptor(data, style),
        }),
        FxOnline => T::FxOnlineOfflineState(FxOnlineOfflineStateTarget {
            commons,
            fx: convert_fx_descriptor(data, style),
        }),
        LoadFxSnapshot => T::LoadFxSnapshot(LoadFxSnapshotTarget {
            commons,
            snapshot: {
                data.fx_snapshot.as_ref().map(|s| schema::FxSnapshot {
                    fx_kind: style.required_value(s.fx_type.clone()),
                    fx_name: style.required_value(s.fx_name.clone()),
                    preset_name: style.optional_value(s.preset_name.clone()),
                    content: {
                        schema::FxSnapshotContent::Chunk {
                            chunk: (*s.chunk).clone(),
                        }
                    },
                })
            },
            fx: convert_fx_descriptor(data, style),
        }),
        FxPreset => T::CycleThroughFxPresets(CycleThroughFxPresetsTarget {
            commons,
            fx: convert_fx_descriptor(data, style),
        }),
        FxOpen => T::FxVisibility(FxVisibilityTarget {
            commons,
            display_kind: convert_fx_display_kind(data.fx_display_type, style),
            fx: convert_fx_descriptor(data, style),
        }),
        SendOsc => T::SendOsc(SendOscTarget {
            commons,
            address: style.required_value(data.osc_address_pattern),
            argument: convert_osc_argument(data.osc_arg_index, data.osc_arg_type, style),
            destination: {
                use schema::OscDestination as T;
                let v = match data.osc_dev_id {
                    None => T::FeedbackOutput,
                    Some(id) => T::Device { id: id.to_string() },
                };
                style.required_value(v)
            },
        }),
        EnableInstances => T::EnableInstances(EnableInstancesTarget {
            commons,
            tags: convert_tags(&data.tags, style),
            exclusivity: {
                use schema::InstanceExclusivity as T;
                use Exclusivity::*;
                match data.exclusivity {
                    NonExclusive => None,
                    Exclusive => Some(T::Exclusive),
                    ExclusiveOnOnly => Some(T::ExclusiveOnOnly),
                }
            },
        }),
        EnableMappings => T::EnableMappings(EnableMappingsTarget {
            commons,
            tags: convert_tags(&data.tags, style),
            exclusivity: {
                use schema::MappingExclusivity as T;
                use Exclusivity::*;
                match data.exclusivity {
                    NonExclusive => None,
                    Exclusive => Some(T::Exclusive),
                    ExclusiveOnOnly => Some(T::ExclusiveOnOnly),
                }
            },
        }),
        LoadMappingSnapshot => T::LoadMappingSnapshots(LoadMappingSnapshotsTarget {
            commons,
            tags: convert_tags(&data.tags, style),
            active_mappings_only: Some(data.active_mappings_only),
        }),
        NavigateWithinGroup => T::CycleThroughGroupMappings(CycleThroughGroupMappingsTarget {
            commons,
            exclusivity: {
                use schema::GroupMappingExclusivity as T;
                use Exclusivity::*;
                match data.exclusivity {
                    NonExclusive => None,
                    Exclusive | ExclusiveOnOnly => Some(T::Exclusive),
                }
            },
            group: style.required_value(data.group_id.into()),
        }),
    };
    Ok(target)
}

fn convert_commons(
    unit: TargetUnit,
    style: ConversionStyle,
) -> ConversionResult<schema::TargetCommons> {
    let commons = schema::TargetCommons {
        unit: {
            use schema::TargetUnit as T;
            use TargetUnit::*;
            let unit = match unit {
                Native => T::Native,
                Percent => T::Percent,
            };
            style.required_value(unit)
        },
    };
    Ok(commons)
}

fn convert_automation_mode_override(
    r#type: AutomationModeOverrideType,
    mode: RealearnAutomationMode,
) -> Option<schema::AutomationModeOverride> {
    use schema::AutomationModeOverride as T;
    match r#type {
        AutomationModeOverrideType::None => None,
        AutomationModeOverrideType::Bypass => Some(T::Bypass),
        AutomationModeOverrideType::Override => Some(T::Mode {
            mode: convert_automation_mode(mode),
        }),
    }
}

fn convert_transport_action(transport_action: TransportAction) -> schema::TransportAction {
    use schema::TransportAction as T;
    use TransportAction::*;
    match transport_action {
        PlayStop => T::PlayStop,
        PlayPause => T::PlayPause,
        Stop => T::Stop,
        Pause => T::Pause,
        RecordStop => T::Record,
        Repeat => T::Repeat,
    }
}

fn convert_any_on_parameter(parameter: AnyOnParameter) -> schema::AnyOnParameter {
    use schema::AnyOnParameter as T;
    use AnyOnParameter::*;
    match parameter {
        TrackSolo => T::TrackSolo,
        TrackMute => T::TrackMute,
        TrackArm => T::TrackArm,
        TrackSelection => T::TrackSelection,
    }
}

fn convert_automation_mode(mode: RealearnAutomationMode) -> schema::AutomationMode {
    use schema::AutomationMode as T;
    use RealearnAutomationMode::*;
    match mode {
        TrimRead => T::TrimRead,
        Read => T::Read,
        Touch => T::Touch,
        Write => T::Write,
        Latch => T::Latch,
        LatchPreview => T::LatchPreview,
    }
}

fn convert_track_exclusivity(exclusivity: TrackExclusivity) -> Option<schema::TrackExclusivity> {
    use schema::TrackExclusivity as T;
    use TrackExclusivity::*;
    match exclusivity {
        NonExclusive => None,
        ExclusiveWithinProject => Some(T::WithinProject),
        ExclusiveWithinFolder => Some(T::WithinFolder),
        ExclusiveWithinProjectOnOnly => Some(T::WithinProjectOnOnly),
        ExclusiveWithinFolderOnOnly => Some(T::WithinFolderOnOnly),
    }
}

fn convert_fx_display_kind(
    display_type: FxDisplayType,
    style: ConversionStyle,
) -> Option<schema::FxDisplayKind> {
    use schema::FxDisplayKind as T;
    use FxDisplayType::*;
    let v = match display_type {
        FloatingWindow => T::FloatingWindow,
        Chain => T::Chain,
    };
    style.required_value(v)
}

fn convert_virtual_target(data: TargetModelData, style: ConversionStyle) -> schema::Target {
    schema::Target::Virtual(schema::VirtualTarget {
        id: convert_control_element_id(data.control_element_index),
        character: convert_control_element_kind(data.control_element_type, style),
    })
}

fn convert_track_descriptor(
    data: TrackData,
    only_if_track_selected: bool,
    style: ConversionStyle,
) -> Option<schema::TrackDescriptor> {
    let props = deserialize_track(&data);
    use schema::TrackDescriptor as T;
    use VirtualTrackType::*;
    let commons = schema::TrackDescriptorCommons {
        track_must_be_selected: style.required_value_with_default(
            only_if_track_selected,
            defaults::TARGET_TRACK_MUST_BE_SELECTED,
        ),
    };
    let desc = match props.r#type {
        This => T::This { commons },
        Selected | AllSelected => T::Selected {
            allow_multiple: style.required_value_with_default(
                props.r#type == AllSelected,
                defaults::TARGET_TRACK_SELECTED_ALLOW_MULTIPLE,
            ),
        },
        Dynamic => T::Dynamic {
            commons,
            expression: props.expression,
        },
        Master => T::Master { commons },
        ById | ByIdOrName => T::ById {
            commons,
            id: props.id.map(|guid| guid.to_string_without_braces()),
        },
        ByName | AllByName => T::ByName {
            commons,
            name: props.name,
            allow_multiple: style.required_value_with_default(
                props.r#type == AllByName,
                defaults::TARGET_BY_NAME_ALLOW_MULTIPLE,
            ),
        },
        ByIndex => T::ByIndex {
            commons,
            index: props.index,
        },
    };
    style.required_value(desc)
}

fn convert_fx_chain_descriptor(
    data: TargetModelData,
    style: ConversionStyle,
) -> schema::FxChainDescriptor {
    schema::FxChainDescriptor::Track {
        track: convert_track_descriptor(
            data.track_data,
            data.enable_only_if_track_is_selected,
            style,
        ),
        chain: convert_fx_chain(data.fx_data.is_input_fx, style),
    }
}

fn convert_fx_chain(is_input_fx: bool, style: ConversionStyle) -> Option<schema::TrackFxChain> {
    let chain = if is_input_fx {
        schema::TrackFxChain::Input
    } else {
        schema::TrackFxChain::Normal
    };
    style.required_value(chain)
}

fn convert_fx_parameter_descriptor(
    data: TargetModelData,
    style: ConversionStyle,
) -> schema::FxParameterDescriptor {
    let props = deserialize_fx_parameter(&data.fx_parameter_data);
    use schema::FxParameterDescriptor as T;
    use VirtualFxParameterType::*;
    match props.r#type {
        Dynamic => T::Dynamic {
            expression: props.expression,
            fx: convert_fx_descriptor(data, style),
        },
        ByName => T::ByName {
            name: props.name,
            fx: convert_fx_descriptor(data, style),
        },
        ById => T::ById {
            index: props.index,
            fx: convert_fx_descriptor(data, style),
        },
        ByIndex => T::ByIndex {
            index: props.index,
            fx: convert_fx_descriptor(data, style),
        },
    }
}

fn convert_route_descriptor(
    data: TargetModelData,
    style: ConversionStyle,
) -> schema::RouteDescriptor {
    let props = deserialize_track_route(&data.track_route_data);
    use schema::RouteDescriptor as T;
    use TrackRouteSelectorType::*;
    let commons = schema::RouteDescriptorCommons {
        track: convert_track_descriptor(
            data.track_data,
            data.enable_only_if_track_is_selected,
            style,
        ),
        kind: {
            use schema::TrackRouteKind as T;
            use TrackRouteType::*;
            let kind = match data.track_route_data.r#type {
                Send => T::Send,
                Receive => T::Receive,
                HardwareOutput => T::HardwareOutput,
            };
            style.required_value(kind)
        },
    };
    match props.selector_type {
        Dynamic => T::Dynamic {
            commons,
            expression: props.expression,
        },
        ByName => T::ByName {
            commons,
            name: props.name,
        },
        ById => T::ById {
            commons,
            id: props.id.map(|guid| guid.to_string_without_braces()),
        },
        ByIndex => T::ByIndex {
            commons,
            index: props.index,
        },
    }
}

fn convert_fx_descriptor(
    data: TargetModelData,
    style: ConversionStyle,
) -> Option<schema::FxDescriptor> {
    let props = deserialize_fx(&data.fx_data, None);
    use schema::FxDescriptor as T;
    use VirtualFxType::*;
    let commons = schema::FxDescriptorCommons {
        fx_must_have_focus: style.required_value_with_default(
            data.enable_only_if_fx_has_focus,
            defaults::TARGET_FX_MUST_HAVE_FOCUS,
        ),
    };
    let v = match props.r#type {
        This => T::This { commons },
        Focused => T::Focused,
        Dynamic => T::Dynamic {
            commons,
            expression: props.expression,
            chain: convert_fx_chain_descriptor(data, style),
        },
        ById | ByIdOrIndex => T::ById {
            commons,
            id: props.id.map(|guid| guid.to_string_without_braces()),
            chain: convert_fx_chain_descriptor(data, style),
        },
        ByName | AllByName => T::ByName {
            commons,
            name: props.name,
            allow_multiple: style.required_value_with_default(
                props.r#type == AllByName,
                defaults::TARGET_BY_NAME_ALLOW_MULTIPLE,
            ),
            chain: convert_fx_chain_descriptor(data, style),
        },
        ByIndex => T::ByIndex {
            commons,
            index: props.index,
            chain: convert_fx_chain_descriptor(data, style),
        },
    };
    style.required_value(v)
}

fn convert_feedback_resolution(
    r: FeedbackResolution,
    style: ConversionStyle,
) -> Option<schema::FeedbackResolution> {
    use schema::FeedbackResolution as T;
    use FeedbackResolution::*;
    let v = match r {
        Beat => T::Beat,
        High => T::High,
    };
    style.required_value(v)
}
