use crate::application::{
    merge_affected, ActivationConditionCommand, ActivationConditionModel, ActivationConditionProp,
    Affected, Change, ChangeResult, GetProcessingRelevance, MakeFxNonStickyMode,
    MakeTrackNonStickyMode, MappingExtensionModel, ModeCommand, ModeModel, ModeProp,
    ProcessingRelevance, SourceCommand, SourceModel, SourceProp, TargetCategory, TargetCommand,
    TargetModel, TargetModelFormatVeryShort, TargetModelWithContext, TargetProp,
};
use crate::domain::{
    ActivationCondition, CompartmentKind, CompoundMappingSource, CompoundMappingTarget,
    EelTransformation, ExtendedProcessorContext, ExtendedSourceCharacter, FeedbackSendBehavior,
    GroupId, MainMapping, MappingId, MappingKey, Mode, PersistentMappingProcessingState,
    ProcessorMappingOptions, QualifiedMappingId, RealearnTarget, ReaperTarget, Script, Tag,
    TargetCharacter, UnresolvedCompoundMappingTarget, VirtualFx, VirtualTrack,
};
use helgoboss_learn::{
    AbsoluteMode, ControlType, DetailedSourceCharacter, DiscreteIncrement, Interval,
    ModeApplicabilityCheckInput, ModeParameter, SourceCharacter, Target, UnitValue,
};

use reaper_high::{Fx, Track};
use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

pub enum MappingCommand {
    SetName(String),
    SetTags(Vec<Tag>),
    SetGroupId(GroupId),
    SetIsEnabled(bool),
    SetControlIsEnabled(bool),
    SetFeedbackIsEnabled(bool),
    SetFeedbackSendBehavior(FeedbackSendBehavior),
    SetVisibleInProjection(bool),
    SetBeepOnSuccess(bool),
    ChangeActivationCondition(ActivationConditionCommand),
    ChangeSource(SourceCommand),
    ChangeMode(ModeCommand),
    ChangeTarget(TargetCommand),
}

#[derive(Eq, PartialEq)]
pub enum MappingProp {
    Name,
    Tags,
    GroupId,
    IsEnabled,
    ControlIsEnabled,
    FeedbackIsEnabled,
    FeedbackSendBehavior,
    VisibleInProjection,
    BeepOnSuccess,
    AdvancedSettings,
    InActivationCondition(Affected<ActivationConditionProp>),
    InSource(Affected<SourceProp>),
    InMode(Affected<ModeProp>),
    InTarget(Affected<TargetProp>),
}

impl GetProcessingRelevance for MappingProp {
    fn processing_relevance(&self) -> Option<ProcessingRelevance> {
        use MappingProp as P;
        match self {
            P::Name
            | P::Tags
            | P::ControlIsEnabled
            | P::FeedbackIsEnabled
            | P::FeedbackSendBehavior
            | P::VisibleInProjection
            | P::AdvancedSettings
            | P::BeepOnSuccess => Some(ProcessingRelevance::ProcessingRelevant),
            P::InActivationCondition(p) => p.processing_relevance(),
            P::InMode(p) => p.processing_relevance(),
            P::InSource(p) => p.processing_relevance(),
            P::InTarget(p) => p.processing_relevance(),
            P::IsEnabled => Some(ProcessingRelevance::PersistentProcessingRelevant),
            MappingProp::GroupId => {
                // This is handled in different ways.
                None
            }
        }
    }
}

/// A model for creating mappings (a combination of source, mode and target).
#[derive(Clone, Debug)]
pub struct MappingModel {
    id: MappingId,
    key: MappingKey,
    compartment: CompartmentKind,
    name: String,
    tags: Vec<Tag>,
    group_id: GroupId,
    is_enabled: bool,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
    feedback_send_behavior: FeedbackSendBehavior,
    pub activation_condition_model: ActivationConditionModel,
    visible_in_projection: bool,
    beep_on_success: bool,
    pub source_model: SourceModel,
    pub mode_model: ModeModel,
    pub target_model: TargetModel,
    advanced_settings: Option<serde_yaml::mapping::Mapping>,
    extension_model: MappingExtensionModel,
}

pub type SharedMapping = Rc<RefCell<MappingModel>>;

pub fn share_mapping(mapping: MappingModel) -> SharedMapping {
    Rc::new(RefCell::new(mapping))
}

// We design mapping models as entity (in the DDD sense), so we compare them by ID, not by value.
// Because we store everything in memory instead of working with a database, the memory
// address serves us as ID. That means we just compare pointers.
//
// In all functions which don't need access to the mapping's internal state (comparisons, hashing
// etc.) we use `*const MappingModel` as parameter type because this saves the consumer from
// having to borrow the mapping (when kept in a RefCell). Whenever we can we should compare pointers
// directly, in order to prevent borrowing just to make the following comparison (the RefCell
// comparison internally calls `borrow()`!).
impl PartialEq for MappingModel {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self as _, other as _)
    }
}

impl Change<'_> for MappingModel {
    type Command = MappingCommand;
    type Prop = MappingProp;

    fn change(&mut self, cmd: MappingCommand) -> Option<Affected<MappingProp>> {
        use Affected::*;
        use MappingCommand as C;
        use MappingProp as P;
        let affected = match cmd {
            C::SetName(v) => {
                self.name = v;
                One(P::Name)
            }
            C::SetTags(v) => {
                self.tags = v;
                One(P::Tags)
            }
            C::SetGroupId(v) => {
                self.group_id = v;
                One(P::GroupId)
            }
            C::SetIsEnabled(v) => {
                self.is_enabled = v;
                One(P::IsEnabled)
            }
            C::SetControlIsEnabled(v) => {
                self.control_is_enabled = v;
                One(P::ControlIsEnabled)
            }
            C::SetFeedbackIsEnabled(v) => {
                self.feedback_is_enabled = v;
                One(P::FeedbackIsEnabled)
            }
            C::SetFeedbackSendBehavior(v) => {
                self.feedback_send_behavior = v;
                One(P::FeedbackSendBehavior)
            }
            C::SetVisibleInProjection(v) => {
                self.visible_in_projection = v;
                One(P::VisibleInProjection)
            }
            C::SetBeepOnSuccess(v) => {
                self.beep_on_success = v;
                One(P::BeepOnSuccess)
            }
            C::ChangeActivationCondition(cmd) => {
                return self
                    .activation_condition_model
                    .change(cmd)
                    .map(|affected| One(P::InActivationCondition(affected)));
            }
            C::ChangeSource(cmd) => {
                return self
                    .source_model
                    .change(cmd)
                    .map(|affected| One(P::InSource(affected)));
            }
            C::ChangeMode(cmd) => {
                return self
                    .mode_model
                    .change(cmd)
                    .map(|affected| One(P::InMode(affected)));
            }
            C::ChangeTarget(cmd) => {
                return self
                    .target_model
                    .change(cmd)
                    .map(|affected| One(P::InTarget(affected)));
            }
        };
        Some(affected)
    }
}

impl MappingModel {
    pub fn new(
        compartment: CompartmentKind,
        initial_group_id: GroupId,
        key: MappingKey,
        id: MappingId,
    ) -> Self {
        Self {
            id,
            key,
            compartment,
            name: Default::default(),
            tags: Default::default(),
            group_id: initial_group_id,
            is_enabled: true,
            control_is_enabled: true,
            feedback_is_enabled: true,
            feedback_send_behavior: Default::default(),
            activation_condition_model: Default::default(),
            visible_in_projection: true,
            beep_on_success: false,
            source_model: SourceModel::new(),
            mode_model: Default::default(),
            target_model: TargetModel::default_for_compartment(compartment),
            advanced_settings: None,
            extension_model: Default::default(),
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn key(&self) -> &MappingKey {
        &self.key
    }

    pub fn group_id(&self) -> GroupId {
        self.group_id
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    pub fn control_is_enabled(&self) -> bool {
        self.control_is_enabled
    }

    pub fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled
    }

    pub fn feedback_send_behavior(&self) -> FeedbackSendBehavior {
        self.feedback_send_behavior
    }

    pub fn visible_in_projection(&self) -> bool {
        self.visible_in_projection
    }

    pub fn beep_on_success(&self) -> bool {
        self.beep_on_success
    }

    pub fn activation_condition_model(&self) -> &ActivationConditionModel {
        &self.activation_condition_model
    }

    pub fn reset_key(&mut self) {
        self.key = MappingKey::random();
    }

    pub fn qualified_id(&self) -> QualifiedMappingId {
        QualifiedMappingId::new(self.compartment, self.id)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn effective_name(&self) -> String {
        if self.name.is_empty() {
            TargetModelFormatVeryShort(&self.target_model).to_string()
        } else {
            self.name.clone()
        }
    }

    pub fn make_target_non_sticky(
        &mut self,
        context: ExtendedProcessorContext,
        track_mode: MakeTrackNonStickyMode,
        fx_mode: MakeFxNonStickyMode,
    ) -> Option<Affected<MappingProp>> {
        self.make_target_non_sticky_internal(
            context,
            |t| track_mode.build_virtual_track(t.as_ref()),
            |fx| fx_mode.build_virtual_fx(fx.as_ref()),
        )
    }

    #[must_use]
    fn make_target_non_sticky_internal(
        &mut self,
        context: ExtendedProcessorContext,
        create_virtual_track: impl FnOnce(Option<Track>) -> Option<VirtualTrack>,
        create_virtual_fx: impl FnOnce(Option<Fx>) -> Option<VirtualFx>,
    ) -> Option<Affected<MappingProp>> {
        let compartment = self.compartment();
        let target = &mut self.target_model;
        match target.category() {
            TargetCategory::Reaper => {
                // Change FX
                if target.supports_fx() {
                    let target_with_context = target.with_context(context, compartment);
                    let containing_fx = context.context().containing_fx();
                    let resolved_fx = target_with_context.first_fx().ok();
                    let new_virtual_fx = if resolved_fx.as_ref() == Some(containing_fx) {
                        // This is ourselves!
                        Some(VirtualFx::This)
                    } else {
                        create_virtual_fx(resolved_fx)
                    };
                    if let Some(fx) = new_virtual_fx {
                        let _ = target.set_virtual_fx(fx, context, compartment);
                    }
                }
                // Change track
                if target.target_type().supports_track() {
                    let new_virtual_track = if target.fx_type().requires_fx_chain() {
                        let resolved_track = target
                            .with_context(context, compartment)
                            .first_effective_track()
                            .ok();
                        create_virtual_track(resolved_track)
                    } else {
                        // Track doesn't matter at all. We change it to <This>. Looks nice.
                        Some(VirtualTrack::This)
                    };
                    if let Some(t) = new_virtual_track {
                        let _ = target.set_virtual_track(t, Some(context.context()));
                    }
                }
                Some(Affected::Multiple)
            }
            TargetCategory::Virtual => None,
        }
    }

    pub fn make_target_sticky(
        &mut self,
        context: ExtendedProcessorContext,
    ) -> Result<Option<Affected<MappingProp>>, Box<dyn Error>> {
        let target = &mut self.target_model;
        match target.category() {
            TargetCategory::Reaper => {
                if target.supports_track() {
                    target.make_track_sticky(self.compartment, context)?;
                }
                if target.supports_fx() {
                    target.make_fx_sticky(self.compartment, context)?;
                }
                if target.supports_route() {
                    target.make_route_sticky(self.compartment, context)?;
                }
            }
            TargetCategory::Virtual => {}
        }
        Ok(Some(Affected::Multiple))
    }

    pub fn advanced_settings(&self) -> Option<&serde_yaml::Mapping> {
        self.advanced_settings.as_ref()
    }

    fn update_extension_model_from_advanced_settings(&mut self) -> Result<(), String> {
        // Immediately update extension model
        let extension_model = if let Some(yaml_mapping) = self.advanced_settings() {
            serde_yaml::from_value(serde_yaml::Value::Mapping(yaml_mapping.clone()))
                .map_err(|e| e.to_string())?
        } else {
            Default::default()
        };
        self.extension_model = extension_model;
        Ok(())
    }

    pub fn duplicate(&self) -> MappingModel {
        MappingModel {
            id: MappingId::random(),
            key: MappingKey::random(),
            ..self.clone()
        }
    }

    pub fn compartment(&self) -> CompartmentKind {
        self.compartment
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
    ) -> MappingModelWithContext<'a> {
        MappingModelWithContext {
            mapping: self,
            context,
        }
    }

    #[must_use]
    pub fn adjust_mode_if_necessary(
        &mut self,
        context: ExtendedProcessorContext,
    ) -> Option<Affected<MappingProp>> {
        let with_context = self.with_context(context);
        if with_context.absolute_mode_makes_sense() == Ok(false) {
            if let Ok(preferred_mode_type) = with_context.preferred_mode_type() {
                self.mode_model
                    .change(ModeCommand::SetAbsoluteMode(preferred_mode_type));
                self.set_preferred_mode_values(context)
            } else {
                None
            }
        } else {
            None
        }
    }

    #[must_use]
    pub fn reset_mode(
        &mut self,
        context: ExtendedProcessorContext,
    ) -> Option<Affected<MappingProp>> {
        self.mode_model.change(ModeCommand::ResetWithinType);
        let _ = self.set_preferred_mode_values(context);
        Some(Affected::Multiple)
    }

    pub fn set_advanced_settings(
        &mut self,
        yaml: Option<serde_yaml::mapping::Mapping>,
    ) -> ChangeResult<MappingProp> {
        self.advanced_settings = yaml;
        self.update_extension_model_from_advanced_settings()?;
        Ok(Some(Affected::One(MappingProp::AdvancedSettings)))
    }

    #[must_use]
    pub fn set_absolute_mode_and_preferred_values(
        &mut self,
        context: ExtendedProcessorContext,
        mode: AbsoluteMode,
    ) -> Option<Affected<MappingProp>> {
        let affected_1 = self.change(MappingCommand::ChangeMode(ModeCommand::SetAbsoluteMode(
            mode,
        )));
        let affected_2 = self.set_preferred_mode_values(context);
        merge_affected(affected_1, affected_2)
    }

    // Changes mode settings if there are some preferred ones for a certain source or target.
    #[must_use]
    fn set_preferred_mode_values(
        &mut self,
        context: ExtendedProcessorContext,
    ) -> Option<Affected<MappingProp>> {
        let affected_1 = self
            .mode_model
            .change(ModeCommand::SetStepSizeInterval(
                self.with_context(context).preferred_step_size_interval(),
            ))
            .map(|affected| Affected::One(MappingProp::InMode(affected)));
        let affected_2 = self
            .mode_model
            .change(ModeCommand::SetStepFactorInterval(
                self.with_context(context).preferred_step_factor_interval(),
            ))
            .map(|affected| Affected::One(MappingProp::InMode(affected)));
        merge_affected(affected_1, affected_2)
    }

    pub fn base_mode_applicability_check_input(&self) -> ModeApplicabilityCheckInput {
        let transformation =
            EelTransformation::compile_for_control(self.mode_model.eel_control_transformation());
        ModeApplicabilityCheckInput {
            target_is_virtual: self.target_model.is_virtual(),
            // TODO-high-discrete Enable (also taking source into consideration!)
            target_supports_discrete_values: false,
            control_transformation_uses_time: transformation
                .as_ref()
                .map(|t| t.uses_time())
                .unwrap_or(false),
            control_transformation_produces_relative_values: transformation
                .as_ref()
                .map(|t| t.produces_relative_values())
                .unwrap_or(false),
            is_feedback: false,
            make_absolute: self.mode_model.make_absolute(),
            use_textual_feedback: self.mode_model.feedback_type().is_textual(),
            // Any is okay, will be overwritten.
            source_character: DetailedSourceCharacter::RangeControl,
            absolute_mode: self.mode_model.absolute_mode(),
            fire_mode: self.mode_model.fire_mode(),
            target_value_sequence_is_set: !self.mode_model.target_value_sequence().is_empty(),
        }
    }

    pub fn control_is_enabled_and_supported(&self) -> bool {
        self.control_is_enabled()
            && self.source_model.supports_control()
            && self.target_model.supports_control()
    }

    pub fn feedback_is_enabled_and_supported(&self) -> bool {
        self.feedback_is_enabled()
            && self.source_model.supports_feedback()
            && self.target_model.supports_feedback()
    }

    pub fn mode_parameter_is_relevant(
        &self,
        mode_parameter: ModeParameter,
        base_input: ModeApplicabilityCheckInput,
        possible_source_characters: &[DetailedSourceCharacter],
    ) -> bool {
        self.mode_model.mode_parameter_is_relevant(
            mode_parameter,
            base_input,
            possible_source_characters,
            self.control_is_enabled_and_supported(),
            self.feedback_is_enabled_and_supported(),
        )
    }

    fn create_source(&self) -> CompoundMappingSource {
        self.source_model.create_source()
    }

    fn create_mode(&self) -> Mode {
        let possible_source_characters = self.source_model.possible_detailed_characters();
        self.mode_model.create_mode(
            self.base_mode_applicability_check_input(),
            &possible_source_characters,
        )
    }

    fn create_target(&self) -> Option<UnresolvedCompoundMappingTarget> {
        self.target_model.create_target(self.compartment).ok()
    }

    pub fn create_persistent_mapping_processing_state(&self) -> PersistentMappingProcessingState {
        PersistentMappingProcessingState {
            is_enabled: self.is_enabled(),
        }
    }

    pub fn get_simple_mapping(&self) -> Option<playtime_api::runtime::SimpleMapping> {
        let target = self.target_model.simple_target()?;
        let source = self.source_model.simple_source()?;
        let mapping = playtime_api::runtime::SimpleMapping { source, target };
        Some(mapping)
    }

    /// Creates an intermediate mapping for splintering into very dedicated mapping types that are
    /// then going to be distributed to real-time and main processor.
    pub fn create_main_mapping(&self, group_data: GroupData) -> MainMapping {
        let id = self.id;
        let source = self.create_source();
        let mode = self.create_mode();
        let unresolved_target = self.create_target();
        let activation_condition = self
            .activation_condition_model
            .create_activation_condition();
        let options = ProcessorMappingOptions {
            // TODO-medium Encapsulate, don't set here
            target_is_active: false,
            persistent_processing_state: self.create_persistent_mapping_processing_state(),
            control_is_enabled: group_data.control_is_enabled && self.control_is_enabled(),
            feedback_is_enabled: group_data.feedback_is_enabled && self.feedback_is_enabled(),
            feedback_send_behavior: self.feedback_send_behavior(),
            beep_on_success: self.beep_on_success,
        };
        let mut merged_tags = group_data.tags;
        merged_tags.extend_from_slice(&self.tags);
        MainMapping::new(
            self.compartment,
            id,
            &self.key,
            self.group_id(),
            self.name.clone(),
            merged_tags,
            source,
            mode,
            self.mode_model.group_interaction(),
            unresolved_target,
            group_data.activation_condition,
            activation_condition,
            options,
            self.extension_model
                .create_mapping_extension()
                .unwrap_or_default(),
        )
    }
}

pub struct GroupData {
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
    pub activation_condition: ActivationCondition,
    pub tags: Vec<Tag>,
}

impl Default for GroupData {
    fn default() -> Self {
        Self {
            control_is_enabled: true,
            feedback_is_enabled: true,
            activation_condition: ActivationCondition::Always,
            tags: vec![],
        }
    }
}

pub struct MappingModelWithContext<'a> {
    mapping: &'a MappingModel,
    context: ExtendedProcessorContext<'a>,
}

impl MappingModelWithContext<'_> {
    /// Returns if the absolute make sense under the current conditions.
    ///
    /// Conditions are:
    ///
    /// - Source character
    /// - Target character and control type
    pub fn absolute_mode_makes_sense(&self) -> Result<bool, &'static str> {
        use ExtendedSourceCharacter::*;
        use SourceCharacter::*;
        let source_character = self.mapping.source_model.character();
        let absolute_mode = self.mapping.mode_model.absolute_mode();
        let makes_sense = match source_character {
            Normal(RangeElement) => match absolute_mode {
                AbsoluteMode::Normal
                | AbsoluteMode::MakeRelative
                | AbsoluteMode::PerformanceControl => true,
                AbsoluteMode::IncrementalButton | AbsoluteMode::ToggleButton => false,
            },
            Normal(MomentaryButton | ToggleButton) => {
                let target = self.target_with_context().resolve_first()?;
                let target_is_relative = target
                    .control_type(self.context.control_context())
                    .is_relative();
                match absolute_mode {
                    AbsoluteMode::Normal | AbsoluteMode::ToggleButton => !target_is_relative,
                    AbsoluteMode::IncrementalButton => {
                        if target_is_relative {
                            true
                        } else {
                            match target.character(self.context.control_context()) {
                                TargetCharacter::Discrete
                                | TargetCharacter::Continuous
                                | TargetCharacter::VirtualMulti => true,
                                TargetCharacter::Trigger
                                | TargetCharacter::Switch
                                | TargetCharacter::VirtualButton => false,
                            }
                        }
                    }
                    AbsoluteMode::MakeRelative => {
                        // "Incremental button" is the correct special form of "Make relative"
                        // for button presses!
                        false
                    }
                    AbsoluteMode::PerformanceControl => false,
                }
            }
            Normal(Encoder1) | Normal(Encoder2) | Normal(Encoder3) => {
                // TODO-low No idea why this is true. But so what, auto-correct settings is not
                //  really a thing anymore?
                true
            }
            VirtualContinuous => true,
        };
        Ok(makes_sense)
    }

    pub fn has_target(&self, target: &ReaperTarget) -> bool {
        self.target_with_context()
            .resolve()
            .iter()
            .flatten()
            .any(|t| match t {
                CompoundMappingTarget::Reaper(t) => &**t == target,
                _ => false,
            })
    }

    pub fn preferred_mode_type(&self) -> Result<AbsoluteMode, &'static str> {
        use ExtendedSourceCharacter::*;
        use SourceCharacter::*;
        let result = match self.mapping.source_model.character() {
            Normal(RangeElement) | VirtualContinuous => AbsoluteMode::Normal,
            Normal(MomentaryButton) | Normal(ToggleButton) => {
                let target = self.target_with_context().resolve_first()?;
                if target
                    .control_type(self.context.control_context())
                    .is_relative()
                {
                    AbsoluteMode::IncrementalButton
                } else {
                    match target.character(self.context.control_context()) {
                        TargetCharacter::Trigger
                        | TargetCharacter::Continuous
                        | TargetCharacter::VirtualMulti => AbsoluteMode::Normal,
                        TargetCharacter::Switch | TargetCharacter::VirtualButton => {
                            AbsoluteMode::ToggleButton
                        }
                        TargetCharacter::Discrete => AbsoluteMode::IncrementalButton,
                    }
                }
            }
            Normal(Encoder1) | Normal(Encoder2) | Normal(Encoder3) => AbsoluteMode::Normal,
        };
        Ok(result)
    }

    /// If this returns `true`, the Speed sliders will be shown, allowing relative
    /// increments/decrements to be throttled or multiplied.
    pub fn uses_step_factors(&self) -> bool {
        let mode = self.mapping.create_mode();
        if mode.settings().make_absolute {
            // If we convert increments to absolute values, we want step sizes of course.
            return false;
        }
        if !mode.settings().target_value_sequence.is_empty() {
            // If we have a target value sequence, we are discrete all the way!
            return true;
        }
        let target = match self.target_with_context().resolve_first().ok() {
            None => return false,
            Some(t) => t,
        };
        match target.control_type(self.context.control_context()) {
            ControlType::AbsoluteContinuousRetriggerable => {
                // Retriggerable targets which can't report the current value and are pure triggers.
                // In #613, we introduced a convenient behavior that allows encoder movements
                // trigger such targets. But we want to support throttling the encoder speed, so
                // we consider this as using step counts.
                !target.can_report_current_value()
            }
            ControlType::AbsoluteContinuous => false,
            ControlType::AbsoluteContinuousRoundable { .. } => false,
            ControlType::AbsoluteDiscrete { .. } => true,
            ControlType::Relative => true,
            ControlType::VirtualMulti => true,
            ControlType::VirtualButton => false,
        }
    }

    fn preferred_step_size_interval(&self) -> Interval<UnitValue> {
        match self.target_step_size() {
            Some(step_size) => Interval::new(step_size, step_size),
            None => ModeModel::default_step_size_interval(),
        }
    }

    fn preferred_step_factor_interval(&self) -> Interval<DiscreteIncrement> {
        let inc = DiscreteIncrement::new(1);
        Interval::new(inc, inc)
    }

    fn target_step_size(&self) -> Option<UnitValue> {
        let target = self.target_with_context().resolve_first().ok()?;
        target
            .control_type(self.context.control_context())
            .step_size()
    }

    fn target_with_context(&self) -> TargetModelWithContext<'_> {
        self.mapping
            .target_model
            .with_context(self.context, self.mapping.compartment)
    }
}
