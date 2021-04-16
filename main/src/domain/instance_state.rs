use crate::domain::{
    ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput, PreviewSlot,
    RealearnTargetContext, ReaperTarget,
};
use reaper_high::{Item, Reaper, Track};
use reaper_medium::MediaItem;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};

const PREVIEW_SLOT_COUNT: usize = 20;

#[derive(Debug, Default)]
pub struct InstanceState {
    preview_slots: [PreviewSlot; PREVIEW_SLOT_COUNT],
}

impl InstanceState {
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

    fn get_slot_mut(&mut self, slot_index: usize) -> Result<&mut PreviewSlot, &'static str> {
        self.preview_slots.get_mut(slot_index).ok_or("no such slot")
    }

    pub fn play_preview(
        &mut self,
        slot_index: usize,
        track: Option<&Track>,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.play(track)
    }

    pub fn preview_slot_is_filled(&self, slot_index: usize) -> bool {
        if let Some(slot) = self.preview_slots.get(slot_index) {
            slot.is_filled()
        } else {
            false
        }
    }
}
