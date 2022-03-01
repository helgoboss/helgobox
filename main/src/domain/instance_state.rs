use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use enum_map::EnumMap;
use reaper_high::Track;
use rxrust::prelude::*;

use rx_util::Notifier;

use crate::base::{AsyncNotifier, Prop};
use crate::domain::{
    GroupId, MappingCompartment, MappingId, NormalAudioHookTask, NormalRealTimeTask,
    QualifiedMappingId, RealTimeSender, Tag,
};
use playtime_clip_engine::main::{ClipMatrixHandler, ClipRecordTask, ClipSlotCoordinates, Matrix};
use playtime_clip_engine::rt::ClipChangedEvent;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;

pub type RealearnClipMatrix = Matrix<RealearnClipMatrixHandler>;

/// State connected to the instance which also needs to be accessible from layers *above* the
/// processing layer (otherwise it could reside in the main processor).
#[derive(Debug)]
pub struct InstanceState {
    clip_matrix: Option<RealearnClipMatrix>,
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
    audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
    real_time_processor_sender: RealTimeSender<NormalRealTimeTask>,
    this_track: Option<Track>,
    slot_contents_changed_subject: LocalSubject<'static, (), ()>,
    /// Which mappings are in which group.
    ///
    /// - Used for target "ReaLearn: Navigate within group"
    /// - Automatically filled by main processor on sync
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer,
    ///   but it's very related to the active mapping by group, so we decided to keep it here too.
    mappings_by_group: EnumMap<MappingCompartment, HashMap<GroupId, Vec<MappingId>>>,
    /// Which is the active mapping in which group.
    ///
    /// - Set by target "ReaLearn: Navigate within group".
    /// - Non-redundant state!
    active_mapping_by_group: EnumMap<MappingCompartment, HashMap<GroupId, MappingId>>,
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
    active_mapping_tags: EnumMap<MappingCompartment, HashSet<Tag>>,
    /// All instance tags whose instances have been switched on via tag.
    ///
    /// - Set by target "ReaLearn: Enable/disable instances".
    /// - Non-redundant state!
    active_instance_tags: HashSet<Tag>,
}

#[derive(Debug)]
pub struct RealearnClipMatrixHandler {
    audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
}

impl RealearnClipMatrixHandler {
    fn new(
        audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
    ) -> Self {
        Self {
            audio_hook_task_sender,
            instance_feedback_event_sender,
        }
    }
}

impl ClipMatrixHandler for RealearnClipMatrixHandler {
    fn request_recording_input(&self, task: ClipRecordTask) {
        self.audio_hook_task_sender
            .send(NormalAudioHookTask::StartClipRecording(task))
            .unwrap()
    }

    fn notify_slot_contents_changed(&mut self) {
        self.instance_feedback_event_sender
            .try_send(InstanceStateChanged::AllClips)
            .unwrap();
    }

    fn notify_clip_changed(&self, slot_coordinates: ClipSlotCoordinates, event: ClipChangedEvent) {
        let event = InstanceStateChanged::Clip {
            slot_coordinates,
            event,
        };
        self.instance_feedback_event_sender.try_send(event).unwrap();
    }
}

#[derive(Debug)]
pub struct MappingInfo {
    pub name: String,
}

impl InstanceState {
    pub fn new(
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
        audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
        real_time_processor_sender: RealTimeSender<NormalRealTimeTask>,
        this_track: Option<Track>,
    ) -> Self {
        Self {
            clip_matrix: None,
            instance_feedback_event_sender,
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
        }
    }

    pub fn clip_matrix(&self) -> Option<&RealearnClipMatrix> {
        self.clip_matrix.as_ref()
    }

    pub fn require_clip_matrix_mut(&mut self) -> &mut RealearnClipMatrix {
        self.init_clip_matrix_if_necessary();
        self.clip_matrix
            .as_mut()
            .expect("clip matrix not filled yet")
    }

    pub fn shut_down_clip_matrix(&mut self) {
        tracing_debug!("Shut down clip matrix");
        self.real_time_processor_sender
            .send(NormalRealTimeTask::SetClipMatrix(None))
            .unwrap();
        self.clip_matrix = None;
    }

    fn init_clip_matrix_if_necessary(&mut self) {
        if self.clip_matrix.is_some() {
            return;
        }
        let clip_matrix = init_clip_matrix(
            self.audio_hook_task_sender.clone(),
            self.real_time_processor_sender.clone(),
            self.instance_feedback_event_sender.clone(),
            self.this_track.clone(),
        );
        self.clip_matrix = Some(clip_matrix);
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
        compartment: MappingCompartment,
        tags: &HashSet<Tag>,
    ) -> bool {
        tags == &self.active_mapping_tags[compartment]
    }

    pub fn at_least_those_mapping_tags_are_active(
        &self,
        compartment: MappingCompartment,
        tags: &HashSet<Tag>,
    ) -> bool {
        tags.is_subset(&self.active_mapping_tags[compartment])
    }

    pub fn activate_or_deactivate_mapping_tags(
        &mut self,
        compartment: MappingCompartment,
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

    pub fn set_active_mapping_tags(&mut self, compartment: MappingCompartment, tags: HashSet<Tag>) {
        self.active_mapping_tags[compartment] = tags;
        self.notify_active_mapping_tags_changed(compartment);
    }

    fn notify_active_mapping_tags_changed(&mut self, compartment: MappingCompartment) {
        let instance_event = InstanceStateChanged::ActiveMappingTags { compartment };
        self.instance_feedback_event_sender
            .try_send(instance_event)
            .unwrap();
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
            .try_send(InstanceStateChanged::ActiveInstanceTags)
            .unwrap();
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
        compartment: MappingCompartment,
    ) -> &HashMap<GroupId, MappingId> {
        &self.active_mapping_by_group[compartment]
    }

    pub fn active_mapping_tags(&self, compartment: MappingCompartment) -> &HashSet<Tag> {
        &self.active_mapping_tags[compartment]
    }

    pub fn set_active_mapping_by_group(
        &mut self,
        compartment: MappingCompartment,
        value: HashMap<GroupId, MappingId>,
    ) {
        self.active_mapping_by_group[compartment] = value;
    }

    /// Sets the ID of the currently active mapping within the given group.
    pub fn set_active_mapping_within_group(
        &mut self,
        compartment: MappingCompartment,
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
            .try_send(instance_event)
            .unwrap();
    }

    /// Gets the ID of the currently active mapping within the given group.
    pub fn get_active_mapping_within_group(
        &self,
        compartment: MappingCompartment,
        group_id: GroupId,
    ) -> Option<MappingId> {
        self.active_mapping_by_group[compartment]
            .get(&group_id)
            .copied()
    }

    pub fn set_mappings_by_group(
        &mut self,
        compartment: MappingCompartment,
        mappings_by_group: HashMap<GroupId, Vec<MappingId>>,
    ) {
        for group_id in self.active_mapping_by_group[compartment].keys() {
            if !mappings_by_group.contains_key(group_id) {
                let event = InstanceStateChanged::ActiveMappingWithinGroup {
                    compartment,
                    group_id: *group_id,
                    mapping_id: None,
                };
                self.instance_feedback_event_sender.try_send(event).unwrap();
            }
        }
        self.mappings_by_group[compartment] = mappings_by_group;
    }

    pub fn get_on_mappings_within_group(
        &self,
        compartment: MappingCompartment,
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

#[derive(Debug)]
pub enum InstanceStateChanged {
    Clip {
        slot_coordinates: ClipSlotCoordinates,
        event: ClipChangedEvent,
    },
    AllClips,
    ActiveMappingWithinGroup {
        compartment: MappingCompartment,
        group_id: GroupId,
        mapping_id: Option<MappingId>,
    },
    ActiveMappingTags {
        compartment: MappingCompartment,
    },
    ActiveInstanceTags,
}

fn init_clip_matrix(
    audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
    real_time_processor_sender: RealTimeSender<NormalRealTimeTask>,
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
    this_track: Option<Track>,
) -> RealearnClipMatrix {
    let clip_matrix_handler =
        RealearnClipMatrixHandler::new(audio_hook_task_sender, instance_feedback_event_sender);
    let (matrix, real_time_matrix) = Matrix::new(clip_matrix_handler, this_track);
    real_time_processor_sender
        .send(NormalRealTimeTask::SetClipMatrix(Some(real_time_matrix)))
        .unwrap();
    matrix
}
