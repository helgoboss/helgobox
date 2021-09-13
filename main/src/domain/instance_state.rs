use crate::base::AsyncNotifier;
use crate::domain::{
    ClipPlayState, ClipSlot, GroupId, MappingId, SlotContent, SlotDescriptor, SlotPlayOptions,
};
use helgoboss_learn::UnitValue;
use reaper_high::{Item, Project, Track};
use reaper_medium::{PlayState, ReaperVolumeValue};
use rx_util::Notifier;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

pub const CLIP_SLOT_COUNT: usize = 8;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;

#[derive(Debug)]
pub struct InstanceState {
    clip_slots: [ClipSlot; CLIP_SLOT_COUNT],
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceFeedbackEvent>,
    slot_contents_changed_subject: LocalSubject<'static, (), ()>,
    active_mapping_by_group: HashMap<GroupId, MappingId>,
}

impl InstanceState {
    pub fn new(
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceFeedbackEvent>,
    ) -> Self {
        Self {
            clip_slots: Default::default(),
            instance_feedback_event_sender,
            slot_contents_changed_subject: Default::default(),
            active_mapping_by_group: Default::default(),
        }
    }

    /// Sets the ID of the currently active mapping within the given group.
    pub fn set_active_mapping_within_group(&mut self, group_id: GroupId, mapping_id: MappingId) {
        self.active_mapping_by_group.insert(group_id, mapping_id);
        let instance_event = InstanceFeedbackEvent::ActiveMappingWithinGroupChanged {
            group_id,
            mapping_id: Some(mapping_id),
        };
        self.instance_feedback_event_sender
            .try_send(instance_event)
            .unwrap();
    }

    /// Gets the ID of the currently active mapping within the given group.
    pub fn get_active_mapping_within_group(&self, group_id: GroupId) -> Option<MappingId> {
        self.active_mapping_by_group.get(&group_id).copied()
    }

    pub fn process_transport_change(&mut self, new_play_state: PlayState) {
        for (slot_index, slot) in self.clip_slots.iter_mut().enumerate() {
            if let Ok(Some(event)) = slot.process_transport_change(new_play_state) {
                let instance_event = InstanceFeedbackEvent::ClipChanged { slot_index, event };
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
    pub fn poll_slot(&mut self, slot_index: usize) -> Option<ClipChangedEvent> {
        self.clip_slots
            .get_mut(slot_index)
            .expect("no such slot")
            .poll()
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
        content: SlotContent,
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
        self.get_slot_mut(slot_index)?
            .fill_with_source_from_item(item)?;
        self.notify_slot_contents_changed();
        Ok(())
    }

    pub fn play(
        &mut self,
        slot_index: usize,
        track: Option<Track>,
        options: SlotPlayOptions,
    ) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.play(track, options)?;
        self.send_clip_changed_event(slot_index, event);
        Ok(())
    }

    /// If repeat is not enabled and `immediately` is false, this has essentially no effect.
    pub fn stop(&mut self, slot_index: usize, immediately: bool) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.stop(immediately)?;
        self.send_clip_changed_event(slot_index, event);
        Ok(())
    }

    pub fn pause(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.pause()?;
        self.send_clip_changed_event(slot_index, event);
        Ok(())
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
        let event = self.get_slot_mut(slot_index)?.set_position(position)?;
        self.send_clip_changed_event(slot_index, event);
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

    pub fn get_slot(&self, slot_index: usize) -> Result<&ClipSlot, &'static str> {
        self.clip_slots.get(slot_index).ok_or("no such slot")
    }

    fn get_slot_mut(&mut self, slot_index: usize) -> Result<&mut ClipSlot, &'static str> {
        self.clip_slots.get_mut(slot_index).ok_or("no such slot")
    }

    fn send_clip_changed_event(&self, slot_index: usize, event: ClipChangedEvent) {
        self.send_feedback_event(InstanceFeedbackEvent::ClipChanged { slot_index, event });
    }

    fn send_feedback_event(&self, event: InstanceFeedbackEvent) {
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
    pub descriptor: SlotDescriptor,
}

#[derive(Debug)]
pub enum InstanceFeedbackEvent {
    ClipChanged {
        slot_index: usize,
        event: ClipChangedEvent,
    },
    ActiveMappingWithinGroupChanged {
        group_id: GroupId,
        mapping_id: Option<MappingId>,
    },
}

#[derive(Debug)]
pub enum ClipChangedEvent {
    PlayState(ClipPlayState),
    ClipVolume(ReaperVolumeValue),
    ClipRepeat(bool),
    ClipPosition(UnitValue),
}
