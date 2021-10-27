use crate::infrastructure::api::schema::{
    OscArgument, VirtualControlElementId, VirtualControlElementKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum Target {
    LastTouched(LastTouchedTarget),
    AutomationModeOverride(AutomationModeOverrideTarget),
    ReaperAction(ReaperActionTarget),
    TransportAction(TransportActionTarget),
    CycleThroughTracks(CycleThroughTracksTarget),
    Seek(SeekTarget),
    PlayRate(PlayRateTarget),
    Tempo(TempoTarget),
    GoToBookmark(GoToBookmarkTarget),
    TrackArmState(TrackArmStateTarget),
    AllTrackFxOnOffState(AllTrackFxOnOffStateTarget),
    TrackMuteState(TrackMuteStateTarget),
    TrackPeak(TrackPeakTarget),
    TrackPhase(TrackPhaseTarget),
    TrackSelectionState(TrackSelectionStateTarget),
    TrackAutomationMode(TrackAutomationModeTarget),
    TrackAutomationTouchState(TrackAutomationTouchStateTarget),
    TrackPan(TrackPanTarget),
    TrackWidth(TrackWidthTarget),
    TrackVolume(TrackVolumeTarget),
    TrackVisibility(TrackVisibilityTarget),
    TrackSoloState(TrackSoloStateTarget),
    CycleThroughFx(CycleThroughFxTarget),
    FxOnOffState(FxOnOffStateTarget),
    LoadFxSnapshot(LoadFxSnapshotTarget),
    CycleThroughFxPresets(CycleThroughFxPresetsTarget),
    FxVisibility(FxVisibilityTarget),
    FxParameterValue(FxParameterValueTarget),
    RouteAutomationMode(RouteAutomationModeTarget),
    RouteMonoState(RouteMonoStateTarget),
    RouteMuteState(RouteMuteStateTarget),
    RoutePhase(RoutePhaseTarget),
    RoutePan(RoutePanTarget),
    RouteVolume(RouteVolumeTarget),
    ClipTransportAction(ClipTransportActionTarget),
    ClipSeek(ClipSeekTarget),
    ClipVolume(ClipVolumeTarget),
    SendMidi(SendMidiTarget),
    SendOsc(SendOscTarget),
    EnableInstances(EnableInstancesTarget),
    EnableMappings(EnableMappingsTarget),
    LoadMappingSnapshots(LoadMappingSnapshotsTarget),
    CycleThroughGroupMappings(CycleThroughGroupMappingsTarget),
    Virtual(VirtualTarget),
}

impl Default for Target {
    fn default() -> Self {
        Self::LastTouched(Default::default())
    }
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TargetCommons {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<TargetUnit>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
pub enum TargetUnit {
    Native,
    Percent,
}

impl Default for TargetUnit {
    fn default() -> Self {
        Self::Native
    }
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct LastTouchedTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AutomationModeOverrideTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#override: Option<AutomationModeOverride>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct ReaperActionTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<ReaperCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation: Option<ActionInvocationKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TransportActionTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub action: TransportAction,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CycleThroughTracksTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_arrange_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_mixer: Option<bool>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SeekTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_time_selection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_loop_points: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_regions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_project: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek_play: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_resolution: Option<FeedbackResolution>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct PlayRateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TempoTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct GoToBookmarkTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub bookmark: BookmarkDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_time_selection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_loop_points: Option<bool>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackArmStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct AllTrackFxOnOffStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackMuteStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackPeakTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackPhaseTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackSelectionStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_arrange_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_mixer: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackAutomationModeTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    pub mode: AutomationMode,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackAutomationTouchStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    pub touched_parameter: TouchedParameter,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackPanTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackWidthTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackVolumeTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackVisibilityTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
    pub area: TrackArea,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackSoloStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<TrackExclusivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<SoloBehavior>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CycleThroughFxTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub chain: FxChainDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_kind: Option<FxDisplayKind>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FxOnOffStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx: Option<FxDescriptor>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct LoadFxSnapshotTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx: Option<FxDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<FxSnapshot>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CycleThroughFxPresetsTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx: Option<FxDescriptor>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FxVisibilityTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx: Option<FxDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_kind: Option<FxDisplayKind>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FxParameterValueTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub parameter: FxParameterDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RouteAutomationModeTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub route: RouteDescriptor,
    pub mode: AutomationMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RouteMonoStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub route: RouteDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RouteMuteStateTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub route: RouteDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RoutePhaseTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub route: RouteDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_for_feedback: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RoutePanTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub route: RouteDescriptor,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RouteVolumeTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub route: RouteDescriptor,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct ClipTransportActionTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<ClipOutput>,
    pub clip: ClipDescriptor,
    pub action: TransportAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_bar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffered: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct ClipSeekTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub clip: ClipDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_resolution: Option<FeedbackResolution>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct ClipVolumeTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    pub clip: ClipDescriptor,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SendMidiTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<MidiDestination>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct SendOscTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument: Option<OscArgument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<OscDestination>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct EnableInstancesTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<InstanceExclusivity>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct EnableMappingsTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<MappingExclusivity>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct LoadMappingSnapshotsTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_mappings_only: Option<bool>,
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct CycleThroughGroupMappingsTarget {
    #[serde(flatten)]
    pub commons: TargetCommons,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusivity: Option<GroupMappingExclusivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct VirtualTarget {
    pub id: VirtualControlElementId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<VirtualControlElementKind>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum AutomationModeOverride {
    Bypass,
    Mode { mode: AutomationMode },
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum AutomationMode {
    TrimRead,
    Read,
    Touch,
    Write,
    Latch,
    LatchPreview,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum TransportAction {
    PlayStop,
    PlayPause,
    Stop,
    Pause,
    Record,
    Repeat,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum ActionInvocationKind {
    Trigger,
    Absolute,
    Relative,
}

impl Default for ActionInvocationKind {
    fn default() -> Self {
        Self::Absolute
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum ReaperCommand {
    Id(u32),
    Name(String),
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "address")]
pub enum TrackDescriptor {
    This {
        #[serde(flatten)]
        commons: TrackDescriptorCommons,
    },
    Master {
        #[serde(flatten)]
        commons: TrackDescriptorCommons,
    },
    Selected {
        #[serde(skip_serializing_if = "Option::is_none")]
        allow_multiple: Option<bool>,
    },
    Dynamic {
        #[serde(flatten)]
        commons: TrackDescriptorCommons,
        expression: String,
    },
    ById {
        #[serde(flatten)]
        commons: TrackDescriptorCommons,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    ByIndex {
        #[serde(flatten)]
        commons: TrackDescriptorCommons,
        index: u32,
    },
    ByName {
        #[serde(flatten)]
        commons: TrackDescriptorCommons,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        allow_multiple: Option<bool>,
    },
}

impl Default for TrackDescriptor {
    fn default() -> Self {
        Self::This {
            commons: Default::default(),
        }
    }
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackDescriptorCommons {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_must_be_selected: Option<bool>,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum FeedbackResolution {
    Beat,
    High,
}

impl Default for FeedbackResolution {
    fn default() -> Self {
        Self::Beat
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum TrackExclusivity {
    WithinProject,
    WithinFolder,
    WithinProjectOnOnly,
    WithinFolderOnOnly,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum InstanceExclusivity {
    Exclusive,
    ExclusiveOnOnly,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum MappingExclusivity {
    Exclusive,
    ExclusiveOnOnly,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum GroupMappingExclusivity {
    Exclusive,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum TouchedParameter {
    Volume,
    Pan,
    Width,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum TrackArea {
    Tcp,
    Mcp,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum SoloBehavior {
    InPlace,
    IgnoreRouting,
    ReaperPreference,
}

impl Default for SoloBehavior {
    fn default() -> Self {
        SoloBehavior::InPlace
    }
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum BookmarkDescriptor {
    Marker(BookmarkRef),
    Region(BookmarkRef),
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum BookmarkRef {
    ById { id: u32 },
    ByIndex { index: u32 },
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FxDescriptorCommons {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_must_have_focus: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "address")]
pub enum FxDescriptor {
    This {
        #[serde(flatten)]
        commons: FxDescriptorCommons,
    },
    Focused,
    Dynamic {
        #[serde(flatten)]
        commons: FxDescriptorCommons,
        chain: FxChainDescriptor,
        expression: String,
    },
    ById {
        #[serde(flatten)]
        commons: FxDescriptorCommons,
        chain: FxChainDescriptor,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    ByIndex {
        #[serde(flatten)]
        commons: FxDescriptorCommons,
        chain: FxChainDescriptor,
        index: u32,
    },
    ByName {
        #[serde(flatten)]
        commons: FxDescriptorCommons,
        chain: FxChainDescriptor,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        allow_multiple: Option<bool>,
    },
}

impl Default for FxDescriptor {
    fn default() -> Self {
        Self::This {
            commons: Default::default(),
        }
    }
}

// The best default for this would be a <This> FX chain but we don't have this yet!
// Therefore we don't implement Default at all for now. We can still do it later.
#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "address")]
pub enum FxChainDescriptor {
    Track {
        #[serde(skip_serializing_if = "Option::is_none")]
        track: Option<TrackDescriptor>,
        #[serde(skip_serializing_if = "Option::is_none")]
        chain: Option<TrackFxChain>,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub enum TrackFxChain {
    Normal,
    Input,
}

impl Default for TrackFxChain {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum FxDisplayKind {
    FloatingWindow,
    Chain,
}

impl Default for FxDisplayKind {
    fn default() -> Self {
        Self::FloatingWindow
    }
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FxSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_name: Option<String>,
    pub content: FxSnapshotContent,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum FxSnapshotContent {
    Chunk { chunk: String },
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "address")]
pub enum FxParameterDescriptor {
    Dynamic {
        #[serde(skip_serializing_if = "Option::is_none")]
        fx: Option<FxDescriptor>,
        expression: String,
    },
    ById {
        #[serde(skip_serializing_if = "Option::is_none")]
        fx: Option<FxDescriptor>,
        index: u32,
    },
    ByIndex {
        #[serde(skip_serializing_if = "Option::is_none")]
        fx: Option<FxDescriptor>,
        index: u32,
    },
    ByName {
        #[serde(skip_serializing_if = "Option::is_none")]
        fx: Option<FxDescriptor>,
        name: String,
    },
}

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RouteDescriptorCommons {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<TrackRouteKind>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "address")]
pub enum RouteDescriptor {
    Dynamic {
        #[serde(flatten)]
        commons: RouteDescriptorCommons,
        expression: String,
    },
    ById {
        #[serde(flatten)]
        commons: RouteDescriptorCommons,
        id: Option<String>,
    },
    ByIndex {
        #[serde(flatten)]
        commons: RouteDescriptorCommons,
        index: u32,
    },
    ByName {
        #[serde(flatten)]
        commons: RouteDescriptorCommons,
        name: String,
    },
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum TrackRouteKind {
    Send,
    Receive,
    HardwareOutput,
}

impl Default for TrackRouteKind {
    fn default() -> Self {
        Self::Send
    }
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "address")]
pub enum ClipDescriptor {
    Slot { index: u32 },
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum ClipOutput {
    Track {
        #[serde(skip_serializing_if = "Option::is_none")]
        track: Option<TrackDescriptor>,
    },
}

impl Default for ClipOutput {
    fn default() -> Self {
        Self::Track { track: None }
    }
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum MidiDestination {
    FxOutput,
    FeedbackOutput,
}

impl Default for MidiDestination {
    fn default() -> Self {
        Self::FeedbackOutput
    }
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum OscDestination {
    FeedbackOutput,
    Device { id: String },
}

impl Default for OscDestination {
    fn default() -> Self {
        Self::FeedbackOutput
    }
}
