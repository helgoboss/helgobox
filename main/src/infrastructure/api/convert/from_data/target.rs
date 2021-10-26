use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, RealearnAutomationMode, RealearnTrackArea,
    TargetCategory, TargetUnit, TrackRouteSelectorType, VirtualFxParameterType, VirtualFxType,
    VirtualTrackType,
};
use crate::domain::{
    ActionInvocationType, Exclusivity, FeedbackResolution, FxDisplayType, GroupId,
    ReaperTargetType, SendMidiDestination, SoloBehavior, TouchedParameterType, TrackExclusivity,
    TrackRouteType, TransportAction,
};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_control_element_kind, convert_group_id,
    convert_osc_argument, convert_tags,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::api::schema::{
    AllTrackFxOnOffStateTarget, AutomationModeOverrideTarget, BookmarkDescriptor, BookmarkRef,
    ClipDescriptor, ClipOutput, ClipSeekTarget, ClipTransportActionTarget, ClipVolumeTarget,
    CycleThroughFxPresetsTarget, CycleThroughFxTarget, CycleThroughGroupMappingsTarget,
    CycleThroughTracksTarget, EnableInstancesTarget, EnableMappingsTarget, FxOnOffStateTarget,
    FxParameterValueTarget, FxVisibilityTarget, GoToBookmarkTarget, LastTouchedTarget,
    LoadFxSnapshotTarget, LoadMappingSnapshotsTarget, PlayRateTarget, ReaperActionTarget,
    SeekTarget, SendAutomationModeTarget, SendMidiTarget, SendMonoStateTarget, SendMuteStateTarget,
    SendOscTarget, SendPanTarget, SendPhaseTarget, SendVolumeTarget, TempoTarget,
    TrackArmStateTarget, TrackAutomationModeTarget, TrackAutomationTouchStateTarget,
    TrackMuteStateTarget, TrackPanTarget, TrackPeakTarget, TrackPhaseTarget,
    TrackSelectionStateTarget, TrackSoloStateTarget, TrackVisibilityTarget, TrackVolumeTarget,
    TrackWidthTarget, TransportActionTarget,
};
use crate::infrastructure::data::{
    deserialize_fx, deserialize_fx_parameter, deserialize_track, deserialize_track_route,
    TargetModelData, TrackData,
};

pub fn convert_target(
    data: TargetModelData,
    group_key_by_id: impl Fn(GroupId) -> Option<String>,
) -> ConversionResult<schema::Target> {
    use TargetCategory::*;
    match data.category {
        Reaper => convert_real_target(data, group_key_by_id),
        Virtual => Ok(convert_virtual_target(data)),
    }
}

fn convert_real_target(
    data: TargetModelData,
    group_key_by_id: impl Fn(GroupId) -> Option<String>,
) -> ConversionResult<schema::Target> {
    use schema::Target as T;
    use ReaperTargetType::*;
    let commons = convert_commons(data.unit)?;
    let target = match data.r#type {
        LastTouched => T::LastTouched(LastTouchedTarget { commons }),
        AutomationModeOverride => T::AutomationModeOverride(AutomationModeOverrideTarget {
            commons,
            r#override: convert_automation_mode_override(
                data.automation_mode_override_type,
                data.track_automation_mode,
            ),
        }),
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
                Some(v)
            },
            track: if data.with_track {
                convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected)
            } else {
                None
            },
        }),
        Transport => T::TransportAction(TransportActionTarget {
            commons,
            action: convert_transport_action(data.transport_action),
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
            set_time_selection: Some(data.seek_options.use_time_selection),
            set_loop_points: Some(data.seek_options.use_loop_points),
        }),
        TrackAutomationMode => T::TrackAutomationMode(TrackAutomationModeTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            mode: convert_automation_mode(data.track_automation_mode),
        }),
        AutomationTouchState => T::TrackAutomationTouchState(TrackAutomationTouchStateTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
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
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: Some(data.poll_for_feedback),
            area: {
                match data.track_area {
                    RealearnTrackArea::Tcp => schema::TrackArea::Tcp,
                    RealearnTrackArea::Mcp => schema::TrackArea::Mcp,
                }
            },
        }),
        FxNavigate => T::CycleThroughFx(CycleThroughFxTarget {
            commons,
            display_kind: convert_fx_display_kind(data.fx_display_type),
            chain: convert_fx_chain_descriptor(data),
        }),
        FxParameter => T::FxParameterValue(FxParameterValueTarget {
            commons,
            poll_for_feedback: Some(data.poll_for_feedback),
            parameter: convert_fx_parameter_descriptor(data),
        }),
        TrackSendAutomationMode => T::SendAutomationMode(SendAutomationModeTarget {
            commons,
            mode: convert_automation_mode(data.track_automation_mode),
            poll_for_feedback: Some(data.poll_for_feedback),
            route: convert_route_descriptor(data),
        }),
        TrackSendMono => T::SendMonoState(SendMonoStateTarget {
            commons,
            poll_for_feedback: Some(data.poll_for_feedback),
            route: convert_route_descriptor(data),
        }),
        TrackSendMute => T::SendMuteState(SendMuteStateTarget {
            commons,
            poll_for_feedback: Some(data.poll_for_feedback),
            route: convert_route_descriptor(data),
        }),
        TrackSendPhase => T::SendPhase(SendPhaseTarget {
            commons,
            poll_for_feedback: Some(data.poll_for_feedback),
            route: convert_route_descriptor(data),
        }),
        TrackSendPan => T::SendPan(SendPanTarget {
            commons,
            route: convert_route_descriptor(data),
        }),
        TrackSendVolume => T::SendVolume(SendVolumeTarget {
            commons,
            route: convert_route_descriptor(data),
        }),
        ClipTransport => T::ClipTransportAction(ClipTransportActionTarget {
            commons,
            output: {
                let output = ClipOutput::Track {
                    track: convert_track_descriptor(
                        data.track_data,
                        data.enable_only_if_track_is_selected,
                    ),
                };
                Some(output)
            },
            clip: convert_clip_descriptor(data.slot_index),
            action: convert_transport_action(data.transport_action),
            next_bar: Some(data.next_bar),
            buffered: Some(data.buffered),
        }),
        ClipSeek => T::ClipSeek(ClipSeekTarget {
            commons,
            clip: convert_clip_descriptor(data.slot_index),
            feedback_resolution: convert_feedback_resolution(data.seek_options.feedback_resolution),
        }),
        ClipVolume => T::ClipVolume(ClipVolumeTarget {
            commons,
            clip: convert_clip_descriptor(data.slot_index),
        }),
        SendMidi => T::SendMidi(SendMidiTarget {
            commons,
            message: Some(data.raw_midi_pattern),
            destination: {
                use schema::MidiDestination as T;
                use SendMidiDestination::*;
                let dest = match data.send_midi_destination {
                    FxOutput => T::FxOutput,
                    FeedbackOutput => T::FeedbackOutput,
                };
                Some(dest)
            },
        }),
        SelectedTrack => T::CycleThroughTracks(CycleThroughTracksTarget {
            commons,
            scroll_arrange_view: Some(data.scroll_arrange_view),
            scroll_mixer: Some(data.scroll_mixer),
        }),
        Seek => T::Seek(SeekTarget {
            commons,
            use_time_selection: Some(data.seek_options.use_time_selection),
            use_loop_points: Some(data.seek_options.use_loop_points),
            use_regions: Some(data.seek_options.use_regions),
            use_project: Some(data.seek_options.use_project),
            move_view: Some(data.seek_options.move_view),
            seek_play: Some(data.seek_options.seek_play),
            feedback_resolution: convert_feedback_resolution(data.seek_options.feedback_resolution),
        }),
        Playrate => T::PlayRate(PlayRateTarget { commons }),
        Tempo => T::Tempo(TempoTarget { commons }),
        TrackArm => T::TrackArmState(TrackArmStateTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
        }),
        AllTrackFxEnable => T::AllTrackFxOnOffState(AllTrackFxOnOffStateTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: Some(data.poll_for_feedback),
        }),
        TrackMute => T::TrackMuteState(TrackMuteStateTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
        }),
        TrackPeak => T::TrackPeak(TrackPeakTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
        }),
        TrackPhase => T::TrackPhase(TrackPhaseTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: Some(data.poll_for_feedback),
        }),
        TrackSelection => T::TrackSelectionState(TrackSelectionStateTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            scroll_arrange_view: Some(data.scroll_arrange_view),
            scroll_mixer: Some(data.scroll_mixer),
        }),
        TrackPan => T::TrackPan(TrackPanTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
        }),
        TrackWidth => T::TrackWidth(TrackWidthTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
        }),
        TrackVolume => T::TrackVolume(TrackVolumeTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
        }),
        TrackSolo => T::TrackSoloState(TrackSoloStateTarget {
            commons,
            track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            behavior: {
                use schema::SoloBehavior as T;
                use SoloBehavior::*;
                data.solo_behavior.map(|b| match b {
                    InPlace => T::InPlace,
                    IgnoreRouting => T::IgnoreRouting,
                    ReaperPreference => T::ReaperPreference,
                })
            },
        }),
        FxEnable => T::FxOnOffState(FxOnOffStateTarget {
            commons,
            fx: convert_fx_descriptor(data),
        }),
        LoadFxSnapshot => T::LoadFxSnapshot(LoadFxSnapshotTarget {
            commons,
            snapshot: {
                data.fx_snapshot.as_ref().map(|s| schema::FxSnapshot {
                    fx_kind: Some(s.fx_type.clone()),
                    fx_name: Some(s.fx_name.clone()),
                    preset_name: s.preset_name.clone(),
                    content: {
                        let v = schema::FxSnapshotContent::Chunk {
                            chunk: (*s.chunk).clone(),
                        };
                        Some(v)
                    },
                })
            },
            fx: convert_fx_descriptor(data),
        }),
        FxPreset => T::CycleThroughFxPresets(CycleThroughFxPresetsTarget {
            commons,
            fx: convert_fx_descriptor(data),
        }),
        FxOpen => T::FxVisibility(FxVisibilityTarget {
            commons,
            display_kind: convert_fx_display_kind(data.fx_display_type),
            fx: convert_fx_descriptor(data),
        }),
        SendOsc => T::SendOsc(SendOscTarget {
            commons,
            address: Some(data.osc_address_pattern),
            argument: convert_osc_argument(data.osc_arg_index, data.osc_arg_type, false),
            destination: {
                use schema::OscDestination as T;
                let v = match data.osc_dev_id {
                    None => T::FeedbackOutput,
                    Some(id) => T::Device { id: id.to_string() },
                };
                Some(v)
            },
        }),
        EnableInstances => T::EnableInstances(EnableInstancesTarget {
            commons,
            tags: convert_tags(&data.tags),
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
            tags: convert_tags(&data.tags),
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
            tags: convert_tags(&data.tags),
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
            group: convert_group_id(data.group_id, group_key_by_id),
        }),
    };
    Ok(target)
}

fn convert_commons(unit: TargetUnit) -> ConversionResult<schema::TargetCommons> {
    let commons = schema::TargetCommons {
        unit: {
            use schema::TargetUnit as T;
            use TargetUnit::*;
            let unit = match unit {
                Native => T::Native,
                Percent => T::Percent,
            };
            Some(unit)
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
        Record => T::Record,
        Repeat => T::Repeat,
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

fn convert_fx_display_kind(display_type: FxDisplayType) -> Option<schema::FxDisplayKind> {
    use schema::FxDisplayKind as T;
    use FxDisplayType::*;
    let v = match display_type {
        FloatingWindow => T::FloatingWindow,
        Chain => T::Chain,
    };
    Some(v)
}

fn convert_virtual_target(data: TargetModelData) -> schema::Target {
    schema::Target::Virtual(schema::VirtualTarget {
        id: convert_control_element_id(data.control_element_index),
        kind: convert_control_element_kind(data.control_element_type),
    })
}

fn convert_track_descriptor(
    data: TrackData,
    only_if_track_selected: bool,
) -> Option<schema::TrackDescriptor> {
    let props = deserialize_track(&data);
    use schema::TrackDescriptor as T;
    use VirtualTrackType::*;
    let commons = schema::TrackDescriptorCommons {
        track_must_be_selected: Some(only_if_track_selected),
    };
    let desc = match props.r#type {
        This => T::This { commons },
        Selected | AllSelected => T::Selected {
            allow_multiple: Some(props.r#type == AllSelected),
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
            allow_multiple: Some(props.r#type == AllByName),
        },
        ByIndex => T::ByIndex {
            commons,
            index: props.index,
        },
    };
    Some(desc)
}

fn convert_fx_chain_descriptor(data: TargetModelData) -> schema::FxChainDescriptor {
    schema::FxChainDescriptor::Track {
        track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
        chain: {
            let chain = if data.fx_data.is_input_fx {
                schema::TrackFxChain::Input
            } else {
                schema::TrackFxChain::Normal
            };
            Some(chain)
        },
    }
}

fn convert_fx_parameter_descriptor(data: TargetModelData) -> schema::FxParameterDescriptor {
    let props = deserialize_fx_parameter(&data.fx_parameter_data);
    use schema::FxParameterDescriptor as T;
    use VirtualFxParameterType::*;
    match props.r#type {
        Dynamic => T::Dynamic {
            expression: props.expression,
            fx: convert_fx_descriptor(data),
        },
        ByName => T::ByName {
            name: props.name,
            fx: convert_fx_descriptor(data),
        },
        ById => T::ById {
            index: props.index,
            fx: convert_fx_descriptor(data),
        },
        ByIndex => T::ByIndex {
            index: props.index,
            fx: convert_fx_descriptor(data),
        },
    }
}

fn convert_route_descriptor(data: TargetModelData) -> schema::RouteDescriptor {
    let props = deserialize_track_route(&data.track_route_data);
    use schema::RouteDescriptor as T;
    use TrackRouteSelectorType::*;
    let commons = schema::RouteDescriptorCommons {
        track: convert_track_descriptor(data.track_data, data.enable_only_if_track_is_selected),
        kind: {
            use schema::TrackRouteKind as T;
            use TrackRouteType::*;
            let kind = match data.track_route_data.r#type {
                Send => T::Send,
                Receive => T::Receive,
                HardwareOutput => T::HardwareOutput,
            };
            Some(kind)
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

fn convert_fx_descriptor(data: TargetModelData) -> Option<schema::FxDescriptor> {
    let props = deserialize_fx(&data.fx_data, None);
    use schema::FxDescriptor as T;
    use VirtualFxType::*;
    let commons = schema::FxDescriptorCommons {
        fx_must_have_focus: Some(data.enable_only_if_fx_has_focus),
    };
    let v = match props.r#type {
        This => T::This { commons },
        Focused => T::Focused,
        Dynamic => T::Dynamic {
            commons,
            expression: props.expression,
            chain: convert_fx_chain_descriptor(data),
        },
        ById | ByIdOrIndex => T::ById {
            commons,
            id: props.id.map(|guid| guid.to_string_without_braces()),
            chain: convert_fx_chain_descriptor(data),
        },
        ByName | AllByName => T::ByName {
            commons,
            name: props.name,
            allow_multiple: Some(props.r#type == AllByName),
            chain: convert_fx_chain_descriptor(data),
        },
        ByIndex => T::ByIndex {
            commons,
            index: props.index,
            chain: convert_fx_chain_descriptor(data),
        },
    };
    Some(v)
}

fn convert_clip_descriptor(slot_index: usize) -> schema::ClipDescriptor {
    ClipDescriptor::Slot {
        index: slot_index as _,
    }
}

fn convert_feedback_resolution(r: FeedbackResolution) -> Option<schema::FeedbackResolution> {
    use schema::FeedbackResolution as T;
    use FeedbackResolution::*;
    let v = match r {
        Beat => T::Beat,
        High => T::High,
    };
    Some(v)
}
