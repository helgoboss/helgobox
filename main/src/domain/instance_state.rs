use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use enum_map::EnumMap;
use reaper_high::Track;
use rxrust::prelude::*;

use crate::base::{NamedChannelSender, Prop, SenderToNormalThread, SenderToRealTimeThread};
use crate::domain::{
    BackboneState, Compartment, FxDescriptor, FxInputClipRecordTask, GroupId,
    HardwareInputClipRecordTask, InstanceId, MappingId, MappingSnapshotContainer,
    NormalAudioHookTask, NormalRealTimeTask, QualifiedMappingId, Tag, TagScope, TrackDescriptor,
    VirtualMappingSnapshotIdForLoad,
};
use playtime_clip_engine::main::{
    ApiClipWithColumn, ClipMatrixEvent, ClipMatrixHandler, ClipRecordInput, ClipRecordTask, Matrix,
};
use playtime_clip_engine::rt;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;
pub type WeakInstanceState = Weak<RefCell<InstanceState>>;

pub type RealearnClipMatrix = Matrix<RealearnClipMatrixHandler>;

/// State connected to the instance which also needs to be accessible from layers *above* the
/// processing layer (otherwise it could reside in the main processor).
#[derive(Debug)]
pub struct InstanceState {
    instance_id: InstanceId,
    clip_matrix_ref: Option<ClipMatrixRef>,
    instance_feedback_event_sender: SenderToNormalThread<InstanceStateChanged>,
    clip_matrix_event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
    audio_hook_task_sender: SenderToRealTimeThread<NormalAudioHookTask>,
    real_time_processor_sender: SenderToRealTimeThread<NormalRealTimeTask>,
    this_track: Option<Track>,
    slot_contents_changed_subject: LocalSubject<'static, (), ()>,
    /// Which mappings are in which group.
    ///
    /// - Used for target "ReaLearn: Navigate within group"
    /// - Automatically filled by main processor on sync
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer,
    ///   but it's very related to the active mapping by group, so we decided to keep it here too.
    mappings_by_group: EnumMap<Compartment, HashMap<GroupId, Vec<MappingId>>>,
    /// Which is the active mapping in which group.
    ///
    /// - Set by target "ReaLearn: Navigate within group".
    /// - Non-redundant state!
    active_mapping_by_group: EnumMap<Compartment, HashMap<GroupId, MappingId>>,
    /// Additional info about mappings.
    ///
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer.
    mapping_infos: HashMap<QualifiedMappingId, MappingInfo>,
    /// The mappings which are on.
    ///
    /// - "on" = enabled & control or feedback enabled & mapping active & target active
    /// - Completely derived from mappings, so it's redundant state.
    /// - It's needed by both processing layer and layers above.
    on_mappings: Prop<HashSet<QualifiedMappingId>>,
    /// All mapping tags whose mappings have been switched on via tag.
    ///
    /// - Set by target "ReaLearn: Enable/disable mappings".
    /// - Non-redundant state!
    active_mapping_tags: EnumMap<Compartment, HashSet<Tag>>,
    /// All instance tags whose instances have been switched on via tag.
    ///
    /// - Set by target "ReaLearn: Enable/disable instances".
    /// - Non-redundant state!
    active_instance_tags: HashSet<Tag>,
    copied_clip: Option<playtime_api::persistence::Clip>,
    copied_clips_in_row: Vec<ApiClipWithColumn>,
    instance_track_descriptor: TrackDescriptor,
    // TODO-low It's not so cool that we have the target activation condition as part of the
    //  descriptors (e.g. enable_only_if_track_selected) because we must be sure to set them
    //  to false.
    //  We should probably use the following distinction instead:
    //  - TrackTargetDescriptor contains TrackDescriptor contains VirtualTrack
    //  - FxTargetDescriptor contains FxDescriptor contains VirtualFx
    //  ... where TrackTargetDescriptor contains the condition and TrackDescriptor doesn't.
    instance_fx_descriptor: FxDescriptor,
    mapping_snapshot_container: EnumMap<Compartment, MappingSnapshotContainer>,
}

#[derive(Debug)]
pub enum ClipMatrixRef {
    Own(Box<RealearnClipMatrix>),
    Foreign(InstanceId),
}

#[derive(Debug)]
pub struct RealearnClipMatrixHandler {
    instance_id: InstanceId,
    audio_hook_task_sender: SenderToRealTimeThread<NormalAudioHookTask>,
    real_time_processor_sender: SenderToRealTimeThread<NormalRealTimeTask>,
    event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
}

#[derive(Debug)]
pub struct QualifiedClipMatrixEvent {
    pub instance_id: InstanceId,
    pub event: ClipMatrixEvent,
}

impl RealearnClipMatrixHandler {
    fn new(
        instance_id: InstanceId,
        audio_hook_task_sender: SenderToRealTimeThread<NormalAudioHookTask>,
        real_time_processor_sender: SenderToRealTimeThread<NormalRealTimeTask>,
        event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
    ) -> Self {
        Self {
            instance_id,
            audio_hook_task_sender,
            real_time_processor_sender,
            event_sender,
        }
    }
}

impl ClipMatrixHandler for RealearnClipMatrixHandler {
    fn request_recording_input(&self, task: ClipRecordTask) {
        match task.input {
            ClipRecordInput::HardwareInput(input) => {
                let hw_task = HardwareInputClipRecordTask {
                    input,
                    destination: task.destination,
                };
                self.audio_hook_task_sender
                    .send_complaining(NormalAudioHookTask::StartClipRecording(hw_task));
            }
            ClipRecordInput::FxInput(input) => {
                let fx_task = FxInputClipRecordTask {
                    input,
                    destination: task.destination,
                };
                self.real_time_processor_sender
                    .send_complaining(NormalRealTimeTask::StartClipRecording(fx_task));
            }
        }
    }

    fn emit_event(&self, event: ClipMatrixEvent) {
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
        instance_feedback_event_sender: SenderToNormalThread<InstanceStateChanged>,
        clip_matrix_event_sender: SenderToNormalThread<QualifiedClipMatrixEvent>,
        audio_hook_task_sender: SenderToRealTimeThread<NormalAudioHookTask>,
        real_time_processor_sender: SenderToRealTimeThread<NormalRealTimeTask>,
        this_track: Option<Track>,
    ) -> Self {
        Self {
            instance_id,
            clip_matrix_ref: None,
            instance_feedback_event_sender,
            clip_matrix_event_sender,
            audio_hook_task_sender,
            real_time_processor_sender,
            this_track,
            slot_contents_changed_subject: Default::default(),
            mappings_by_group: Default::default(),
            active_mapping_by_group: Default::default(),
            mapping_infos: Default::default(),
            on_mappings: Default::default(),
            active_mapping_tags: Default::default(),
            active_instance_tags: Default::default(),
            copied_clip: None,
            copied_clips_in_row: vec![],
            instance_track_descriptor: Default::default(),
            instance_fx_descriptor: Default::default(),
            mapping_snapshot_container: Default::default(),
        }
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

    pub fn is_interested_in_clip_matrix_events_from(&self, instance_id: InstanceId) -> bool {
        use ClipMatrixRef::*;
        let our_instance_id = match self.clip_matrix_ref {
            None => return false,
            Some(Own(_)) => self.instance_id,
            Some(Foreign(id)) => id,
        };
        instance_id == our_instance_id
    }

    pub fn copy_clip(&mut self, clip: playtime_api::persistence::Clip) {
        self.copied_clip = Some(clip);
    }

    pub fn copied_clip(&self) -> Option<&playtime_api::persistence::Clip> {
        self.copied_clip.as_ref()
    }

    pub fn copy_clips_in_row(&mut self, clips: Vec<ApiClipWithColumn>) {
        self.copied_clips_in_row = clips;
    }

    pub fn copied_clips_in_row(&self) -> &[ApiClipWithColumn] {
        &self.copied_clips_in_row
    }

    pub fn owned_clip_matrix(&self) -> Option<&RealearnClipMatrix> {
        use ClipMatrixRef::*;
        match self.clip_matrix_ref.as_ref()? {
            Own(m) => Some(m),
            Foreign(_) => None,
        }
    }

    pub fn owned_clip_matrix_mut(&mut self) -> Option<&mut RealearnClipMatrix> {
        use ClipMatrixRef::*;
        match self.clip_matrix_ref.as_mut()? {
            Own(m) => Some(m),
            Foreign(_) => None,
        }
    }

    pub fn clip_matrix_ref(&self) -> Option<&ClipMatrixRef> {
        self.clip_matrix_ref.as_ref()
    }

    pub fn clip_matrix_ref_mut(&mut self) -> Option<&mut ClipMatrixRef> {
        self.clip_matrix_ref.as_mut()
    }

    /// Returns `true` if it installed a clip matrix.
    pub(super) fn create_and_install_owned_clip_matrix_if_necessary(&mut self) -> bool {
        if matches!(self.clip_matrix_ref.as_ref(), Some(ClipMatrixRef::Own(_))) {
            return false;
        }
        let matrix = self.create_owned_clip_matrix();
        self.update_real_time_clip_matrix(Some(matrix.real_time_matrix()), true);
        self.set_clip_matrix_ref(Some(ClipMatrixRef::Own(Box::new(matrix))));
        true
    }

    fn create_owned_clip_matrix(&self) -> RealearnClipMatrix {
        let clip_matrix_handler = RealearnClipMatrixHandler::new(
            self.instance_id,
            self.audio_hook_task_sender.clone(),
            self.real_time_processor_sender.clone(),
            self.clip_matrix_event_sender.clone(),
        );
        Matrix::new(clip_matrix_handler, self.this_track.clone())
    }

    pub(super) fn set_clip_matrix_ref(&mut self, matrix_ref: Option<ClipMatrixRef>) {
        if self.clip_matrix_ref.is_some() {
            tracing_debug!("Shutdown existing clip matrix or remove reference to clip matrix of other instance");
            self.update_real_time_clip_matrix(None, false);
        }
        self.clip_matrix_ref = matrix_ref;
    }

    pub(super) fn update_real_time_clip_matrix(
        &self,
        real_time_matrix: Option<rt::WeakMatrix>,
        is_owned: bool,
    ) {
        let rt_task = NormalRealTimeTask::SetClipMatrix {
            is_owned,
            matrix: real_time_matrix,
        };
        self.real_time_processor_sender.send_complaining(rt_task);
    }

    pub fn slot_contents_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.slot_contents_changed_subject.clone()
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

    pub fn on_mappings_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.on_mappings.changed()
    }

    pub fn set_on_mappings(&mut self, on_mappings: HashSet<QualifiedMappingId>) {
        self.on_mappings.set(on_mappings);
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

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum InstanceStateChanged {
    /// For the "ReaLearn: Navigate within group" target.
    ActiveMappingWithinGroup {
        compartment: Compartment,
        group_id: GroupId,
        mapping_id: Option<MappingId>,
    },
    /// For the "ReaLearn: Enable/disable mappings" target.
    ActiveMappingTags { compartment: Compartment },
    /// For the "ReaLearn: Enable/disable instances" target.
    ActiveInstanceTags,
    /// For the "ReaLearn: Load mapping snapshot" target.
    MappingSnapshotActivated {
        compartment: Compartment,
        tag_scope: TagScope,
        snapshot_id: VirtualMappingSnapshotIdForLoad,
    },
}
