use std::cell::{Ref, RefCell, RefMut};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::RwLock;

use enum_map::EnumMap;
use reaper_high::Fx;
use rxrust::prelude::*;

use crate::base::Prop;
use crate::domain::{
    AnyThreadBackboneState, BackboneState, Compartment, FxDescriptor,
    GlobalControlAndFeedbackState, GroupId, InstanceId, MappingId, MappingSnapshotContainer,
    ProcessorContext, QualifiedMappingId, Tag, TagScope, TrackDescriptor,
    VirtualMappingSnapshotIdForLoad,
};
use base::{NamedChannelSender, SenderToNormalThread};
use pot::{CurrentPreset, OptFilter, PotFavorites, PotFilterExcludes, PotIntegration};
use pot::{PotUnit, PresetId, SharedRuntimePotUnit};
use realearn_api::persistence::PotFilterKind;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;
pub type WeakInstanceState = Weak<RefCell<InstanceState>>;

/// State connected to the instance which also needs to be accessible from layers *above* the
/// processing layer (otherwise it could reside in the main processor).
///
/// This also contains ReaLearn-specific target state that is persistent, even if the state is of
/// more global nature than an instance. Rationale: Otherwise we would need to come up with a
/// new place to persist state, e.g. the project data. But this makes things more complex and
/// raises questions like whether to keep the data in the project even the last ReaLearn instance is
/// gone and what to do if there's no project context - on the monitoring FX chain.
///
/// For state of global nature which doesn't need to be persisted, see `RealearnTargetState`.
#[derive(Debug)]
pub struct InstanceState {
    instance_id: InstanceId,
    processor_context: ProcessorContext,
    /// Owned clip matrix or reference to a clip matrix owned by another instance.
    ///
    /// Persistent.
    #[cfg(feature = "playtime")]
    clip_matrix_ref: Option<ClipMatrixRef>,
    instance_feedback_event_sender: SenderToNormalThread<InstanceStateChanged>,
    #[cfg(feature = "playtime")]
    clip_matrix_event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
    #[cfg(feature = "playtime")]
    audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    #[cfg(feature = "playtime")]
    real_time_processor_sender: base::SenderToRealTimeThread<crate::domain::NormalRealTimeTask>,
    /// Which mappings are in which group.
    ///
    /// - Not persistent
    /// - Used for target "ReaLearn: Browse group mappings"
    /// - Automatically filled by main processor on sync
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer,
    ///   but it's very related to the active mapping by group, so we decided to keep it here too.
    // TODO-low-multi-config Qualify by config
    mappings_by_group: EnumMap<Compartment, HashMap<GroupId, Vec<MappingId>>>,
    /// Which is the active mapping in which group.
    ///
    /// - Persistent
    /// - Set by target "ReaLearn: Browse group mappings".
    /// - Non-redundant state!
    // TODO-low-multi-config Qualify by config
    active_mapping_by_group: EnumMap<Compartment, HashMap<GroupId, MappingId>>,
    /// Additional info about mappings.
    ///
    /// - Not persistent
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer.
    // TODO-low-multi-config Qualify by config
    mapping_infos: HashMap<QualifiedMappingId, MappingInfo>,
    /// The mappings which are on.
    ///
    /// - Not persistent
    /// - "on" = enabled & control or feedback enabled & mapping active & target active
    /// - Completely derived from mappings, so it's redundant state.
    /// - It's needed by both processing layer and layers above.
    // TODO-low-multi-config Qualify by config
    on_mappings: Prop<HashSet<QualifiedMappingId>>,
    /// Whether control/feedback are globally active.
    ///
    /// Not persistent.
    // TODO-low-multi-config Make fully qualified
    global_control_and_feedback_state: Prop<GlobalControlAndFeedbackState>,
    /// All mapping tags whose mappings have been switched on via tag.
    ///
    /// - Persistent
    /// - Set by target "ReaLearn: Enable/disable mappings".
    /// - Non-redundant state!
    // TODO-low-multi-config Make fully qualified
    active_mapping_tags: EnumMap<Compartment, HashSet<Tag>>,
    /// All instance tags whose instances have been switched on via tag.
    ///
    /// - Persistent
    /// - Set by target "ReaLearn: Enable/disable instances".
    /// - Non-redundant state!
    active_instance_tags: HashSet<Tag>,
    /// Instance track.
    ///
    /// The instance track is persistent but it's persisted from the session, not from here.
    /// This track descriptor is a descriptor suited for runtime, not for persistence.
    instance_track_descriptor: TrackDescriptor,
    /// Instance FX.
    ///
    /// See instance track to learn about persistence.
    // TODO-low It's not so cool that we have the target activation condition as part of the
    //  descriptors (e.g. enable_only_if_track_selected) because we must be sure to set them
    //  to false.
    //  We should probably use the following distinction instead:
    //  - TrackTargetDescriptor contains TrackDescriptor contains VirtualTrack
    //  - FxTargetDescriptor contains FxDescriptor contains VirtualFx
    //  ... where TrackTargetDescriptor contains the condition and TrackDescriptor doesn't.
    instance_fx_descriptor: FxDescriptor,
    /// Mapping snapshots.
    ///
    /// Persistent.
    // TODO-low-multi-config Make fully qualified
    mapping_snapshot_container: EnumMap<Compartment, MappingSnapshotContainer>,
    /// Saves the current state for Pot preset navigation.
    ///
    /// Persistent.
    pot_unit: PotUnit,
    // TODO-low-multi-config Make fully qualified
    mapping_which_learns_source: Prop<Option<QualifiedMappingId>>,
    mapping_which_learns_target: Prop<Option<QualifiedMappingId>>,
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub enum ClipMatrixRef {
    Own(Box<playtime_clip_engine::base::Matrix>),
    Foreign(InstanceId),
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub struct MatrixHandler {
    instance_id: InstanceId,
    audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    real_time_processor_sender: base::SenderToRealTimeThread<crate::domain::NormalRealTimeTask>,
    event_sender: base::SenderToNormalThread<QualifiedClipMatrixEvent>,
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub struct QualifiedClipMatrixEvent {
    pub instance_id: InstanceId,
    pub event: playtime_clip_engine::base::ClipMatrixEvent,
}

#[cfg(feature = "playtime")]
impl MatrixHandler {
    fn new(
        instance_id: InstanceId,
        audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
        real_time_processor_sender: base::SenderToRealTimeThread<crate::domain::NormalRealTimeTask>,
        event_sender: base::SenderToNormalThread<QualifiedClipMatrixEvent>,
    ) -> Self {
        Self {
            instance_id,
            audio_hook_task_sender,
            real_time_processor_sender,
            event_sender,
        }
    }
}

#[cfg(feature = "playtime")]
impl playtime_clip_engine::base::ClipMatrixHandler for MatrixHandler {
    fn init_recording(&self, command: playtime_clip_engine::base::HandlerInitRecordingCommand) {
        use crate::domain::{NormalAudioHookTask, NormalRealTimeTask};
        use playtime_clip_engine::rt::audio_hook::ClipEngineAudioHookCommand;
        use playtime_clip_engine::rt::fx_hook::ClipEngineFxHookCommand;
        match command.create_specific_command() {
            playtime_clip_engine::base::SpecificInitRecordingCommand::HardwareInput(t) => {
                let playtime_command = ClipEngineAudioHookCommand::InitRecording(t);
                self.audio_hook_task_sender.send_complaining(
                    NormalAudioHookTask::PlaytimeClipEngineCommand(playtime_command),
                );
            }
            playtime_clip_engine::base::SpecificInitRecordingCommand::FxInput(t) => {
                let playtime_command = ClipEngineFxHookCommand::InitRecording(t);
                self.real_time_processor_sender.send_complaining(
                    NormalRealTimeTask::PlaytimeClipEngineCommand(playtime_command),
                );
            }
        }
    }

    fn emit_event(&self, event: playtime_clip_engine::base::ClipMatrixEvent) {
        let event = QualifiedClipMatrixEvent {
            instance_id: self.instance_id,
            event,
        };
        self.event_sender.send_complaining(event);
    }
}

#[derive(Debug)]
pub struct MappingInfo {
    pub name: String,
}

impl InstanceState {
    pub(super) fn new(
        instance_id: InstanceId,
        processor_context: ProcessorContext,
        instance_feedback_event_sender: SenderToNormalThread<InstanceStateChanged>,
        #[cfg(feature = "playtime")] clip_matrix_event_sender: SenderToNormalThread<
            QualifiedClipMatrixEvent,
        >,
        #[cfg(feature = "playtime")] audio_hook_task_sender: base::SenderToRealTimeThread<
            crate::domain::NormalAudioHookTask,
        >,
        #[cfg(feature = "playtime")] real_time_processor_sender: base::SenderToRealTimeThread<
            crate::domain::NormalRealTimeTask,
        >,
    ) -> Self {
        Self {
            instance_id,
            processor_context,
            #[cfg(feature = "playtime")]
            clip_matrix_ref: None,
            instance_feedback_event_sender,
            #[cfg(feature = "playtime")]
            clip_matrix_event_sender,
            #[cfg(feature = "playtime")]
            audio_hook_task_sender,
            #[cfg(feature = "playtime")]
            real_time_processor_sender,
            mappings_by_group: Default::default(),
            active_mapping_by_group: Default::default(),
            mapping_infos: Default::default(),
            on_mappings: Default::default(),
            global_control_and_feedback_state: Default::default(),
            active_mapping_tags: Default::default(),
            active_instance_tags: Default::default(),
            instance_track_descriptor: Default::default(),
            instance_fx_descriptor: Default::default(),
            mapping_snapshot_container: Default::default(),
            pot_unit: Default::default(),
            mapping_which_learns_source: Default::default(),
            mapping_which_learns_target: Default::default(),
        }
    }

    pub fn set_mapping_which_learns_source(
        &mut self,
        mapping_id: Option<QualifiedMappingId>,
    ) -> Option<QualifiedMappingId> {
        let previous_value = self.mapping_which_learns_source.replace(mapping_id);
        self.instance_feedback_event_sender
            .send_complaining(InstanceStateChanged::MappingWhichLearnsSourceChanged { mapping_id });
        previous_value
    }

    pub fn set_mapping_which_learns_target(
        &mut self,
        mapping_id: Option<QualifiedMappingId>,
    ) -> Option<QualifiedMappingId> {
        let previous_value = self.mapping_which_learns_target.replace(mapping_id);
        self.instance_feedback_event_sender
            .send_complaining(InstanceStateChanged::MappingWhichLearnsTargetChanged { mapping_id });
        previous_value
    }

    pub fn mapping_which_learns_source(&self) -> &Prop<Option<QualifiedMappingId>> {
        &self.mapping_which_learns_source
    }

    pub fn mapping_which_learns_target(&self) -> &Prop<Option<QualifiedMappingId>> {
        &self.mapping_which_learns_target
    }

    pub fn mapping_is_learning_source(&self, id: QualifiedMappingId) -> bool {
        match self.mapping_which_learns_source.get_ref() {
            None => false,
            Some(i) => *i == id,
        }
    }

    pub fn mapping_is_learning_target(&self, id: QualifiedMappingId) -> bool {
        match self.mapping_which_learns_target.get_ref() {
            None => false,
            Some(i) => *i == id,
        }
    }

    /// Returns the runtime pot unit associated with this instance.
    ///
    /// If the pot unit isn't loaded yet and loading has not been attempted yet, loads it.
    ///
    /// Returns an error if the necessary pot database is not available.
    pub fn pot_unit(&mut self) -> Result<SharedRuntimePotUnit, &'static str> {
        let integration = RealearnPotIntegration::new(
            self.processor_context.containing_fx().clone(),
            self.instance_feedback_event_sender.clone(),
        );
        self.pot_unit.loaded(Box::new(integration))
    }

    /// Restores a pot unit state from persistent data.
    ///
    /// This doesn't load the pot unit yet. If the ReaLearn instance never accesses the pot unit,
    /// it simply remains unloaded and its persistent state is kept. The persistent state is also
    /// kept if loading of the pot unit fails (e.g. if the necessary pot database is not available
    /// on the user's computer).
    pub fn restore_pot_unit(&mut self, state: pot::PersistentState) {
        self.pot_unit = PotUnit::unloaded(state);
    }

    /// Returns a pot unit state suitable to be saved by the persistence logic.
    pub fn save_pot_unit(&self) -> pot::PersistentState {
        self.pot_unit.persistent_state()
    }

    pub fn set_mapping_snapshot_container(
        &mut self,
        compartment: Compartment,
        container: MappingSnapshotContainer,
    ) {
        self.mapping_snapshot_container[compartment] = container;
    }

    pub fn mapping_snapshot_container(
        &self,
        compartment: Compartment,
    ) -> &MappingSnapshotContainer {
        &self.mapping_snapshot_container[compartment]
    }

    pub fn mapping_snapshot_container_mut(
        &mut self,
        compartment: Compartment,
    ) -> &mut MappingSnapshotContainer {
        &mut self.mapping_snapshot_container[compartment]
    }

    /// Marks the given snapshot as the active one for all tags in the given scope and sends
    /// instance feedback.
    pub fn mark_snapshot_active(
        &mut self,
        compartment: Compartment,
        tag_scope: &TagScope,
        snapshot_id: &VirtualMappingSnapshotIdForLoad,
    ) {
        self.mapping_snapshot_container[compartment].mark_snapshot_active(tag_scope, snapshot_id);
        self.instance_feedback_event_sender.send_complaining(
            InstanceStateChanged::MappingSnapshotActivated {
                compartment,
                tag_scope: tag_scope.clone(),
                snapshot_id: snapshot_id.clone(),
            },
        )
    }

    pub fn instance_track_descriptor(&self) -> &TrackDescriptor {
        &self.instance_track_descriptor
    }

    pub fn set_instance_track_descriptor(&mut self, descriptor: TrackDescriptor) {
        self.instance_track_descriptor = descriptor;
    }

    pub fn instance_fx_descriptor(&self) -> &FxDescriptor {
        &self.instance_fx_descriptor
    }

    pub fn set_instance_fx_descriptor(&mut self, fx: FxDescriptor) {
        self.instance_fx_descriptor = fx;
    }

    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_relevance(&self, instance_id: InstanceId) -> Option<ClipMatrixRelevance> {
        match self.clip_matrix_ref.as_ref()? {
            ClipMatrixRef::Own(m) if instance_id == self.instance_id => {
                Some(ClipMatrixRelevance::Owns(m))
            }
            ClipMatrixRef::Foreign(id) if instance_id == *id => Some(ClipMatrixRelevance::Borrows),
            _ => None,
        }
    }

    #[cfg(feature = "playtime")]
    pub fn owned_clip_matrix(&self) -> Option<&playtime_clip_engine::base::Matrix> {
        use crate::domain::ClipMatrixRef::*;
        match self.clip_matrix_ref.as_ref()? {
            Own(m) => Some(m),
            Foreign(_) => None,
        }
    }

    #[cfg(feature = "playtime")]
    pub fn owned_clip_matrix_mut(&mut self) -> Option<&mut playtime_clip_engine::base::Matrix> {
        use crate::domain::ClipMatrixRef::*;
        match self.clip_matrix_ref.as_mut()? {
            Own(m) => Some(m),
            Foreign(_) => None,
        }
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_ref(&self) -> Option<&ClipMatrixRef> {
        self.clip_matrix_ref.as_ref()
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_ref_mut(&mut self) -> Option<&mut ClipMatrixRef> {
        self.clip_matrix_ref.as_mut()
    }

    /// Returns `true` if it installed a clip matrix.
    #[cfg(feature = "playtime")]
    pub(super) fn create_and_install_owned_clip_matrix_if_necessary(&mut self) -> bool {
        if matches!(self.clip_matrix_ref.as_ref(), Some(ClipMatrixRef::Own(_))) {
            return false;
        }
        let matrix = self.create_owned_clip_matrix();
        self.update_real_time_clip_matrix(Some(matrix.real_time_matrix()), true);
        self.set_clip_matrix_ref(Some(ClipMatrixRef::Own(Box::new(matrix))));
        self.clip_matrix_event_sender
            .send_complaining(QualifiedClipMatrixEvent {
                instance_id: self.instance_id,
                event: playtime_clip_engine::base::ClipMatrixEvent::EverythingChanged,
            });
        true
    }

    #[cfg(feature = "playtime")]
    fn create_owned_clip_matrix(&self) -> playtime_clip_engine::base::Matrix {
        let clip_matrix_handler = MatrixHandler::new(
            self.instance_id,
            self.audio_hook_task_sender.clone(),
            self.real_time_processor_sender.clone(),
            self.clip_matrix_event_sender.clone(),
        );
        playtime_clip_engine::base::Matrix::new(
            Box::new(clip_matrix_handler),
            self.processor_context.track().cloned(),
        )
    }

    #[cfg(feature = "playtime")]
    pub(super) fn set_clip_matrix_ref(&mut self, matrix_ref: Option<ClipMatrixRef>) {
        if self.clip_matrix_ref.is_some() {
            base::tracing_debug!("Shutdown existing clip matrix or remove reference to clip matrix of other instance");
            self.update_real_time_clip_matrix(None, false);
        }
        self.clip_matrix_ref = matrix_ref;
    }

    #[cfg(feature = "playtime")]
    pub(super) fn update_real_time_clip_matrix(
        &self,
        real_time_matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
        is_owned: bool,
    ) {
        let rt_task = crate::domain::NormalRealTimeTask::SetClipMatrix {
            is_owned,
            matrix: real_time_matrix,
        };
        self.real_time_processor_sender.send_complaining(rt_task);
    }

    pub fn set_mapping_infos(&mut self, mapping_infos: HashMap<QualifiedMappingId, MappingInfo>) {
        self.mapping_infos = mapping_infos;
    }

    pub fn update_mapping_info(&mut self, id: QualifiedMappingId, info: MappingInfo) {
        self.mapping_infos.insert(id, info);
    }

    pub fn get_mapping_info(&self, id: QualifiedMappingId) -> Option<&MappingInfo> {
        self.mapping_infos.get(&id)
    }

    pub fn only_these_mapping_tags_are_active(
        &self,
        compartment: Compartment,
        tags: &HashSet<Tag>,
    ) -> bool {
        tags == &self.active_mapping_tags[compartment]
    }

    pub fn at_least_those_mapping_tags_are_active(
        &self,
        compartment: Compartment,
        tags: &HashSet<Tag>,
    ) -> bool {
        tags.is_subset(&self.active_mapping_tags[compartment])
    }

    pub fn activate_or_deactivate_mapping_tags(
        &mut self,
        compartment: Compartment,
        tags: &HashSet<Tag>,
        activate: bool,
    ) {
        if activate {
            self.active_mapping_tags[compartment].extend(tags.iter().cloned());
        } else {
            self.active_mapping_tags[compartment].retain(|t| !tags.contains(t));
        }
        self.notify_active_mapping_tags_changed(compartment);
    }

    pub fn set_active_mapping_tags(&mut self, compartment: Compartment, tags: HashSet<Tag>) {
        self.active_mapping_tags[compartment] = tags;
        self.notify_active_mapping_tags_changed(compartment);
    }

    fn notify_active_mapping_tags_changed(&mut self, compartment: Compartment) {
        let instance_event = InstanceStateChanged::ActiveMappingTags { compartment };
        self.instance_feedback_event_sender
            .send_complaining(instance_event);
    }

    pub fn only_these_instance_tags_are_active(&self, tags: &HashSet<Tag>) -> bool {
        tags == &self.active_instance_tags
    }

    pub fn at_least_those_instance_tags_are_active(&self, tags: &HashSet<Tag>) -> bool {
        tags.is_subset(&self.active_instance_tags)
    }

    pub fn activate_or_deactivate_instance_tags(&mut self, tags: &HashSet<Tag>, activate: bool) {
        if activate {
            self.active_instance_tags.extend(tags.iter().cloned());
        } else {
            self.active_instance_tags.retain(|t| !tags.contains(t));
        }
        self.notify_active_instance_tags_changed();
    }

    pub fn active_instance_tags(&self) -> &HashSet<Tag> {
        &self.active_instance_tags
    }

    pub fn set_active_instance_tags_without_notification(&mut self, tags: HashSet<Tag>) {
        self.active_instance_tags = tags;
    }

    pub fn set_active_instance_tags(&mut self, tags: HashSet<Tag>) {
        self.active_instance_tags = tags;
        self.notify_active_instance_tags_changed();
    }

    fn notify_active_instance_tags_changed(&mut self) {
        self.instance_feedback_event_sender
            .send_complaining(InstanceStateChanged::ActiveInstanceTags);
    }

    pub fn mapping_is_on(&self, id: QualifiedMappingId) -> bool {
        self.on_mappings.get_ref().contains(&id)
    }

    pub fn global_control_and_feedback_state(&self) -> GlobalControlAndFeedbackState {
        self.global_control_and_feedback_state.get()
    }

    pub fn on_mappings_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.on_mappings.changed()
    }

    pub fn global_control_and_feedback_state_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.global_control_and_feedback_state.changed()
    }

    pub fn set_on_mappings(&mut self, on_mappings: HashSet<QualifiedMappingId>) {
        self.on_mappings.set(on_mappings);
    }

    pub fn set_global_control_and_feedback_state(&mut self, state: GlobalControlAndFeedbackState) {
        self.global_control_and_feedback_state.set(state);
    }

    pub fn set_mapping_on(&mut self, id: QualifiedMappingId, is_on: bool) {
        self.on_mappings.mut_in_place(|m| {
            if is_on {
                m.insert(id);
            } else {
                m.remove(&id);
            }
        });
    }

    pub fn active_mapping_by_group(
        &self,
        compartment: Compartment,
    ) -> &HashMap<GroupId, MappingId> {
        &self.active_mapping_by_group[compartment]
    }

    pub fn active_mapping_tags(&self, compartment: Compartment) -> &HashSet<Tag> {
        &self.active_mapping_tags[compartment]
    }

    pub fn set_active_mapping_by_group(
        &mut self,
        compartment: Compartment,
        value: HashMap<GroupId, MappingId>,
    ) {
        self.active_mapping_by_group[compartment] = value;
    }

    /// Sets the ID of the currently active mapping within the given group.
    pub fn set_active_mapping_within_group(
        &mut self,
        compartment: Compartment,
        group_id: GroupId,
        mapping_id: MappingId,
    ) {
        self.active_mapping_by_group[compartment].insert(group_id, mapping_id);
        let instance_event = InstanceStateChanged::ActiveMappingWithinGroup {
            compartment,
            group_id,
            mapping_id: Some(mapping_id),
        };
        self.instance_feedback_event_sender
            .send_complaining(instance_event);
    }

    /// Gets the ID of the currently active mapping within the given group.
    pub fn get_active_mapping_within_group(
        &self,
        compartment: Compartment,
        group_id: GroupId,
    ) -> Option<MappingId> {
        self.active_mapping_by_group[compartment]
            .get(&group_id)
            .copied()
    }

    pub fn set_mappings_by_group(
        &mut self,
        compartment: Compartment,
        mappings_by_group: HashMap<GroupId, Vec<MappingId>>,
    ) {
        for group_id in self.active_mapping_by_group[compartment].keys() {
            if !mappings_by_group.contains_key(group_id) {
                let event = InstanceStateChanged::ActiveMappingWithinGroup {
                    compartment,
                    group_id: *group_id,
                    mapping_id: None,
                };
                self.instance_feedback_event_sender.send_complaining(event);
            }
        }
        self.mappings_by_group[compartment] = mappings_by_group;
    }

    pub fn get_on_mappings_within_group(
        &self,
        compartment: Compartment,
        group_id: GroupId,
    ) -> impl Iterator<Item = MappingId> + '_ {
        self.mappings_by_group[compartment]
            .get(&group_id)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |id| self.mapping_is_on(QualifiedMappingId::new(compartment, *id)))
    }
}

impl Drop for InstanceState {
    fn drop(&mut self) {
        BackboneState::get().unregister_instance_state(&self.instance_id);
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum InstanceStateChanged {
    /// For the "ReaLearn: Browse group mappings" target.
    ActiveMappingWithinGroup {
        compartment: Compartment,
        group_id: GroupId,
        mapping_id: Option<MappingId>,
    },
    /// For the "ReaLearn: Enable/disable mappings" target.
    ActiveMappingTags {
        compartment: Compartment,
    },
    /// For the "ReaLearn: Enable/disable instances" target.
    ActiveInstanceTags,
    /// For the "ReaLearn: Load mapping snapshot" target.
    MappingSnapshotActivated {
        compartment: Compartment,
        tag_scope: TagScope,
        snapshot_id: VirtualMappingSnapshotIdForLoad,
    },
    PotStateChanged(PotStateChangedEvent),
    MappingWhichLearnsSourceChanged {
        mapping_id: Option<QualifiedMappingId>,
    },
    MappingWhichLearnsTargetChanged {
        mapping_id: Option<QualifiedMappingId>,
    },
}

impl InstanceStateChanged {
    pub fn is_interesting_for_other_instances(&self) -> bool {
        matches!(
            self,
            InstanceStateChanged::MappingWhichLearnsTargetChanged { .. }
        )
    }
}

#[derive(Clone, Debug)]
pub enum PotStateChangedEvent {
    FilterItemChanged {
        kind: PotFilterKind,
        filter: OptFilter,
    },
    PresetChanged {
        id: Option<PresetId>,
    },
    IndexesRebuilt,
    PresetLoaded,
}

#[cfg(feature = "playtime")]
pub enum ClipMatrixRelevance<'a> {
    /// This instance owns the clip matrix with the given ID.
    Owns(&'a playtime_clip_engine::base::Matrix),
    /// This instance borrows the clip matrix with the given ID.
    Borrows,
}

struct RealearnPotIntegration {
    containing_fx: Fx,
    sender: SenderToNormalThread<InstanceStateChanged>,
}

impl RealearnPotIntegration {
    fn new(containing_fx: Fx, sender: SenderToNormalThread<InstanceStateChanged>) -> Self {
        Self {
            containing_fx,
            sender,
        }
    }
}

impl PotIntegration for RealearnPotIntegration {
    fn favorites(&self) -> &RwLock<PotFavorites> {
        &AnyThreadBackboneState::get().pot_favorites
    }

    fn set_current_fx_preset(&self, fx: Fx, preset: CurrentPreset) {
        BackboneState::target_state()
            .borrow_mut()
            .set_current_fx_preset(fx, preset);
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetLoaded,
            ));
    }

    fn exclude_list(&self) -> Ref<PotFilterExcludes> {
        BackboneState::get().pot_filter_exclude_list()
    }

    fn exclude_list_mut(&self) -> RefMut<PotFilterExcludes> {
        BackboneState::get().pot_filter_exclude_list_mut()
    }

    fn notify_preset_changed(&self, id: Option<PresetId>) {
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetChanged { id },
            ));
    }

    fn notify_filter_changed(&self, kind: PotFilterKind, filter: OptFilter) {
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::FilterItemChanged { kind, filter },
            ));
    }

    fn notify_indexes_rebuilt(&self) {
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::IndexesRebuilt,
            ));
    }

    fn protected_fx(&self) -> &Fx {
        &self.containing_fx
    }
}
