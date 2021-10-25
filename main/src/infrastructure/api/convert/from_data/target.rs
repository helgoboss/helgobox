use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, RealearnAutomationMode, RealearnTrackArea,
    TargetCategory, TargetUnit, VirtualFxParameterType, VirtualFxType, VirtualTrackType,
};
use crate::domain::{
    ActionInvocationType, FxDisplayType, ReaperTargetType, TouchedParameterType, TrackExclusivity,
    TransportAction,
};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_control_element_kind,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::api::schema::{
    AutomationModeOverrideTarget, BookmarkDescriptor, BookmarkRef, CycleThroughFxTarget,
    FxParameterValueTarget, GoToBookmarkTarget, LastTouchedTarget, ReaperActionTarget,
    TrackAutomationModeTarget, TrackAutomationTouchStateTarget, TrackVisibilityTarget,
    TransportActionTarget,
};
use crate::infrastructure::data::{
    deserialize_fx, deserialize_fx_parameter, deserialize_track, TargetModelData, TrackData,
};

pub fn convert_target(data: TargetModelData) -> ConversionResult<schema::Target> {
    use TargetCategory::*;
    match data.category {
        Reaper => convert_real_target(data),
        Virtual => Ok(convert_virtual_target(data)),
    }
}

fn convert_real_target(data: TargetModelData) -> ConversionResult<schema::Target> {
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
                    BookmarkDescriptor::Region(bookmark_ref)
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
        SelectedTrack => T::CycleThroughTracks(Default::default()),
        Seek => T::Seek(Default::default()),
        Playrate => T::PlayRate(Default::default()),
        Tempo => T::Tempo(Default::default()),
        TrackArm => T::TrackArmState(Default::default()),
        AllTrackFxEnable => T::AllTrackFxOnOffState(Default::default()),
        TrackMute => T::TrackMuteState(Default::default()),
        TrackPeak => T::TrackPeak(Default::default()),
        TrackPhase => T::TrackPhase(Default::default()),
        TrackSelection => T::TrackSelectionState(Default::default()),
        TrackPan => T::TrackPan(Default::default()),
        TrackWidth => T::TrackWidth(Default::default()),
        TrackVolume => T::TrackVolume(Default::default()),
        TrackSolo => T::TrackSoloState(Default::default()),
        FxEnable => T::FxOnOffState(Default::default()),
        LoadFxSnapshot => T::LoadFxSnapshot(Default::default()),
        FxPreset => T::CycleThroughFxPresets(Default::default()),
        FxOpen => T::FxVisibility(Default::default()),
        TrackSendAutomationMode => T::SendAutomationMode(Default::default()),
        TrackSendMono => T::SendMonoState(Default::default()),
        TrackSendMute => T::SendMuteState(Default::default()),
        TrackSendPhase => T::SendPhase(Default::default()),
        TrackSendPan => T::SendPan(Default::default()),
        TrackSendVolume => T::SendVolume(Default::default()),
        ClipTransport => T::ClipTransportAction(Default::default()),
        ClipSeek => T::ClipSeek(Default::default()),
        ClipVolume => T::ClipVolume(Default::default()),
        SendMidi => T::SendMidi(Default::default()),
        SendOsc => T::SendOsc(Default::default()),
        EnableInstances => T::EnableInstances(Default::default()),
        EnableMappings => T::EnableMappings(Default::default()),
        LoadMappingSnapshot => T::LoadMappingSnapshots(Default::default()),
        NavigateWithinGroup => T::CycleThroughGroupMappings(Default::default()),
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

fn convert_fx_descriptor(data: TargetModelData) -> schema::FxDescriptor {
    let props = deserialize_fx(&data.fx_data, None);
    use schema::FxDescriptor as T;
    use VirtualFxType::*;
    let commons = schema::FxDescriptorCommons {
        fx_must_have_focus: Some(data.enable_only_if_fx_has_focus),
    };
    match props.r#type {
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
    }
}
