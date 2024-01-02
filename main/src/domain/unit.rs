use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use enum_map::EnumMap;
use rxrust::prelude::*;

use crate::base::Prop;
use crate::domain::{
    Compartment, FxDescriptor, GlobalControlAndFeedbackState, GroupId, MappingId,
    MappingSnapshotContainer, ParameterManager, QualifiedMappingId, SharedInstance, Tag, TagScope,
    TrackDescriptor, UnitId, VirtualMappingSnapshotIdForLoad, WeakInstance,
};
use base::{NamedChannelSender, SenderToNormalThread};

pub type SharedUnit = Rc<RefCell<Unit>>;

/// Just the old term as alias for easier class search.
type _InstanceState = Unit;

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
pub struct Unit {
    id: UnitId,
    parent_instance: WeakInstance,
    feedback_event_sender: SenderToNormalThread<UnitStateChanged>,
    /// Which mappings are in which group.
    ///
    /// - Not persistent
    /// - Used for target "ReaLearn: Browse group mappings"
    /// - Automatically filled by main processor on sync
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer,
    ///   but it's very related to the active mapping by group, so we decided to keep it here too.
    mappings_by_group: EnumMap<Compartment, HashMap<GroupId, Vec<MappingId>>>,
    /// Which is the active mapping in which group.
    ///
    /// - Persistent
    /// - Set by target "ReaLearn: Browse group mappings".
    /// - Non-redundant state!
    active_mapping_by_group: EnumMap<Compartment, HashMap<GroupId, MappingId>>,
    /// Additional info about mappings.
    ///
    /// - Not persistent
    /// - Completely derived from mappings, so it's redundant state.
    /// - Could be kept in main processor because it's only accessed by the processing layer.
    mapping_infos: HashMap<QualifiedMappingId, MappingInfo>,
    /// The mappings which are on.
    ///
    /// - Not persistent
    /// - "on" = enabled & control or feedback enabled & mapping active & target active
    /// - Completely derived from mappings, so it's redundant state.
    /// - It's needed by both processing layer and layers above.
    on_mappings: Prop<HashSet<QualifiedMappingId>>,
    /// Whether control/feedback are globally active.
    ///
    /// Not persistent.
    global_control_and_feedback_state: Prop<GlobalControlAndFeedbackState>,
    /// All mapping tags whose mappings have been switched on via tag.
    ///
    /// - Persistent
    /// - Set by target "ReaLearn: Enable/disable mappings".
    /// - Non-redundant state!
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
    mapping_snapshot_container: EnumMap<Compartment, MappingSnapshotContainer>,
    mapping_which_learns_source: Prop<Option<QualifiedMappingId>>,
    mapping_which_learns_target: Prop<Option<QualifiedMappingId>>,
    parameter_manager: Arc<ParameterManager>,
}

#[derive(Debug)]
pub struct MappingInfo {
    pub name: String,
}

impl Unit {
    pub(crate) fn new(
        id: UnitId,
        parent_instance: WeakInstance,
        feedback_event_sender: SenderToNormalThread<UnitStateChanged>,
        parameter_manager: ParameterManager,
    ) -> Self {
        Self {
            id,
            parent_instance,
            feedback_event_sender,
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
            mapping_which_learns_source: Default::default(),
            mapping_which_learns_target: Default::default(),
            parameter_manager: Arc::new(parameter_manager),
        }
    }

    pub fn parameter_manager(&self) -> &Arc<ParameterManager> {
        &self.parameter_manager
    }

    pub fn set_mapping_which_learns_source(
        &mut self,
        mapping_id: Option<QualifiedMappingId>,
    ) -> Option<QualifiedMappingId> {
        let previous_value = self.mapping_which_learns_source.replace(mapping_id);
        self.parent_instance()
            .borrow()
            .notify_learning_target_in_unit_changed(self.id);
        self.feedback_event_sender
            .send_complaining(UnitStateChanged::MappingWhichLearnsSourceChanged { mapping_id });
        previous_value
    }

    fn parent_instance(&self) -> SharedInstance {
        self.parent_instance
            .upgrade()
            .expect("parent instance gone")
    }

    pub fn set_mapping_which_learns_target(
        &mut self,
        mapping_id: Option<QualifiedMappingId>,
    ) -> Option<QualifiedMappingId> {
        let previous_value = self.mapping_which_learns_target.replace(mapping_id);
        self.feedback_event_sender
            .send_complaining(UnitStateChanged::MappingWhichLearnsTargetChanged { mapping_id });
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
        self.feedback_event_sender
            .send_complaining(UnitStateChanged::MappingSnapshotActivated {
                compartment,
                tag_scope: tag_scope.clone(),
                snapshot_id: snapshot_id.clone(),
            })
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

    pub fn id(&self) -> UnitId {
        self.id
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
        let instance_event = UnitStateChanged::ActiveMappingTags { compartment };
        self.feedback_event_sender.send_complaining(instance_event);
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
        self.feedback_event_sender
            .send_complaining(UnitStateChanged::ActiveInstanceTags);
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
        let instance_event = UnitStateChanged::ActiveMappingWithinGroup {
            compartment,
            group_id,
            mapping_id: Some(mapping_id),
        };
        self.feedback_event_sender.send_complaining(instance_event);
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
                let event = UnitStateChanged::ActiveMappingWithinGroup {
                    compartment,
                    group_id: *group_id,
                    mapping_id: None,
                };
                self.feedback_event_sender.send_complaining(event);
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

#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum UnitStateChanged {
    /// For the "ReaLearn: Browse group mappings" target.
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
    MappingWhichLearnsSourceChanged {
        mapping_id: Option<QualifiedMappingId>,
    },
    MappingWhichLearnsTargetChanged {
        mapping_id: Option<QualifiedMappingId>,
    },
}

impl UnitStateChanged {
    pub fn is_interesting_for_other_units(&self) -> bool {
        matches!(
            self,
            UnitStateChanged::MappingWhichLearnsTargetChanged { .. }
        )
    }
}
