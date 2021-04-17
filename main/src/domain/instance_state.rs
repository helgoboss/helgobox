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

pub const PREVIEW_SLOT_COUNT: usize = 20;

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

    pub fn get_slot_mut(&mut self, slot_index: usize) -> Result<&mut PreviewSlot, &'static str> {
        self.preview_slots.get_mut(slot_index).ok_or("no such slot")
    }
}
