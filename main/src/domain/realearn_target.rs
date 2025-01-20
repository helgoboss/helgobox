use crate::application::SharedUnitModel;
use crate::domain::ui_util::{
    format_as_percentage_without_unit, log_output, parse_unit_value_from_percentage, OutputReason,
};
use crate::domain::{
    format_as_pretty_hex, new_set_track_ui_functions_are_available, scoped_track_index,
    AdditionalFeedbackEvent, AdditionalTransformationInput, BasicSettings, CompartmentKind,
    DomainEventHandler, Exclusivity, ExtendedProcessorContext, FeedbackAudioHookTask,
    FeedbackOutput, FeedbackRealTimeTask, GroupId, InstanceStateChanged, MainMapping,
    MappingControlResult, MappingId, OrderedMappingMap, OscFeedbackTask, PluginParamIndex,
    ProcessorContext, QualifiedMappingId, RealTimeReaperTarget, RealearnModeContext,
    RealearnSourceContext, ReaperTarget, SharedInstance, SharedUnit, StreamDeckDeviceId, Tag,
    TagScope, TargetCharacter, TrackExclusivity, UnitEvent, UnitId, WeakRealTimeInstance,
    ACTION_TARGET, ALL_TRACK_FX_ENABLE_TARGET, ANY_ON_TARGET, AUTOMATION_MODE_OVERRIDE_TARGET,
    BROWSE_FXS_TARGET, BROWSE_GROUP_MAPPINGS_TARGET, BROWSE_POT_FILTER_ITEMS_TARGET,
    BROWSE_POT_PRESETS_TARGET, COMPARTMENT_PARAMETER_VALUE_TARGET, DUMMY_TARGET,
    ENABLE_INSTANCES_TARGET, ENABLE_MAPPINGS_TARGET, FX_ENABLE_TARGET, FX_ONLINE_TARGET,
    FX_OPEN_TARGET, FX_PARAMETER_TARGET, FX_PARAMETER_TOUCH_STATE_TARGET, FX_PRESET_TARGET,
    FX_TOOL_TARGET, GO_TO_BOOKMARK_TARGET, LAST_TOUCHED_TARGET, LEARN_MAPPING_TARGET,
    LOAD_FX_SNAPSHOT_TARGET, LOAD_MAPPING_SNAPSHOT_TARGET, LOAD_POT_PRESET_TARGET,
    MIDI_SEND_TARGET, MOUSE_TARGET, OSC_SEND_TARGET, PLAYRATE_TARGET, PREVIEW_POT_PRESET_TARGET,
    ROUTE_AUTOMATION_MODE_TARGET, ROUTE_MONO_TARGET, ROUTE_MUTE_TARGET, ROUTE_PAN_TARGET,
    ROUTE_PHASE_TARGET, ROUTE_TOUCH_STATE_TARGET, ROUTE_VOLUME_TARGET,
    SAVE_MAPPING_SNAPSHOT_TARGET, SEEK_TARGET, SELECTED_TRACK_TARGET,
    STREAM_DECK_BRIGHTNESS_TARGET, TEMPO_TARGET, TRACK_ARM_TARGET, TRACK_AUTOMATION_MODE_TARGET,
    TRACK_MONITORING_MODE_TARGET, TRACK_MUTE_TARGET, TRACK_PAN_TARGET, TRACK_PARENT_SEND_TARGET,
    TRACK_PEAK_TARGET, TRACK_PHASE_TARGET, TRACK_SELECTION_TARGET, TRACK_SHOW_TARGET,
    TRACK_SOLO_TARGET, TRACK_TOOL_TARGET, TRACK_TOUCH_STATE_TARGET, TRACK_VOLUME_TARGET,
    TRACK_WIDTH_TARGET, TRANSPORT_TARGET,
};
use base::hash_util::NonCryptoHashSet;
use base::{SenderToNormalThread, SenderToRealTimeThread};
use enum_dispatch::enum_dispatch;
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, NumericValue, PropValue, RawMidiEvent, RgbColor,
    TransformationInputProvider, UnitValue,
};
use helgobox_api::persistence::{LearnableTargetKind, TrackScope};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{ChangeEvent, Fx, Guid, Project, Reaper, Track, TrackRoute};
use reaper_medium::CommandId;
use serde_repr::*;
use std::borrow::Cow;
use std::fmt::{Debug, Display, Formatter};
use strum::IntoEnumIterator;

#[enum_dispatch(ReaperTarget)]
pub trait RealearnTarget {
    // TODO-low Instead of taking the ControlContext as parameter in each method, we could also
    //  choose to implement RealearnTarget for a wrapper that contains the control context.
    //  We did this with ValueFormatter and ValueParser.
    fn character(&self, context: ControlContext) -> TargetCharacter {
        self.control_type_and_character(context).1
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        None
    }

    fn control_type_and_character(
        &self,
        _context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn open(&self, context: ControlContext) {
        let _ = context;
        if let Some(fx) = self.fx() {
            let _ = fx.show_in_floating_window();
            return;
        }
        if let Some(track) = self.track() {
            track.select_exclusively();
            // Scroll to track
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()), None)
                .expect("built-in action should exist");
        }
    }

    /// Parses the given text as a target value and returns it as unit value.
    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = context;
        parse_unit_value_from_percentage(text)
    }

    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = context;
        parse_unit_value_from_percentage(text)
    }

    /// This converts the given normalized value to a discrete value.
    ///
    /// Used for displaying discrete target values in edit fields.
    /// Must be implemented for discrete targets only which don't support parsing according to
    /// `can_parse_values()`, e.g. FX preset. This target reports a step size. If we want to
    /// display an increment or a particular value in an edit field, we don't show normalized
    /// values of course but a discrete number, by using this function. Should be the reverse of
    /// `convert_discrete_value_to_unit_value()` because latter is used for parsing.
    ///
    /// # Errors
    ///
    /// Returns an error if this target doesn't report a step size.
    fn convert_unit_value_to_discrete_value(
        &self,
        value: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        let _ = value;
        let _ = context;
        Err("not supported")
    }

    /// Formats the given value without unit.
    ///
    /// Shown in the mapping panel text field for continuous targets.
    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        self.format_as_discrete_or_percentage(value, context)
    }

    /// Shown as default textual feedback.
    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        let _ = context;
        None
    }

    /// Usable in textual feedback expressions as
    /// [`helgoboss_learn::target_prop_keys::NUMERIC_VALUE`]. Don't implement for on/off values!
    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let _ = context;
        None
    }

    /// Formats the given step size without unit.
    fn format_step_size_without_unit(
        &self,
        step_size: UnitValue,
        context: ControlContext,
    ) -> String {
        self.format_as_discrete_or_percentage(step_size, context)
    }

    /// Reusable function
    // TODO-medium Never overwritten. Can be factored out!
    fn format_as_discrete_or_percentage(
        &self,
        value: UnitValue,
        context: ControlContext,
    ) -> String {
        match self.character(context) {
            TargetCharacter::Discrete => self
                .convert_unit_value_to_discrete_value(value, context)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            TargetCharacter::Trigger => String::new(),
            _ => format_as_percentage_without_unit(value),
        }
    }

    /// If this returns true, a value will not be printed (e.g. because it's already in the edit
    /// field).
    fn hide_formatted_value(&self, context: ControlContext) -> bool {
        let _ = context;
        false
    }

    /// If this returns true, a step size will not be printed (e.g. because it's already in the
    /// edit field).
    fn hide_formatted_step_size(&self, context: ControlContext) -> bool {
        let _ = context;
        false
    }

    /// For mapping panel.
    fn value_unit(&self, context: ControlContext) -> &'static str {
        self.value_unit_default(context)
    }

    fn value_unit_default(&self, context: ControlContext) -> &'static str {
        match self.character(context) {
            TargetCharacter::Trigger | TargetCharacter::Discrete => "",
            _ => "%",
        }
    }

    /// For textual feedback.
    fn numeric_value_unit(&self, context: ControlContext) -> &'static str {
        self.value_unit(context)
    }

    fn step_size_unit(&self, context: ControlContext) -> &'static str {
        if self.character(context) == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }
    /// Formats the value completely (including a possible unit).
    fn format_value(&self, value: UnitValue, context: ControlContext) -> String {
        self.format_value_generic(value, context)
    }

    // TODO-medium Never overwritten. Can be factored out!
    fn format_value_generic(&self, value: UnitValue, context: ControlContext) -> String {
        format!(
            "{} {}",
            self.format_value_without_unit(value, context),
            self.value_unit(context)
        )
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let (_, _) = (value, context);
        Err("not supported")
    }

    fn can_report_current_value(&self) -> bool {
        // We will quickly realize if not.
        true
    }

    /// Used for target "Global: Last touched" and queryable as target prop.
    fn is_available(&self, _context: ControlContext) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        None
    }
    fn track(&self) -> Option<&Track> {
        None
    }
    fn fx(&self) -> Option<&Fx> {
        None
    }
    fn route(&self) -> Option<&TrackRoute> {
        None
    }
    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        None
    }
    fn clip_slot_address(&self) -> Option<playtime_api::persistence::SlotAddress> {
        None
    }
    fn clip_column_address(&self) -> Option<playtime_api::persistence::ColumnAddress> {
        None
    }
    fn clip_row_address(&self) -> Option<playtime_api::persistence::RowAddress> {
        None
    }

    /// Whether the target supports automatic feedback in response to some events or polling.
    ///
    /// If the target doesn't support automatic feedback, you are left with a choice:
    ///
    /// - a) Use polling (continuously poll the target value).
    /// - b) Set this to `false` (will send feedback after control only).
    ///
    /// Choose (a) if the target value is a real, global target value that also can affect
    /// other mappings. Polling is obviously not the optimal choice because of the performance
    /// drawback ... but at least multiple mappings can participate.
    ///
    /// Choose (b) if the target value is not global but artificial, that is, attached to the
    /// mapping itself - and can therefore not have any effect on other mappings. This is also
    /// not the optimal choice because other mappings can't participate in the feedback value ...
    /// but at least it's fast.
    fn supports_automatic_feedback(&self) -> bool {
        // Usually yes. We will quickly realize if not.
        true
    }

    /// Might return the new value if changed but is not required to! If it doesn't and the consumer
    /// wants to know the new value, it should just query the current value of the target.
    ///
    /// Is called in any case (even if feedback not enabled). So we can use it for general-purpose
    /// change event reactions such as reacting to transport stop.
    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        let (_, _) = (evt, context);
        (false, None)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        None
    }

    /// Like `convert_unit_value_to_discrete_value()` but in the other direction.
    ///
    /// Used for parsing discrete values of discrete targets that can't do real parsing according to
    /// `can_parse_values()`.
    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = value;
        let _ = context;
        Err("not supported")
    }

    fn parse_value_from_discrete_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(
            text.parse().map_err(|_| "not a discrete value")?,
            context,
        )
    }

    /// Returns a value for the given key if the target supports it.
    ///
    /// You don't need to implement the commonly supported prop values here! They will
    /// be handed out automatically as a fallback by the calling method in case you return `None`.
    //
    // Requiring owned strings here makes the API more pleasant and is probably not a big deal
    // performance-wise (feedback strings don't get large). If we want to optimize this in future,
    // don't use Cows. The only real performance win would be to use a writer API.
    // With Cows, we would still need to turn ReaperStr into owned String. With writer API,
    // we could just read borrowed ReaperStr as str and write into the result buffer. However, in
    // practice we don't often get borrowed strings from Reaper anyway.
    // TODO-low Use a formatter API instead of returning owned strings (for this to work, we also
    //  need to adjust the textual feedback expression parsing to take advantage of it).
    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        let _ = key;
        let _ = context;
        None
    }
}

#[derive(Copy, Clone, Debug)]
pub enum CompoundChangeEvent<'a> {
    Reaper(&'a ChangeEvent),
    Additional(&'a AdditionalFeedbackEvent),
    Instance(&'a InstanceStateChanged),
    Unit(&'a UnitEvent),
    CompartmentParameter(PluginParamIndex),
    #[cfg(feature = "playtime")]
    ClipMatrix(&'a playtime_clip_engine::base::ClipMatrixEvent),
}

pub fn get_track_name(t: &Track, scope: TrackScope) -> String {
    if let Some(n) = t.name() {
        if n.to_str().is_empty() {
            let track_index = scoped_track_index(t, scope);
            format!("Track {}", track_index.unwrap_or(0) + 1)
        } else {
            n.into_string()
        }
    } else {
        "<Master>".to_string()
    }
}

pub fn convert_reaper_color_to_helgoboss_learn(color: reaper_medium::RgbColor) -> RgbColor {
    let reaper_medium::RgbColor { r, g, b } = color;
    RgbColor::new(r, g, b)
}

pub trait UnitContainer: Debug {
    fn find_session_by_id(&self, session_id: &str) -> Option<SharedUnitModel>;
    fn find_session_by_instance_id(&self, instance_id: UnitId) -> Option<SharedUnitModel>;
    /// Returns activated tags if they don't correspond to the tags in the args.
    fn enable_instances(&self, args: EnableInstancesArgs) -> Option<NonCryptoHashSet<Tag>>;
    fn change_instance_fx(&self, args: ChangeInstanceFxArgs) -> Result<(), &'static str>;
    fn change_instance_track(&self, args: ChangeInstanceTrackArgs) -> Result<(), &'static str>;
}

pub struct EnableInstancesArgs<'a> {
    pub common: InstanceContainerCommonArgs<'a>,
    pub is_enable: bool,
    pub exclusivity: Exclusivity,
}

pub struct ChangeInstanceFxArgs<'a> {
    pub common: InstanceContainerCommonArgs<'a>,
    pub request: InstanceFxChangeRequest,
}

pub struct ChangeInstanceTrackArgs<'a> {
    pub common: InstanceContainerCommonArgs<'a>,
    pub request: InstanceTrackChangeRequest,
}

#[derive(Copy, Clone, Debug)]
pub enum InstanceFxChangeRequest {
    Pin {
        /// `None` means master track.
        track_guid: Option<Guid>,
        is_input_fx: bool,
        fx_guid: Guid,
    },
    SetFromMapping(QualifiedMappingId),
}

#[derive(Copy, Clone, Debug)]
pub enum InstanceTrackChangeRequest {
    /// `None` means master track.
    Pin(Option<Guid>),
    SetFromMapping(QualifiedMappingId),
}

pub struct InstanceContainerCommonArgs<'a> {
    pub initiator_instance_id: UnitId,
    /// `None` if monitoring FX.
    pub initiator_project: Option<Project>,
    pub scope: &'a TagScope,
}

#[derive(Copy, Clone, Debug)]
pub struct ControlContext<'a> {
    pub feedback_audio_hook_task_sender: &'a SenderToRealTimeThread<FeedbackAudioHookTask>,
    pub feedback_real_time_task_sender: &'a SenderToRealTimeThread<FeedbackRealTimeTask>,
    pub osc_feedback_task_sender: &'a SenderToNormalThread<OscFeedbackTask>,
    pub feedback_output: Option<FeedbackOutput>,
    pub stream_deck_dev_id: Option<StreamDeckDeviceId>,
    pub unit_container: &'a dyn UnitContainer,
    pub instance: &'a SharedInstance,
    pub unit: &'a SharedUnit,
    pub unit_id: UnitId,
    pub output_logging_enabled: bool,
    pub source_context: RealearnSourceContext<'a>,
    pub mode_context: RealearnModeContext<'a>,
    pub processor_context: &'a ProcessorContext,
}

#[derive(Copy, Clone, Debug)]
pub struct RealTimeControlContext<'a> {
    pub instance: &'a WeakRealTimeInstance,
    pub _p: &'a (),
}

impl<'a> RealTimeControlContext<'a> {
    #[cfg(feature = "playtime")]
    pub fn clip_matrix(&self) -> Result<playtime_clip_engine::rt::SharedRtMatrix, &'static str> {
        let instance = self.instance.upgrade().ok_or("real-time instance gone")?;
        let instance = base::non_blocking_lock(&*instance, "real-time instance");
        instance
            .clip_matrix()
            .ok_or("real-time Playtime matrix not yet initialized")?
            .upgrade()
            .ok_or("real-time Playtime matrix doesn't exist anymore")
    }
}

impl<'a> TransformationInputProvider<AdditionalTransformationInput> for RealTimeControlContext<'a> {
    fn additional_input(&self) -> AdditionalTransformationInput {
        AdditionalTransformationInput::default()
    }
}

impl<'a> ControlContext<'a> {
    pub fn instance(&self) -> &SharedInstance {
        self.instance
    }

    pub fn log_outgoing_target_midi(&self, events: &[RawMidiEvent]) {
        if self.output_logging_enabled {
            for e in events {
                log_output(
                    self.unit_id,
                    OutputReason::TargetOutput,
                    format_as_pretty_hex(e.bytes()),
                );
            }
        }
    }

    pub fn instance_container_common_args<'c>(
        &self,
        scope: &'c TagScope,
    ) -> InstanceContainerCommonArgs<'c> {
        InstanceContainerCommonArgs {
            initiator_instance_id: self.unit_id,
            initiator_project: self.processor_context.project(),
            scope,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingControlContext<'a> {
    pub control_context: ControlContext<'a>,
    pub mapping_data: MappingData,
    /// This means control was initiated in the real-time processor (currently MIDI only).
    ///
    /// This information is used by some particular targets whose work is partially done in real-time and partially
    /// in the main thread.
    pub coming_from_real_time: bool,
}

impl<'a> TransformationInputProvider<AdditionalTransformationInput> for MappingControlContext<'a> {
    fn additional_input(&self) -> AdditionalTransformationInput {
        AdditionalTransformationInput {
            y_last: self
                .mapping_data
                .last_non_performance_target_value
                .map(|v| v.to_unit_value().get())
                .unwrap_or_default(),
        }
    }
}

impl<'a> From<MappingControlContext<'a>> for ControlContext<'a> {
    fn from(v: MappingControlContext<'a>) -> Self {
        v.control_context
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingData {
    pub compartment: CompartmentKind,
    pub mapping_id: MappingId,
    pub group_id: GroupId,
    pub last_non_performance_target_value: Option<AbsoluteValue>,
}

impl MappingData {
    pub fn qualified_mapping_id(&self) -> QualifiedMappingId {
        QualifiedMappingId::new(self.compartment, self.mapping_id)
    }
}

pub type BoxedHitInstruction = Box<dyn HitInstruction>;

pub struct HitResponse {
    /// If the target invocation caused an effect.
    ///
    /// Typical example: `false` if the button was released and the target only does something
    /// when the button is pressed.
    ///
    /// At the moment, we also return `true` if the target invocation was done but the value stays
    /// the same. That's okay.
    ///
    /// This can be `true` even if a hit instruction is given ... maybe part of the invocation can
    /// be done before the hit instruction and has an effect already.
    pub caused_effect: bool,
    /// A hit instruction to be executed later.
    pub hit_instruction: Option<BoxedHitInstruction>,
}

impl HitResponse {
    pub fn ignored() -> Self {
        Self {
            caused_effect: false,
            hit_instruction: None,
        }
    }

    pub fn processed_with_effect() -> Self {
        Self {
            caused_effect: true,
            hit_instruction: None,
        }
    }

    /// Creates a response that says "no effect so far, but please execute the given hit
    /// instruction".
    pub fn hit_instruction(hit_instruction: Box<dyn HitInstruction>) -> Self {
        Self {
            caused_effect: false,
            hit_instruction: Some(hit_instruction),
        }
    }
}

pub enum HitInstructionResponse {
    /// Hit instruction ignored.
    Ignored,
    /// Hit instruction caused effect.
    ///
    /// Caller should inspect the given "inner" mapping control results and possibly execute
    /// the hit instructions returned from them in a second pass.
    CausedEffect(Vec<MappingControlResult>),
}

pub trait HitInstruction {
    fn execute(self: Box<Self>, context: HitInstructionContext) -> HitInstructionResponse;
}

pub struct HitInstructionContext<'a> {
    /// All mappings in the relevant compartment.
    pub mappings: &'a mut OrderedMappingMap<MainMapping>,
    // TODO-medium This became part of ExtendedProcessorContext, so redundant (not just here BTW)
    pub control_context: ControlContext<'a>,
    pub domain_event_handler: &'a dyn DomainEventHandler,
    pub processor_context: ExtendedProcessorContext<'a>,
    pub basic_settings: &'a BasicSettings,
}

/// Type of a target
///
/// Display implementation produces single-line medium-length names that are supposed to be shown
/// e.g. in the dropdown.
///
/// IMPORTANT: Don't change the numbers! They are serialized.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize_repr,
    Deserialize_repr,
    strum::EnumIter,
    strum::EnumCount,
    TryFromPrimitive,
    IntoPrimitive,
)]
#[repr(usize)]
pub enum ReaperTargetType {
    // Global targets
    LastTouched = 20,
    Mouse = 57,
    AutomationModeOverride = 26,

    // Project targets
    AnyOn = 43,
    BrowseTracks = 14,
    Action = 0,
    Transport = 16,
    Seek = 23,
    PlayRate = 11,
    Tempo = 10,

    // Marker/region targets
    GoToBookmark = 22,

    // Track targets
    TrackTool = 44,
    TrackArm = 5,
    AllTrackFxEnable = 15,
    TrackParentSend = 56,
    TrackMute = 7,
    TrackPeak = 34,
    TrackPhase = 39,
    TrackSelection = 6,
    TrackAutomationMode = 25,
    TrackTouchState = 21,
    TrackMonitoringMode = 49,
    TrackPan = 4,
    TrackWidth = 17,
    TrackVolume = 2,
    TrackShow = 24,
    TrackSolo = 8,

    // FX chain targets
    BrowseFxs = 28,

    // FX targets
    FxTool = 54,
    FxPreset = 13,
    FxEnable = 12,
    FxOnline = 42,
    LoadFxSnapshot = 19,
    FxOpen = 27,

    // FX parameter targets
    FxParameterTouchState = 47,
    #[default]
    FxParameterValue = 1,

    // Pot targets
    BrowsePotFilterItems = 61,
    BrowsePotPresets = 58,
    PreviewPotPreset = 59,
    LoadPotPreset = 60,

    // Send targets
    RouteTouchState = 48,
    RouteMono = 41,
    RouteMute = 18,
    RoutePhase = 40,
    RouteAutomationMode = 45,
    RoutePan = 9,
    RouteVolume = 3,

    // Clip targets
    PlaytimeSlotManagementAction = 46,
    PlaytimeSlotTransportAction = 31,
    PlaytimeSlotSeek = 32,
    PlaytimeSlotVolume = 33,

    // Clip column targets
    PlaytimeColumnAction = 50,

    // Clip row targets
    PlaytimeRowAction = 52,

    // Playtime matrix
    PlaytimeMatrixAction = 51,
    PlaytimeControlUnitScroll = 64,
    PlaytimeBrowseCells = 65,

    // Misc
    SendMidi = 29,
    SendOsc = 30,
    StreamDeckBrightness = 66,

    // ReaLearn targets
    Dummy = 53,
    EnableInstances = 38,
    EnableMappings = 36,
    ModifyMapping = 62,
    CompartmentParameterValue = 63,
    LoadMappingSnapshot = 35,
    TakeMappingSnapshot = 55,
    BrowseGroup = 37,
}

impl Display for ReaperTargetType {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let def = self.definition();
        write!(f, "{}: {}", def.section, def.name)
    }
}

impl ReaperTargetType {
    pub fn from_target(target: &ReaperTarget) -> ReaperTargetType {
        target
            .reaper_target_type()
            .expect("a REAPER target should always return a REAPER target type")
    }

    pub fn from_learnable_target_kind(kind: LearnableTargetKind) -> ReaperTargetType {
        use LearnableTargetKind as A;
        use ReaperTargetType as B;
        match kind {
            A::TrackVolume => B::TrackVolume,
            A::TrackPan => B::TrackPan,
            A::RouteVolume => B::RouteVolume,
            A::RoutePan => B::RoutePan,
            A::TrackArmState => B::TrackArm,
            A::TrackMuteState => B::TrackMute,
            A::TrackSoloState => B::TrackSolo,
            A::TrackSelectionState => B::TrackSelection,
            A::FxOnOffState => B::FxEnable,
            A::FxParameterValue => B::FxParameterValue,
            A::BrowseFxPresets => B::FxPreset,
            A::PlayRate => B::PlayRate,
            A::Tempo => B::Tempo,
            A::TrackAutomationMode => B::TrackAutomationMode,
            A::TrackMonitoringMode => B::TrackMonitoringMode,
            A::AutomationModeOverride => B::AutomationModeOverride,
            A::ReaperAction => B::Action,
            A::TransportAction => B::Transport,
        }
    }

    pub fn from_learnable_target_kinds(
        set: impl IntoIterator<Item = LearnableTargetKind>,
    ) -> NonCryptoHashSet<ReaperTargetType> {
        set.into_iter()
            .map(ReaperTargetType::from_learnable_target_kind)
            .collect()
    }

    pub fn all() -> NonCryptoHashSet<ReaperTargetType> {
        ReaperTargetType::iter().collect()
    }

    pub fn supports_feedback_resolution(self) -> bool {
        use ReaperTargetType::*;
        matches!(self, PlaytimeSlotSeek | Seek)
    }

    pub fn supports_poll_for_feedback(self) -> bool {
        self.definition().supports_poll_for_feedback()
    }

    pub const fn definition(self) -> &'static TargetTypeDef {
        use ReaperTargetType::*;
        match self {
            Mouse => &MOUSE_TARGET,
            LastTouched => &LAST_TOUCHED_TARGET,
            AutomationModeOverride => &AUTOMATION_MODE_OVERRIDE_TARGET,
            AnyOn => &ANY_ON_TARGET,
            Action => &ACTION_TARGET,
            Transport => &TRANSPORT_TARGET,
            BrowseTracks => &SELECTED_TRACK_TARGET,
            Seek => &SEEK_TARGET,
            PlayRate => &PLAYRATE_TARGET,
            Tempo => &TEMPO_TARGET,
            GoToBookmark => &GO_TO_BOOKMARK_TARGET,
            TrackArm => &TRACK_ARM_TARGET,
            TrackParentSend => &TRACK_PARENT_SEND_TARGET,
            AllTrackFxEnable => &ALL_TRACK_FX_ENABLE_TARGET,
            TrackTool => &TRACK_TOOL_TARGET,
            TrackMute => &TRACK_MUTE_TARGET,
            TrackPeak => &TRACK_PEAK_TARGET,
            TrackPhase => &TRACK_PHASE_TARGET,
            TrackSelection => &TRACK_SELECTION_TARGET,
            TrackAutomationMode => &TRACK_AUTOMATION_MODE_TARGET,
            TrackMonitoringMode => &TRACK_MONITORING_MODE_TARGET,
            TrackTouchState => &TRACK_TOUCH_STATE_TARGET,
            TrackPan => &TRACK_PAN_TARGET,
            TrackWidth => &TRACK_WIDTH_TARGET,
            TrackVolume => &TRACK_VOLUME_TARGET,
            TrackShow => &TRACK_SHOW_TARGET,
            TrackSolo => &TRACK_SOLO_TARGET,
            FxTool => &FX_TOOL_TARGET,
            BrowseFxs => &BROWSE_FXS_TARGET,
            FxEnable => &FX_ENABLE_TARGET,
            FxOnline => &FX_ONLINE_TARGET,
            LoadFxSnapshot => &LOAD_FX_SNAPSHOT_TARGET,
            FxPreset => &FX_PRESET_TARGET,
            FxOpen => &FX_OPEN_TARGET,
            FxParameterValue => &FX_PARAMETER_TARGET,
            FxParameterTouchState => &FX_PARAMETER_TOUCH_STATE_TARGET,
            RouteAutomationMode => &ROUTE_AUTOMATION_MODE_TARGET,
            RouteMono => &ROUTE_MONO_TARGET,
            RouteMute => &ROUTE_MUTE_TARGET,
            RoutePhase => &ROUTE_PHASE_TARGET,
            RoutePan => &ROUTE_PAN_TARGET,
            RouteVolume => &ROUTE_VOLUME_TARGET,
            RouteTouchState => &ROUTE_TOUCH_STATE_TARGET,
            PlaytimeSlotTransportAction => &crate::domain::PLAYTIME_SLOT_TRANSPORT_TARGET,
            PlaytimeColumnAction => &crate::domain::PLAYTIME_COLUMN_TARGET,
            PlaytimeRowAction => &crate::domain::PLAYTIME_ROW_TARGET,
            PlaytimeSlotSeek => &crate::domain::PLAYTIME_SLOT_SEEK_TARGET,
            PlaytimeSlotVolume => &crate::domain::PLAYTIME_SLOT_VOLUME_TARGET,
            PlaytimeSlotManagementAction => &crate::domain::PLAYTIME_SLOT_MANAGEMENT_TARGET,
            PlaytimeMatrixAction => &crate::domain::PLAYTIME_MATRIX_TARGET,
            PlaytimeControlUnitScroll => &crate::domain::PLAYTIME_CONTROL_UNIT_SCROLL_TARGET,
            PlaytimeBrowseCells => &crate::domain::PLAYTIME_BROWSE_CELLS_TARGET,
            SendMidi => &MIDI_SEND_TARGET,
            SendOsc => &OSC_SEND_TARGET,
            Dummy => &DUMMY_TARGET,
            EnableInstances => &ENABLE_INSTANCES_TARGET,
            EnableMappings => &ENABLE_MAPPINGS_TARGET,
            ModifyMapping => &LEARN_MAPPING_TARGET,
            LoadMappingSnapshot => &LOAD_MAPPING_SNAPSHOT_TARGET,
            TakeMappingSnapshot => &SAVE_MAPPING_SNAPSHOT_TARGET,
            BrowseGroup => &BROWSE_GROUP_MAPPINGS_TARGET,
            BrowsePotFilterItems => &BROWSE_POT_FILTER_ITEMS_TARGET,
            BrowsePotPresets => &BROWSE_POT_PRESETS_TARGET,
            PreviewPotPreset => &PREVIEW_POT_PRESET_TARGET,
            LoadPotPreset => &LOAD_POT_PRESET_TARGET,
            CompartmentParameterValue => &COMPARTMENT_PARAMETER_VALUE_TARGET,
            StreamDeckBrightness => &STREAM_DECK_BRIGHTNESS_TARGET,
        }
    }

    pub fn supports_track(self) -> bool {
        self.definition().supports_track()
    }

    pub fn supports_track_must_be_selected(self) -> bool {
        self.definition().supports_track_must_be_selected()
    }

    pub fn supports_track_scrolling(self) -> bool {
        self.definition().supports_track_scrolling()
    }

    pub fn supports_clip_slot(self) -> bool {
        self.definition().supports_clip_slot()
    }

    pub fn supports_fx(self) -> bool {
        self.definition().supports_fx()
    }

    pub fn supports_seek_behavior(self) -> bool {
        self.definition().supports_seek_behavior()
    }

    pub fn supports_tags(self) -> bool {
        self.definition().supports_tags()
    }

    pub fn supports_fx_chain(self) -> bool {
        self.definition().supports_fx_chain()
    }

    pub fn supports_fx_display_type(self) -> bool {
        self.definition().supports_fx_display_type()
    }

    pub fn supports_send(self) -> bool {
        self.definition().supports_send()
    }

    pub fn supports_track_exclusivity(self) -> bool {
        self.definition().supports_track_exclusivity()
    }

    pub fn supports_exclusivity(self) -> bool {
        self.definition().supports_exclusivity()
    }

    pub fn supports_fx_parameter(self) -> bool {
        self.definition().supports_fx_parameter()
    }

    pub fn supports_control(&self) -> bool {
        self.definition().supports_control()
    }

    pub fn supports_feedback(&self) -> bool {
        self.definition().supports_feedback()
    }

    pub fn hint(&self) -> &'static str {
        self.definition().hint()
    }

    /// Produces a shorter name than the Display implementation.
    ///
    /// For example, it doesn't contain the leading context information.
    pub fn short_name(&self) -> &'static str {
        self.definition().short_name()
    }
}

#[derive(
    Copy, Clone, Eq, PartialEq, Hash, Debug, strum::AsRefStr, strum::Display, strum::EnumIter,
)]
pub enum TargetSection {
    Global,
    Project,
    #[strum(serialize = "Marker/region")]
    Bookmark,
    Track,
    #[strum(serialize = "FX chain")]
    FxChain,
    #[strum(serialize = "FX")]
    Fx,
    #[strum(serialize = "FX parameter")]
    FxParameter,
    Pot,
    Send,
    Playtime,
    #[strum(serialize = "MIDI")]
    Midi,
    #[strum(serialize = "OSC")]
    Osc,
    #[strum(serialize = "Stream Deck")]
    StreamDeck,
    ReaLearn,
}

pub struct TargetTypeDef {
    pub lua_only: bool,
    pub section: TargetSection,
    pub name: &'static str,
    pub short_name: &'static str,
    pub hint: &'static str,
    pub supports_track: bool,
    pub supports_mouse_button: bool,
    pub supports_axis: bool,
    pub supports_gang_selected: bool,
    pub supports_gang_grouping: bool,
    pub if_so_supports_track_must_be_selected: bool,
    pub supports_track_scrolling: bool,
    pub supports_clip_slot: bool,
    pub supports_fx: bool,
    pub supports_fx_parameter: bool,
    pub supports_fx_chain: bool,
    pub supports_fx_display_type: bool,
    pub supports_tags: bool,
    pub supports_send: bool,
    pub supports_track_exclusivity: bool,
    pub supports_exclusivity: bool,
    pub supports_poll_for_feedback: bool,
    pub supports_feedback_resolution: bool,
    pub supports_control: bool,
    pub supports_feedback: bool,
    pub supports_seek_behavior: bool,
    pub supports_track_grouping_only_gang_behavior: bool,
    pub supports_real_time_control: bool,
    pub supports_included_targets: bool,
}

impl TargetTypeDef {
    pub const fn section(&self) -> TargetSection {
        self.section
    }
    pub const fn name(&self) -> &'static str {
        self.name
    }
    pub const fn short_name(&self) -> &'static str {
        self.short_name
    }
    pub const fn hint(&self) -> &'static str {
        self.hint
    }
    pub const fn supports_track(&self) -> bool {
        self.supports_track
    }
    pub const fn supports_mouse_button(&self) -> bool {
        self.supports_mouse_button
    }
    pub const fn supports_axis(&self) -> bool {
        self.supports_axis
    }
    pub const fn supports_gang_selected(&self) -> bool {
        self.supports_gang_selected
    }
    pub const fn supports_gang_grouping(&self) -> bool {
        self.supports_gang_grouping
    }
    pub fn supports_track_grouping_only_gang_behavior(&self) -> bool {
        self.supports_track_grouping_only_gang_behavior
            || new_set_track_ui_functions_are_available()
    }
    pub const fn supports_track_must_be_selected(&self) -> bool {
        self.supports_track() && self.if_so_supports_track_must_be_selected
    }
    pub const fn supports_track_scrolling(&self) -> bool {
        self.supports_track_scrolling
    }
    pub const fn supports_clip_slot(&self) -> bool {
        self.supports_clip_slot
    }
    pub const fn supports_seek_behavior(&self) -> bool {
        self.supports_seek_behavior
    }
    pub const fn supports_fx(&self) -> bool {
        self.supports_fx
    }
    pub const fn supports_fx_parameter(&self) -> bool {
        self.supports_fx_parameter
    }
    pub const fn supports_fx_chain(&self) -> bool {
        self.supports_fx() || self.supports_fx_chain
    }
    pub const fn supports_fx_display_type(&self) -> bool {
        self.supports_fx_display_type
    }
    pub const fn supports_tags(&self) -> bool {
        self.supports_tags
    }
    pub const fn supports_send(&self) -> bool {
        self.supports_send
    }
    pub const fn supports_track_exclusivity(&self) -> bool {
        self.supports_track_exclusivity
    }
    pub const fn supports_exclusivity(&self) -> bool {
        self.supports_exclusivity
    }
    pub const fn supports_poll_for_feedback(&self) -> bool {
        self.supports_poll_for_feedback
    }
    pub const fn supports_feedback_resolution(&self) -> bool {
        self.supports_feedback_resolution
    }
    pub const fn supports_control(&self) -> bool {
        self.supports_control
    }
    pub const fn supports_feedback(&self) -> bool {
        self.supports_feedback
    }
    pub const fn supports_real_time_control(&self) -> bool {
        self.supports_real_time_control
    }
    pub const fn supports_included_targets(&self) -> bool {
        self.supports_included_targets
    }
}

pub const DEFAULT_TARGET: TargetTypeDef = TargetTypeDef {
    lua_only: false,
    section: TargetSection::Global,
    name: "",
    short_name: "",
    hint: "",
    supports_control: true,
    supports_feedback: true,
    supports_track: false,
    supports_mouse_button: false,
    supports_axis: false,
    supports_gang_selected: false,
    supports_gang_grouping: false,
    if_so_supports_track_must_be_selected: true,
    supports_track_scrolling: false,
    supports_clip_slot: false,
    supports_fx: false,
    supports_fx_parameter: false,
    supports_fx_chain: false,
    supports_fx_display_type: false,
    supports_tags: false,
    supports_send: false,
    supports_track_exclusivity: false,
    supports_exclusivity: false,
    supports_poll_for_feedback: false,
    supports_feedback_resolution: false,
    supports_seek_behavior: false,
    supports_track_grouping_only_gang_behavior: false,
    supports_real_time_control: false,
    supports_included_targets: false,
};

pub const AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY: &str = "Automatic feedback via polling only";
