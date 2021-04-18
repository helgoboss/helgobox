use crate::domain::{
    ClipPlayState, ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput,
    PreviewSlot, RealearnTargetContext, ReaperTarget, SlotPlayOptions,
};
use reaper_high::{Item, Reaper, Track};
use reaper_medium::{MediaItem, ReaperVolumeValue};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const PREVIEW_SLOT_COUNT: usize = 20;

pub type SharedInstanceState = Rc<RefCell<InstanceState>>;

#[derive(Debug)]
pub struct InstanceState {
    preview_slots: [PreviewSlot; PREVIEW_SLOT_COUNT],
    instance_feedback_event_sender: crossbeam_channel::Sender<InstanceFeedbackEvent>,
}

impl InstanceState {
    pub fn new(
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceFeedbackEvent>,
    ) -> Self {
        Self {
            preview_slots: Default::default(),
            instance_feedback_event_sender,
        }
    }

    pub fn fill_preview_slot_with_file(
        &mut self,
        slot_index: usize,
        file: &Path,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?
            .fill_with_source_from_file(file)
    }

    pub fn fill_preview_slot_with_item_source(
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
        self.get_slot_mut(slot_index)?.play(track, options)?;
        self.send_clip_play_state_feedback_event(slot_index);
        Ok(())
    }

    pub fn stop(&mut self, slot_index: usize) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.stop()?;
        self.send_clip_play_state_feedback_event(slot_index);
        Ok(())
    }

    pub fn pause(&mut self, slot_index: usize) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.pause()?;
        self.send_clip_play_state_feedback_event(slot_index);
        Ok(())
    }

    pub fn toggle_looped(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let new_value = self.get_slot_mut(slot_index)?.toggle_looped()?;
        self.send_feedback_event(InstanceFeedbackEvent::ClipRepeatChanged(
            ClipRepeatChangedEvent {
                slot_index,
                new_value,
            },
        ));
        Ok(())
    }

    pub fn set_volume(
        &mut self,
        slot_index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.set_volume(volume)?;
        self.send_feedback_event(InstanceFeedbackEvent::ClipVolumeChanged(
            ClipVolumeChangedEvent {
                slot_index,
                new_value: volume,
            },
        ));
        Ok(())
    }

    pub fn get_play_state(&self, slot_index: usize) -> Result<ClipPlayState, &'static str> {
        Ok(self.get_slot(slot_index)?.play_state())
    }

    pub fn get_volume(&self, slot_index: usize) -> Result<ReaperVolumeValue, &'static str> {
        self.get_slot(slot_index)?.volume()
    }

    pub fn get_is_looped(&self, slot_index: usize) -> Result<bool, &'static str> {
        self.get_slot(slot_index)?.is_looped()
    }

    fn get_slot(&self, slot_index: usize) -> Result<&PreviewSlot, &'static str> {
        self.preview_slots.get(slot_index).ok_or("no such slot")
    }

    fn get_slot_mut(&mut self, slot_index: usize) -> Result<&mut PreviewSlot, &'static str> {
        self.preview_slots.get_mut(slot_index).ok_or("no such slot")
    }

    fn send_clip_play_state_feedback_event(&self, slot_index: usize) {
        self.send_feedback_event(InstanceFeedbackEvent::ClipPlayStateChanged(
            ClipPlayStateChangedEvent {
                slot_index,
                new_value: self
                    .preview_slots
                    .get(slot_index)
                    .expect("impossible")
                    .play_state(),
            },
        ));
    }

    fn send_feedback_event(&self, event: InstanceFeedbackEvent) {
        self.instance_feedback_event_sender.send(event).unwrap();
    }
}

#[derive(Debug)]
pub enum InstanceFeedbackEvent {
    ClipPlayStateChanged(ClipPlayStateChangedEvent),
    ClipVolumeChanged(ClipVolumeChangedEvent),
    ClipRepeatChanged(ClipRepeatChangedEvent),
}

#[derive(Debug)]
pub struct ClipPlayStateChangedEvent {
    pub slot_index: usize,
    pub new_value: ClipPlayState,
}

#[derive(Debug)]
pub struct ClipVolumeChangedEvent {
    pub slot_index: usize,
    pub new_value: ReaperVolumeValue,
}

#[derive(Debug)]
pub struct ClipRepeatChangedEvent {
    pub slot_index: usize,
    pub new_value: bool,
}
