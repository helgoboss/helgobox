use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, MappingModificationKind,
    RealearnAutomationMode, RealearnTrackArea, TargetCategory, TargetUnit, TrackRouteSelectorType,
    VirtualFxParameterType, VirtualFxType, VirtualTrackType,
};
use crate::domain::{
    ActionInvocationType, AnyOnParameter, Exclusivity, FeedbackResolution, FxDisplayType,
    ReaperTargetType, SendMidiDestinationType, SoloBehavior, TouchedRouteParameterType,
    TouchedTrackParameterType, TrackExclusivity, TrackRouteType, TransportAction,
};
use crate::infrastructure::api::convert::from_data::{
    convert_control_element_id, convert_osc_argument, convert_tags, ConversionStyle,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::{
    deserialize_fx, deserialize_fx_parameter, deserialize_track, deserialize_track_route,
    MigrationDescriptor, TargetModelData, TrackData, TrackDeserializationInput,
};
use base::hash_util::convert_into_other_hash_set;
use helgobox_api::persistence;
use helgobox_api::persistence::{
    AllTrackFxOnOffStateTarget, AnyOnTarget, AutomationModeOverrideTarget,
    BackwardCompatibleMappingSnapshotDescForTake, BookmarkDescriptor, BookmarkRef,
    BrowseFxChainTarget, BrowseFxPresetsTarget, BrowseGroupMappingsTarget,
    BrowsePotFilterItemsTarget, BrowsePotPresetsTarget, BrowseTracksTarget,
    CompartmentParameterDescriptor, CompartmentParameterValueTarget, DummyTarget,
    EnableInstancesTarget, EnableMappingsTarget, FxOnOffStateTarget, FxOnlineOfflineStateTarget,
    FxParameterAutomationTouchStateTarget, FxParameterValueTarget, FxToolTarget,
    FxVisibilityTarget, GoToBookmarkTarget, InputDeviceMidiDestination, LastTouchedTarget,
    LearnTargetMappingModification, LoadFxSnapshotTarget, LoadMappingSnapshotTarget,
    LoadPotPresetTarget, MappingModification, ModifyMappingTarget, MouseTarget, PlayRateTarget,
    PreviewPotPresetTarget, ReaperActionTarget, RouteAutomationModeTarget, RouteMonoStateTarget,
    RouteMuteStateTarget, RoutePanTarget, RoutePhaseTarget, RouteTouchStateTarget,
    RouteVolumeTarget, SeekTarget, SendMidiTarget, SendOscTarget,
    SetTargetToLastTouchedMappingModification, StreamDeckBrightnessTarget,
    TakeMappingSnapshotTarget, TempoTarget, TrackArmStateTarget, TrackAutomationModeTarget,
    TrackAutomationTouchStateTarget, TrackMonitoringModeTarget, TrackMuteStateTarget,
    TrackPanTarget, TrackParentSendStateTarget, TrackPeakTarget, TrackPhaseTarget,
    TrackSelectionStateTarget, TrackSoloStateTarget, TrackToolTarget, TrackVisibilityTarget,
    TrackVolumeTarget, TrackWidthTarget, TransportActionTarget,
};

pub fn convert_target(
    data: TargetModelData,
    style: ConversionStyle,
) -> ConversionResult<persistence::Target> {
    use TargetCategory::*;
    match data.category {
        Reaper => convert_real_target(data, style),
        Virtual => Ok(convert_virtual_target(data, style)),
    }
}

fn convert_real_target(
    data: TargetModelData,
    style: ConversionStyle,
) -> ConversionResult<persistence::Target> {
    use persistence::Target as T;
    use ReaperTargetType::*;
    let commons = convert_commons(data.unit, style)?;
    let target = match data.r#type {
        Mouse => T::Mouse(MouseTarget {
            commons,
            action: data.mouse_action,
        }),
        LastTouched => T::LastTouched(LastTouchedTarget {
            commons,
            included_targets: data.included_targets.map(|s| s.into_iter().collect()),
            touch_cause: style.required_value(data.touch_cause),
        }),
        AutomationModeOverride => {
            let t = AutomationModeOverrideTarget {
                commons,
                override_value: convert_automation_mode_override(
                    data.automation_mode_override_type,
                    data.track_automation_mode,
                ),
            };
            T::AutomationModeOverride(t)
        }
        Action => T::ReaperAction(ReaperActionTarget {
            commons,
            scope: style.required_value(data.action_scope),
            command: {
                if let Some(n) = data.command_name {
                    let v = match n.parse::<u32>() {
                        Ok(id) => persistence::ReaperCommand::Id(id),
                        Err(_) => persistence::ReaperCommand::Name(n),
                    };
                    Some(v)
                } else {
                    None
                }
            },
            invocation: {
                use persistence::ActionInvocationKind as T;
                use ActionInvocationType::*;
                let v = match data.invocation_type {
                    Trigger => T::Trigger,
                    Absolute14Bit => T::Absolute14Bit,
                    Absolute7Bit => T::Absolute7Bit,
                    Relative => T::Relative,
                };
                style.required_value(v)
            },
            track: if data.with_track {
                convert_track_descriptor(
                    data.track_data,
                    data.enable_only_if_track_is_selected,
                    &data.clip_column,
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
            seek_behavior: style.optional_value(data.seek_behavior),
        }),
        TrackAutomationMode => T::TrackAutomationMode(TrackAutomationModeTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            mode: convert_automation_mode(data.track_automation_mode),
        }),
        TrackMonitoringMode => T::TrackMonitoringMode(TrackMonitoringModeTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            mode: data.track_monitoring_mode,
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        TrackTouchState => T::TrackAutomationTouchState(TrackAutomationTouchStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
            touched_parameter: {
                use persistence::TouchedTrackParameter as T;
                use TouchedTrackParameterType::*;
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
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            poll_for_feedback: None,
            area: {
                match data.track_area {
                    RealearnTrackArea::Tcp => persistence::TrackArea::Tcp,
                    RealearnTrackArea::Mcp => persistence::TrackArea::Mcp,
                }
            },
        }),
        BrowseFxs => T::BrowseFxChain(BrowseFxChainTarget {
            commons,
            display_kind: convert_fx_display_kind(data.fx_display_type, style),
            chain: convert_fx_chain_descriptor(data, style),
        }),
        FxParameterValue => T::FxParameterValue(FxParameterValueTarget {
            commons,
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            retrigger: style
                .required_value_with_default(data.retrigger, defaults::TARGET_RETRIGGER),
            real_time: style
                .required_value_with_default(data.real_time, defaults::TARGET_REAL_TIME),
            parameter: convert_fx_parameter_descriptor(data, style),
        }),
        CompartmentParameterValue => {
            T::CompartmentParameterValue(CompartmentParameterValueTarget {
                commons,
                parameter: CompartmentParameterDescriptor::ById {
                    index: data.fx_parameter_data.index,
                },
            })
        }
        FxParameterTouchState => {
            T::FxParameterAutomationTouchState(FxParameterAutomationTouchStateTarget {
                commons,
                parameter: convert_fx_parameter_descriptor(data, style),
            })
        }
        RouteAutomationMode => T::RouteAutomationMode(RouteAutomationModeTarget {
            commons,
            mode: convert_automation_mode(data.track_automation_mode),
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            route: convert_route_descriptor(data, style),
        }),
        RouteMono => T::RouteMonoState(RouteMonoStateTarget {
            commons,
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            route: convert_route_descriptor(data, style),
        }),
        RouteMute => T::RouteMuteState(RouteMuteStateTarget {
            commons,
            poll_for_feedback: Some(data.poll_for_feedback),
            route: convert_route_descriptor(data, style),
        }),
        RoutePhase => T::RoutePhase(RoutePhaseTarget {
            commons,
            poll_for_feedback: style.required_value_with_default(
                data.poll_for_feedback,
                defaults::TARGET_POLL_FOR_FEEDBACK,
            ),
            route: convert_route_descriptor(data, style),
        }),
        RoutePan => T::RoutePan(RoutePanTarget {
            commons,
            route: convert_route_descriptor(data, style),
        }),
        RouteVolume => T::RouteVolume(RouteVolumeTarget {
            commons,
            route: convert_route_descriptor(data, style),
        }),
        RouteTouchState => T::RouteTouchState(RouteTouchStateTarget {
            commons,
            touched_parameter: {
                use persistence::TouchedRouteParameter as T;
                use TouchedRouteParameterType::*;
                match data.touched_route_parameter_type {
                    Volume => T::Volume,
                    Pan => T::Pan,
                }
            },
            route: convert_route_descriptor(data, style),
        }),
        PlaytimeSlotTransportAction => T::PlaytimeSlotTransportAction(
            helgobox_api::persistence::PlaytimeSlotTransportActionTarget {
                commons,
                slot: data.clip_slot.unwrap_or_default(),
                action: data.clip_transport_action.unwrap_or_default(),
                stop_column_if_slot_empty: style.required_value_with_default(
                    data.stop_column_if_slot_empty,
                    defaults::TARGET_STOP_COLUMN_IF_SLOT_EMPTY,
                ),
            },
        ),
        PlaytimeColumnAction => {
            T::PlaytimeColumnAction(helgobox_api::persistence::PlaytimeColumnActionTarget {
                commons,
                column: data.clip_column,
                action: data.clip_column_action,
            })
        }
        PlaytimeRowAction => {
            T::PlaytimeRowAction(helgobox_api::persistence::PlaytimeRowActionTarget {
                commons,
                row: data.clip_row,
                action: data.clip_row_action,
            })
        }
        PlaytimeMatrixAction => {
            T::PlaytimeMatrixAction(helgobox_api::persistence::PlaytimeMatrixActionTarget {
                commons,
                action: data.clip_matrix_action,
            })
        }
        PlaytimeControlUnitScroll => T::PlaytimeControlUnitScroll(
            helgobox_api::persistence::PlaytimeControlUnitScrollTarget {
                commons,
                axis: data.axis,
            },
        ),
        PlaytimeBrowseCells => {
            T::PlaytimeBrowseCells(helgobox_api::persistence::PlaytimeBrowseCellsTarget {
                commons,
                axis: data.axis,
            })
        }
        PlaytimeSlotSeek => {
            T::PlaytimeSlotSeek(helgobox_api::persistence::PlaytimeSlotSeekTarget {
                commons,
                slot: data.clip_slot.unwrap_or_default(),
                feedback_resolution: convert_feedback_resolution(
                    data.seek_options.feedback_resolution,
                    style,
                ),
            })
        }
        PlaytimeSlotVolume => {
            T::PlaytimeSlotVolume(helgobox_api::persistence::PlaytimeSlotVolumeTarget {
                commons,
                slot: data.clip_slot.unwrap_or_default(),
            })
        }
        PlaytimeSlotManagementAction => T::PlaytimeSlotManagementAction(
            helgobox_api::persistence::PlaytimeSlotManagementActionTarget {
                commons,
                slot: data.clip_slot.unwrap_or_default(),
                action: data.clip_management_action,
            },
        ),
        SendMidi => T::SendMidi(SendMidiTarget {
            commons,
            message: style.required_value(data.raw_midi_pattern),
            destination: {
                use persistence::SendMidiDestination as T;
                use SendMidiDestinationType::*;
                let dest = match data.send_midi_destination {
                    FxOutput => T::FxOutput,
                    FeedbackOutput => T::FeedbackOutput,
                    InputDevice => T::InputDevice(InputDeviceMidiDestination {
                        device_id: data.midi_input_device_id.map(|id| id.get()),
                    }),
                };
                style.required_value(dest)
            },
        }),
        Dummy => T::Dummy(DummyTarget { commons }),
        BrowseTracks => T::BrowseTracks(BrowseTracksTarget {
            commons,
            scroll_arrange_view: style.required_value_with_default(
                data.scroll_arrange_view,
                defaults::TARGET_TRACK_SELECTION_SCROLL_ARRANGE_VIEW,
            ),
            scroll_mixer: style.required_value_with_default(
                data.scroll_mixer,
                defaults::TARGET_TRACK_SELECTION_SCROLL_MIXER,
            ),
            mode: style.required_value(data.browse_tracks_mode),
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
            behavior: style.optional_value(data.seek_behavior),
        }),
        PlayRate => T::PlayRate(PlayRateTarget { commons }),
        Tempo => T::Tempo(TempoTarget { commons }),
        TrackArm => T::TrackArmState(TrackArmStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        TrackParentSend => T::TrackParentSendState(TrackParentSendStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
        }),
        AllTrackFxEnable => T::AllTrackFxOnOffState(AllTrackFxOnOffStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
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
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        TrackPeak => T::TrackPeak(TrackPeakTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
        }),
        TrackPhase => T::TrackPhase(TrackPhaseTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
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
                &data.clip_column,
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
                &data.clip_column,
                style,
            ),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        TrackWidth => T::TrackWidth(TrackWidthTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        TrackVolume => T::TrackVolume(TrackVolumeTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        TrackTool => T::TrackTool(TrackToolTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            action: Some(data.track_tool_action),
            instance_tags: convert_tags(&data.tags, style),
        }),
        TrackSolo => T::TrackSoloState(TrackSoloStateTarget {
            commons,
            track: convert_track_descriptor(
                data.track_data,
                data.enable_only_if_track_is_selected,
                &data.clip_column,
                style,
            ),
            exclusivity: convert_track_exclusivity(data.track_exclusivity),
            behavior: {
                use persistence::SoloBehavior as T;
                use SoloBehavior::*;
                let v = data.solo_behavior.map(|b| match b {
                    InPlace => T::InPlace,
                    IgnoreRouting => T::IgnoreRouting,
                    ReaperPreference => T::ReaperPreference,
                });
                style.optional_value(v)
            },
            use_track_grouping: style.optional_value_with_default(
                data.use_track_grouping,
                defaults::TARGET_USE_TRACK_GROUPING,
            ),
            use_selection_ganging: style.optional_value_with_default(
                data.use_selection_ganging,
                defaults::TARGET_USE_SELECTION_GANGING,
            ),
        }),
        FxTool => T::FxTool(FxToolTarget {
            commons,
            action: Some(data.fx_tool_action),
            instance_tags: convert_tags(&data.tags, style),
            fx: convert_fx_descriptor(data, style),
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
                data.fx_snapshot.as_ref().map(|s| persistence::FxSnapshot {
                    fx_kind: style.required_value(s.fx_type.clone()),
                    fx_name: style.required_value(s.fx_name.clone()),
                    preset_name: style.optional_value(s.preset_name.clone()),
                    content: {
                        persistence::FxSnapshotContent::Chunk {
                            chunk: (*s.chunk).clone(),
                        }
                    },
                })
            },
            fx: convert_fx_descriptor(data, style),
        }),
        FxPreset => T::BrowseFxPresets(BrowseFxPresetsTarget {
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
            argument: convert_osc_argument(
                data.osc_arg_index,
                data.osc_arg_type,
                data.osc_arg_value_range,
                style,
            ),
            destination: {
                use persistence::OscDestination as T;
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
                use persistence::InstanceExclusivity as T;
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
                use persistence::MappingExclusivity as T;
                use Exclusivity::*;
                match data.exclusivity {
                    NonExclusive => None,
                    Exclusive => Some(T::Exclusive),
                    ExclusiveOnOnly => Some(T::ExclusiveOnOnly),
                }
            },
        }),
        ModifyMapping => T::ModifyMapping(ModifyMappingTarget {
            commons,
            modification: match data.mapping_modification_kind {
                MappingModificationKind::LearnTarget => {
                    MappingModification::LearnTarget(LearnTargetMappingModification {
                        included_targets: data.included_targets.map(|s| s.into_iter().collect()),
                        touch_cause: style.required_value(data.touch_cause),
                    })
                }
                MappingModificationKind::SetTargetToLastTouched => {
                    MappingModification::SetTargetToLastTouched(
                        SetTargetToLastTouchedMappingModification {
                            included_targets: data
                                .included_targets
                                .map(convert_into_other_hash_set),
                            touch_cause: style.required_value(data.touch_cause),
                        },
                    )
                }
            },
            session: data.session_id,
            mapping: data.mapping_key.map(|key| key.into()),
        }),
        LoadMappingSnapshot => T::LoadMappingSnapshot(LoadMappingSnapshotTarget {
            commons,
            tags: convert_tags(&data.tags, style),
            active_mappings_only: Some(data.active_mappings_only),
            snapshot: style.required_value(data.mapping_snapshot),
            default_value: data.mapping_snapshot_default_value,
        }),
        TakeMappingSnapshot => T::TakeMappingSnapshot(TakeMappingSnapshotTarget {
            commons,
            tags: convert_tags(&data.tags, style),
            active_mappings_only: Some(data.active_mappings_only),
            snapshot: data
                .take_mapping_snapshot
                .map(BackwardCompatibleMappingSnapshotDescForTake::New)
                .unwrap_or_default(),
        }),
        BrowseGroup => T::BrowseGroupMappings(BrowseGroupMappingsTarget {
            commons,
            exclusivity: {
                use persistence::GroupMappingExclusivity as T;
                use Exclusivity::*;
                match data.exclusivity {
                    NonExclusive => None,
                    Exclusive | ExclusiveOnOnly => Some(T::Exclusive),
                }
            },
            group: style.required_value(data.group_id.into()),
        }),
        BrowsePotFilterItems => T::BrowsePotFilterItems(BrowsePotFilterItemsTarget {
            commons,
            item_kind: style.required_value(data.pot_filter_item_kind),
        }),
        BrowsePotPresets => T::BrowsePotPresets(BrowsePotPresetsTarget { commons }),
        PreviewPotPreset => T::PreviewPotPreset(PreviewPotPresetTarget { commons }),
        LoadPotPreset => T::LoadPotPreset(LoadPotPresetTarget {
            commons,
            fx: convert_fx_descriptor(data, style),
        }),
        StreamDeckBrightness => T::StreamDeckBrightness(StreamDeckBrightnessTarget { commons }),
    };
    Ok(target)
}

fn convert_commons(
    unit: TargetUnit,
    style: ConversionStyle,
) -> ConversionResult<persistence::TargetCommons> {
    let commons = persistence::TargetCommons {
        unit: {
            use persistence::TargetUnit as T;
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
) -> Option<persistence::AutomationModeOverride> {
    use persistence::AutomationModeOverride as T;
    match r#type {
        AutomationModeOverrideType::None => None,
        AutomationModeOverrideType::Bypass => Some(T::Bypass),
        AutomationModeOverrideType::Override => Some(T::Mode {
            mode: convert_automation_mode(mode),
        }),
    }
}

fn convert_transport_action(transport_action: TransportAction) -> persistence::TransportAction {
    use persistence::TransportAction as T;
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

fn convert_any_on_parameter(parameter: AnyOnParameter) -> persistence::AnyOnParameter {
    use persistence::AnyOnParameter as T;
    use AnyOnParameter::*;
    match parameter {
        TrackSolo => T::TrackSolo,
        TrackMute => T::TrackMute,
        TrackArm => T::TrackArm,
        TrackSelection => T::TrackSelection,
    }
}

fn convert_automation_mode(mode: RealearnAutomationMode) -> persistence::AutomationMode {
    use persistence::AutomationMode as T;
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

fn convert_track_exclusivity(
    exclusivity: TrackExclusivity,
) -> Option<persistence::TrackExclusivity> {
    use persistence::TrackExclusivity as T;
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
) -> Option<persistence::FxDisplayKind> {
    use persistence::FxDisplayKind as T;
    use FxDisplayType::*;
    let v = match display_type {
        FloatingWindow => T::FloatingWindow,
        Chain => T::Chain,
    };
    style.required_value(v)
}

fn convert_virtual_target(data: TargetModelData, style: ConversionStyle) -> persistence::Target {
    persistence::Target::Virtual(persistence::VirtualTarget {
        id: convert_control_element_id(data.control_element_index),
        character: style.required_value(data.control_element_type),
        learnable: style.required_value_with_default(data.learnable, defaults::TARGET_LEARNABLE),
    })
}

fn convert_track_descriptor(
    data: TrackData,
    only_if_track_selected: bool,
    clip_column: &helgobox_api::persistence::PlaytimeColumnDescriptor,
    style: ConversionStyle,
) -> Option<persistence::TrackDescriptor> {
    let input = TrackDeserializationInput {
        track_data: &data,
        clip_column,
    };
    let props = deserialize_track(input);
    use persistence::TrackDescriptor as T;
    use VirtualTrackType::*;
    let commons = persistence::TrackDescriptorCommons {
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
        Dynamic | DynamicTcp | DynamicMcp => T::Dynamic {
            commons,
            expression: props.expression,
            scope: style.optional_value(props.r#type.virtual_track_scope()),
        },
        Master => T::Master { commons },
        Unit => T::Instance { commons },
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
        ByIndex | ByIndexTcp | ByIndexMcp => T::ByIndex {
            commons,
            index: props.index,
            scope: style.optional_value(props.r#type.virtual_track_scope()),
        },
        FromClipColumn => T::FromClipColumn {
            commons,
            column: props.clip_column,
            context: props.clip_column_track_context,
        },
    };
    style.required_value(desc)
}

fn convert_fx_chain_descriptor(
    data: TargetModelData,
    style: ConversionStyle,
) -> persistence::FxChainDescriptor {
    persistence::FxChainDescriptor::Track {
        track: convert_track_descriptor(
            data.track_data,
            data.enable_only_if_track_is_selected,
            &data.clip_column,
            style,
        ),
        chain: convert_fx_chain(data.fx_data.is_input_fx, style),
    }
}

fn convert_fx_chain(
    is_input_fx: bool,
    style: ConversionStyle,
) -> Option<persistence::TrackFxChain> {
    let chain = if is_input_fx {
        persistence::TrackFxChain::Input
    } else {
        persistence::TrackFxChain::Normal
    };
    style.required_value(chain)
}

fn convert_fx_parameter_descriptor(
    data: TargetModelData,
    style: ConversionStyle,
) -> persistence::FxParameterDescriptor {
    let props = deserialize_fx_parameter(&data.fx_parameter_data);
    use persistence::FxParameterDescriptor as T;
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
) -> persistence::RouteDescriptor {
    let props = deserialize_track_route(&data.track_route_data);
    use persistence::RouteDescriptor as T;
    use TrackRouteSelectorType::*;
    let commons = persistence::RouteDescriptorCommons {
        track: convert_track_descriptor(
            data.track_data,
            data.enable_only_if_track_is_selected,
            &data.clip_column,
            style,
        ),
        route_kind: {
            use persistence::TrackRouteKind as T;
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
) -> Option<persistence::FxDescriptor> {
    let props = deserialize_fx(&data.fx_data, None, &MigrationDescriptor::default());
    use persistence::FxDescriptor as T;
    use VirtualFxType::*;
    let commons = persistence::FxDescriptorCommons {
        fx_must_have_focus: style.required_value_with_default(
            data.enable_only_if_fx_has_focus,
            defaults::TARGET_FX_MUST_HAVE_FOCUS,
        ),
    };
    let v = match props.r#type {
        This => T::This { commons },
        Focused => T::Focused,
        Unit => T::Instance { commons },
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
) -> Option<persistence::FeedbackResolution> {
    use persistence::FeedbackResolution as T;
    use FeedbackResolution::*;
    let v = match r {
        Beat => T::Beat,
        High => T::High,
    };
    style.required_value(v)
}
