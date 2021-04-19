use crate::domain::{
    ClipPlayState, ClipSlot, ControlInput, DeviceControlInput, DeviceFeedbackOutput,
    FeedbackOutput, RealearnTargetContext, ReaperTarget, SlotContent, SlotDescriptor,
    SlotPlayOptions,
};
use reaper_high::{Item, Project, Reaper, Track};
use reaper_medium::{MediaItem, PositionInSeconds, ReaperVolumeValue};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const CLIP_SLOT_COUNT: usize = 8;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;

#[derive(Debug)]
pub struct InstanceState {
    clip_slots: [ClipSlot; CLIP_SLOT_COUNT],
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceFeedbackEvent>,
}

impl InstanceState {
    pub fn new(
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceFeedbackEvent>,
    ) -> Self {
        Self {
            clip_slots: Default::default(),
            instance_feedback_event_sender,
        }
    }

    /// Detects clips that are finished playing and invokes a stop feedback event if not looped.
    pub fn poll_slot(&mut self, slot_index: usize) -> Option<ClipChangedEvent> {
        self.clip_slots.get_mut(slot_index)?.poll()
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
                let mut slot = self.get_slot_mut(desc.index)?;
                slot.load(desc.descriptor, project)?
            };
            for e in events {
                self.send_clip_changed_event(desc.index, e);
            }
        }
        Ok(())
    }

    pub fn fill_slot(
        &mut self,
        slot_index: usize,
        content: SlotContent,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.fill(content, project)
    }

    pub fn fill_slot_with_item_source(
        &mut self,
        slot_index: usize,
        item: Item,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?
            .fill_with_source_from_item(item)
    }

    pub fn play(
        &mut self,
        slot_index: usize,
        track: Option<&Track>,
        options: SlotPlayOptions,
    ) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.play(track, options)?;
        self.send_clip_changed_event(slot_index, event);
        Ok(())
    }

    pub fn stop(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.stop()?;
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
        self.instance_feedback_event_sender.send(event).unwrap();
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
}

#[derive(Debug)]
pub enum ClipChangedEvent {
    PlayStateChanged(ClipPlayState),
    ClipVolumeChanged(ReaperVolumeValue),
    ClipRepeatedChanged(bool),
    ClipPositionChanged(PositionInSeconds),
}
