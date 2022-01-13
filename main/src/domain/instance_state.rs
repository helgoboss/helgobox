use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::rc::Rc;

use enum_map::EnumMap;
use reaper_high::{Item, Project, Track};
use reaper_medium::{Bpm, PlayState, PositionInSeconds, ReaperVolumeValue};
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};

use helgoboss_learn::UnitValue;
use rx_util::Notifier;

use crate::base::{AsyncNotifier, Prop};
use crate::domain::clip::{
    clip_timeline, Clip, ClipChangedEvent, ClipContent, ClipPlayState, ClipRecordMode,
    ClipRecordTiming, ClipSlot, SlotPlayOptions, SlotStopBehavior,
};
use crate::domain::{
    ClipRecordTask, GroupId, MappingCompartment, MappingId, NormalAudioHookTask,
    QualifiedMappingId, RealTimeSender, Tag, Timeline, TimelineMoment,
};

pub const CLIP_SLOT_COUNT: usize = 8;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;

/// State connected to the instance which also needs to be accessible from layers *above* the
/// processing layer (otherwise it could reside in the main processor).
#[derive(Debug)]
pub struct InstanceState {
    clip_slots: Vec<ClipSlot>,
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
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
    audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
}

#[derive(Debug)]
pub struct MappingInfo {
    pub name: String,
}

impl InstanceState {
    pub fn new(
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
        audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
    ) -> Self {
        Self {
            clip_slots: (0..8).map(ClipSlot::new).collect(),
            instance_feedback_event_sender,
            slot_contents_changed_subject: Default::default(),
            mappings_by_group: Default::default(),
            active_mapping_by_group: Default::default(),
            mapping_infos: Default::default(),
            on_mappings: Default::default(),
            active_mapping_tags: Default::default(),
            active_instance_tags: Default::default(),
            audio_hook_task_sender,
        }
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

    pub fn process_transport_change(
        &mut self,
        new_play_state: PlayState,
        project: Option<Project>,
    ) {
        let timeline = clip_timeline(project);
        let moment = timeline.capture_moment();
        for (slot_index, slot) in self.clip_slots.iter_mut().enumerate() {
            if let Some(event) = slot
                .process_transport_change(new_play_state, moment, &timeline)
                .unwrap()
            {
                let instance_event = InstanceStateChanged::Clip { slot_index, event };
                self.instance_feedback_event_sender
                    .try_send(instance_event)
                    .unwrap();
            }
        }
    }

    pub fn slot_contents_changed(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.slot_contents_changed_subject.clone()
    }

    /// Detects clips that are finished playing and invokes a stop feedback event if not looped.
    pub fn poll_slot(
        &mut self,
        slot_index: usize,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
    ) -> Option<ClipChangedEvent> {
        self.clip_slots
            .get_mut(slot_index)
            .expect("no such slot")
            .poll(timeline_cursor_pos, timeline_tempo)
    }

    pub fn filled_slot_descriptors(&self) -> Vec<QualifiedSlotDescriptor> {
        self.clip_slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_filled())
            .map(|(i, s)| QualifiedSlotDescriptor {
                index: i,
                descriptor: s.descriptor().clone(),
            })
            .collect()
    }

    pub fn load_slots(
        &mut self,
        descriptors: Vec<QualifiedSlotDescriptor>,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        for slot in &mut self.clip_slots {
            let _ = slot.reset();
        }
        for desc in descriptors {
            let events = {
                let slot = self.get_slot_mut(desc.index)?;
                slot.load(desc.descriptor, project)?
            };
            for e in events {
                self.send_clip_changed_event(desc.index, e);
            }
        }
        self.notify_slot_contents_changed();
        Ok(())
    }

    pub fn fill_slot_by_user(
        &mut self,
        slot_index: usize,
        content: ClipContent,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?
            .fill_by_user(content, project)?;
        self.notify_slot_contents_changed();
        Ok(())
    }

    pub fn fill_slot_with_item_source(
        &mut self,
        slot_index: usize,
        item: Item,
    ) -> Result<(), Box<dyn Error>> {
        let slot = self.get_slot_mut(slot_index)?;
        let content = ClipContent::from_item(item)?;
        slot.fill_by_user(content, item.project())?;
        self.notify_slot_contents_changed();
        Ok(())
    }

    pub fn play_clip(
        &mut self,
        project: Project,
        slot_index: usize,
        track: Option<Track>,
        options: SlotPlayOptions,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.play(
            project,
            track,
            options,
            clip_timeline(Some(project)).capture_moment(),
        )
    }

    /// If repeat is not enabled and `immediately` is false, this has essentially no effect.
    pub fn stop_clip(
        &mut self,
        slot_index: usize,
        stop_behavior: SlotStopBehavior,
        project: Project,
    ) -> Result<(), &'static str> {
        if self.get_slot_mut(slot_index)?.play_state() == ClipPlayState::Recording {
            let task = NormalAudioHookTask::StopClipRecording;
            self.audio_hook_task_sender.send(task).unwrap();
        }
        self.get_slot_mut(slot_index)?
            .stop(stop_behavior, clip_timeline(Some(project)).capture_moment())
    }

    pub fn record_clip(
        &mut self,
        slot_index: usize,
        timing: ClipRecordTiming,
        mode: ClipRecordMode,
        project: Project,
    ) -> Result<(), &'static str> {
        let register = self.get_slot_mut(slot_index)?.record()?;
        let task = ClipRecordTask {
            abs_start_pos: clip_timeline(Some(project)).cursor_pos(),
            register,
            timing,
            mode,
            project,
        };
        self.audio_hook_task_sender
            .send(NormalAudioHookTask::StartClipRecording(task))
            .map_err(|_| "couldn't send record task")
    }

    pub fn pause_clip(&mut self, slot_index: usize) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.pause()
    }

    pub fn toggle_repeat(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.toggle_repeat();
        self.send_clip_changed_event(slot_index, event);
        Ok(())
    }

    pub fn seek_slot(
        &mut self,
        slot_index: usize,
        position: UnitValue,
    ) -> Result<(), &'static str> {
        let event = self
            .get_slot_mut(slot_index)?
            .set_proportional_position(position)?;
        if let Some(event) = event {
            self.send_clip_changed_event(slot_index, event);
        }
        Ok(())
    }

    pub fn set_volume(
        &mut self,
        slot_index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.set_volume(volume);
        self.send_clip_changed_event(slot_index, event);
        Ok(())
    }

    pub fn set_clip_tempo_factor(
        &mut self,
        slot_index: usize,
        tempo_factor: f64,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?
            .set_tempo_factor(tempo_factor);
        Ok(())
    }

    pub fn get_slot(&self, slot_index: usize) -> Result<&ClipSlot, &'static str> {
        self.clip_slots.get(slot_index).ok_or("no such slot")
    }

    fn get_slot_mut(&mut self, slot_index: usize) -> Result<&mut ClipSlot, &'static str> {
        self.clip_slots.get_mut(slot_index).ok_or("no such slot")
    }

    fn send_clip_changed_event(&self, slot_index: usize, event: ClipChangedEvent) {
        self.send_feedback_event(InstanceStateChanged::Clip { slot_index, event });
    }

    fn send_feedback_event(&self, event: InstanceStateChanged) {
        self.instance_feedback_event_sender.try_send(event).unwrap();
    }

    fn notify_slot_contents_changed(&mut self) {
        AsyncNotifier::notify(&mut self.slot_contents_changed_subject, &());
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QualifiedSlotDescriptor {
    #[serde(rename = "index")]
    pub index: usize,
    #[serde(flatten)]
    pub descriptor: Clip,
}

#[derive(Debug)]
pub enum InstanceStateChanged {
    Clip {
        slot_index: usize,
        event: ClipChangedEvent,
    },
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
