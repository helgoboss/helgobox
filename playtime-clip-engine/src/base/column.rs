use crate::base::{Clip, ClipMatrixHandler, MatrixSettings, RelevantContent, Slot};
use crate::rt::supplier::{ChainEquipment, RecorderRequest};
use crate::rt::{
    ClipChangeEvent, ColumnCommandSender, ColumnHandle, ColumnLoadArgs, ColumnMoveSlotContentsArgs,
    ColumnPlayRowArgs, ColumnPlaySlotArgs, ColumnReorderSlotsArgs, ColumnStopArgs,
    ColumnStopSlotArgs, FillSlotMode, OverridableMatrixSettings, RtColumnEvent, RtSlotId, RtSlots,
    SharedRtColumn, SlotChangeEvent,
};
use crate::{rt, source_util, ClipEngineResult};
use crossbeam_channel::{Receiver, Sender};
use either::Either;
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use indexmap::IndexMap;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    preferred_clip_midi_settings, BeatTimeBase, ClipAudioSettings, ClipColor, ClipTimeBase,
    ColumnClipPlayAudioSettings, ColumnClipPlaySettings, ColumnClipRecordSettings, ColumnId,
    ColumnPlayMode, Db, MatrixClipRecordSettings, PositiveBeat, PositiveSecond, Section, SlotId,
    TimeSignature, TrackId,
};
use reaper_high::{Guid, OrCurrentProject, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, Bpm, CustomPcmSource, FlexibleOwnedPcmSource, HelpMode,
    MeasureAlignment, OwnedPreviewRegister, ReaperMutex, ReaperVolumeValue,
};
use std::collections::HashMap;
use std::iter;
use std::ptr::NonNull;
use std::sync::Arc;
use xxhash_rust::xxh3::Xxh3Builder;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    id: ColumnId,
    settings: ColumnSettings,
    rt_settings: rt::RtColumnSettings,
    rt_command_sender: ColumnCommandSender,
    rt_column: SharedRtColumn,
    preview_register: Option<PlayingPreviewRegister>,
    slots: Slots,
    event_receiver: Receiver<RtColumnEvent>,
    project: Option<Project>,
}

type Slots = IndexMap<RtSlotId, Slot, Xxh3Builder>;

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    pub clip_record_settings: ColumnClipRecordSettings,
}

impl ColumnSettings {
    pub fn from_api(api_column: &api::Column) -> Self {
        Self {
            clip_record_settings: api_column.clip_record_settings.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    _preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
    track: Option<Track>,
}

impl Column {
    pub fn new(id: ColumnId, permanent_project: Option<Project>) -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(500);
        let (event_sender, event_receiver) = crossbeam_channel::bounded(500);
        let source = rt::RtColumn::new(permanent_project, command_receiver, event_sender);
        let shared_source = SharedRtColumn::new(source);
        Self {
            id,
            settings: Default::default(),
            rt_settings: Default::default(),
            // preview_register: {
            //     PlayingPreviewRegister::new(shared_source.clone(), track.as_ref())
            // },
            preview_register: None,
            rt_column: shared_source,
            rt_command_sender: ColumnCommandSender::new(command_sender),
            slots: Default::default(),
            event_receiver,
            project: permanent_project,
        }
    }

    pub fn id(&self) -> &ColumnId {
        &self.id
    }

    pub fn duplicate(&self, rt_equipment: ColumnRtEquipment) -> Self {
        let mut new = self.duplicate_without_contents();
        new.slots = self
            .slots
            .values()
            .map(|slot| {
                let duplicate_slot = slot.duplicate(slot.index());
                (duplicate_slot.rt_id(), duplicate_slot)
            })
            .collect();
        new.resync_slots_to_rt_column(rt_equipment);
        new
    }

    pub(crate) fn set_clip_data(
        &mut self,
        slot_index: usize,
        clip_index: usize,
        api_clip: api::Clip,
    ) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, slot_index)?;
        slot.set_clip_data(clip_index, api_clip, &self.rt_command_sender)?;
        Ok(())
    }

    pub fn move_slot_contents(
        &mut self,
        source_index: usize,
        dest_index: usize,
    ) -> ClipEngineResult<()> {
        if source_index >= self.slots.len() {
            return Err("source index out of bounds");
        }
        if dest_index >= self.slots.len() {
            return Err("destination index out of bounds");
        }
        if source_index == dest_index {
            return Ok(());
        }
        // This will get more complicated in future as soon as we support moving on non-empty slots.
        self.slots.swap_indices(source_index, dest_index);
        self.reindex_slots();
        self.rt_command_sender
            .move_slot_contents(ColumnMoveSlotContentsArgs {
                source_index,
                dest_index,
            });
        Ok(())
    }

    pub(crate) fn reorder_slots(
        &mut self,
        source_index: usize,
        dest_index: usize,
    ) -> ClipEngineResult<()> {
        if source_index >= self.slots.len() {
            return Err("source slot doesn't exist");
        }
        if dest_index >= self.slots.len() {
            return Err("destination slot doesn't exist");
        }
        self.slots.move_index(source_index, dest_index);
        self.reindex_slots();
        self.rt_command_sender
            .reorder_slots(ColumnReorderSlotsArgs {
                source_index,
                dest_index,
            });
        Ok(())
    }
    pub fn set_play_mode(&mut self, play_mode: ColumnPlayMode) {
        self.rt_settings.play_mode = play_mode;
    }

    pub fn duplicate_without_contents(&self) -> Self {
        let mut duplicate = Self::new(ColumnId::random(), self.project);
        duplicate.settings = self.settings.clone();
        duplicate.rt_settings = self.rt_settings.clone();
        if let Some(pr) = &self.preview_register {
            duplicate.init_preview_register_if_necessary(pr.track.clone());
        }
        duplicate
    }

    /// Returns the sender for sending commands to the corresponding real-time column.
    pub fn rt_command_sender(&self) -> &ColumnCommandSender {
        &self.rt_command_sender
    }

    pub fn resolve_track_by_id(&self, track_id: &TrackId) -> ClipEngineResult<Track> {
        let guid = Guid::from_string_without_braces(track_id.get())?;
        let track = self.project.or_current_project().track_by_guid(&guid)?;
        Ok(track)
    }

    pub fn load(
        &mut self,
        api_column: api::Column,
        necessary_row_count: usize,
        rt_equipment: ColumnRtEquipment,
    ) -> ClipEngineResult<()> {
        // Track
        let track = if let Some(id) = api_column.clip_play_settings.track.as_ref() {
            self.resolve_track_by_id(id).ok()
        } else {
            None
        };
        self.init_preview_register_if_necessary(track);
        // Settings
        self.settings = ColumnSettings::from_api(&api_column);
        self.rt_settings = rt::RtColumnSettings::from_api(&api_column);
        // Create slots for all rows
        let api_slots = api_column.slots.unwrap_or_default();
        let mut api_slots_map: HashMap<_, _> = api_slots
            .into_iter()
            .map(|api_slot| (api_slot.row, api_slot))
            .collect();
        self.slots = (0..necessary_row_count)
            .map(|row_index| {
                let slot = if let Some(api_slot) = api_slots_map.remove(&row_index) {
                    let mut slot = Slot::new(api_slot.id.clone(), row_index);
                    slot.load(api_slot.into_clips());
                    slot
                } else {
                    Slot::new(SlotId::random(), row_index)
                };
                (slot.rt_id(), slot)
            })
            .collect();
        // Bring slots online
        self.resync_slots_to_rt_column(rt_equipment);
        // Send real-time slots to the real-time column
        self.sync_matrix_and_column_settings_to_rt_column_internal(rt_equipment.matrix_settings);
        Ok(())
    }

    /// Resynchronizes all slots to the real-time column.
    ///
    /// Although this recreates all PCM sources and sends them to the real-time column, the
    /// real-time column should take core to not do more than necessary and ensure a smooth
    /// transition into the new state. This is the code that's also used for undo/redo (restoring
    /// history states).
    ///
    /// So this can also be used for small changed when too lazy to create a real-time column
    /// command. It's a bit heavier on resources though, so it shouldn't be used for column changes
    /// that can happen very frequently.
    fn resync_slots_to_rt_column(&mut self, rt_equipment: ColumnRtEquipment) {
        let rt_slots: RtSlots = self
            .slots
            .values_mut()
            .map(|slot| {
                let rt_slot = slot.bring_online(
                    rt_equipment.chain_equipment,
                    rt_equipment.recorder_request_sender,
                    rt_equipment.matrix_settings,
                    &self.rt_settings,
                    self.project,
                );
                (rt_slot.id(), rt_slot)
            })
            .collect();
        self.rt_command_sender.load(ColumnLoadArgs {
            new_slots: rt_slots,
        });
    }

    fn init_preview_register_if_necessary(&mut self, track: Option<Track>) {
        if let Some(r) = &self.preview_register {
            if r.track == track {
                // No need to init. Column already uses a preview register for that track.
                return;
            }
        }
        self.preview_register = Some(PlayingPreviewRegister::new(self.rt_column.clone(), track));
    }

    pub fn sync_matrix_and_column_settings_to_rt_column(&self, matrix_settings: &MatrixSettings) {
        self.sync_matrix_and_column_settings_to_rt_column_internal(matrix_settings);
    }

    fn sync_matrix_and_column_settings_to_rt_column_internal(
        &self,
        matrix_settings: &MatrixSettings,
    ) {
        self.rt_command_sender
            .update_settings(self.rt_settings.clone());
        self.rt_command_sender
            .update_matrix_settings(matrix_settings.overridable.clone());
    }

    /// Returns all clips that are currently playing (along with slot index) .
    pub(crate) fn playing_clips(&self) -> impl Iterator<Item = (usize, &Clip)> + '_ {
        // TODO-high-clip-engine This is used for building a scene from the currently playing clips.
        //  If multiple clips are currently playing in one column, we shouldn't add new columns
        //  but put the clips into one slot! This is a new possibility and this is a good use case!
        self.slots.values().enumerate().flat_map(|(i, s)| {
            let is_playing = s.play_state().is_as_good_as_playing();
            if is_playing {
                Either::Left(s.clips().map(move |c| (i, c)))
            } else {
                Either::Right(iter::empty())
            }
        })
    }

    pub fn clear_slots(&mut self) {
        self.slots.clear();
        self.rt_command_sender.clear_slots();
    }

    pub fn get_slot(&self, index: usize) -> ClipEngineResult<&Slot> {
        Ok(self.slots.get_index(index).ok_or(SLOT_DOESNT_EXIST)?.1)
    }

    pub(crate) fn get_slot_mut(&mut self, index: usize) -> ClipEngineResult<&mut Slot> {
        get_slot_mut(&mut self.slots, index)
    }

    pub(crate) fn get_slot_kit_mut(&mut self, index: usize) -> ClipEngineResult<SlotKit> {
        let kit = SlotKit {
            sender: &self.rt_command_sender,
            slot: get_slot_mut(&mut self.slots, index)?,
        };
        Ok(kit)
    }

    /// Returns whether the slot at the given index is empty.
    pub(crate) fn slot_is_empty(&self, index: usize) -> bool {
        match self.slots.get_index(index) {
            None => true,
            Some((_, s)) => s.is_empty(),
        }
    }

    /// Returns the actual number of slots in this column.
    ///
    /// Just interesting for internal usage. For external usage, the matrix row count is important.
    pub(super) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn save(&self) -> api::Column {
        let track_id = self.preview_register.as_ref().and_then(|reg| {
            reg.track
                .as_ref()
                .map(|t| t.guid().to_string_without_braces())
                .map(api::TrackId::new)
        });
        api::Column {
            id: self.id.clone(),
            clip_play_settings: ColumnClipPlaySettings {
                mode: Some(self.rt_settings.play_mode),
                track: track_id,
                start_timing: self.rt_settings.clip_play_start_timing,
                stop_timing: self.rt_settings.clip_play_stop_timing,
                audio_settings: ColumnClipPlayAudioSettings {
                    resample_mode: self.rt_settings.audio_resample_mode,
                    time_stretch_mode: self.rt_settings.audio_time_stretch_mode,
                    cache_behavior: self.rt_settings.audio_cache_behavior,
                },
            },
            clip_record_settings: self.settings.clip_record_settings.clone(),
            slots: {
                let slots = self
                    .slots
                    .values()
                    .filter_map(|slot| slot.save(self.project))
                    .collect();
                Some(slots)
            },
        }
    }

    pub fn create_handle(&self) -> ColumnHandle {
        ColumnHandle {
            pointer: self.rt_column.downgrade(),
            command_sender: self.rt_command_sender.clone(),
        }
    }

    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<(usize, SlotChangeEvent)> {
        // Process source events and generate clip change events
        let mut change_events = vec![];
        while let Ok(evt) = self.event_receiver.try_recv() {
            use RtColumnEvent::*;
            let change_event = match evt {
                SlotPlayStateChanged {
                    slot_id,
                    play_state,
                } => {
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        slot.update_play_state(play_state);
                        Some((slot.index(), SlotChangeEvent::PlayState(play_state)))
                    } else {
                        None
                    }
                }
                ClipMaterialInfoChanged {
                    slot_id,
                    clip_id,
                    material_info,
                } => {
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        let _ = slot.update_material_info(clip_id, material_info);
                    }
                    None
                }
                Dispose(_) => None,
                RecordRequestAcknowledged {
                    slot_id, result, ..
                } => {
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        slot.notify_recording_request_acknowledged(result).unwrap();
                    }
                    None
                }
                MidiOverdubFinished {
                    slot_id,
                    mirror_source,
                } => {
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        let event = slot
                            .notify_midi_overdub_finished(mirror_source, self.project)
                            .unwrap();
                        Some((slot.index(), event))
                    } else {
                        None
                    }
                }
                NormalRecordingFinished { slot_id, outcome } => {
                    let recording_track = &self.effective_recording_track().unwrap();
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        let event = slot
                            .notify_normal_recording_finished(
                                outcome,
                                self.project,
                                recording_track,
                            )
                            .unwrap();
                        Some((slot.index(), event))
                    } else {
                        None
                    }
                }
                InteractionFailed(failure) => {
                    let formatted = format!("Playtime: Interaction failed ({})", failure.message);
                    Reaper::get()
                        .medium_reaper()
                        .help_set(formatted, HelpMode::Temporary);
                    None
                }
                SlotCleared { slot_id, .. } => {
                    if let Some(slot) = self.slots.get_mut(&slot_id) {
                        slot.slot_cleared().map(|e| (slot.index(), e))
                    } else {
                        None
                    }
                }
            };
            if let Some(evt) = change_event {
                change_events.push(evt);
            }
        }
        // Add position updates
        let continuous_clip_events = self.slots.values().enumerate().flat_map(|(row, slot)| {
            if !slot.play_state().is_advancing() {
                return Either::Right(iter::empty());
            }
            let temp_project = self.project.or_current_project();
            let iter = match slot.relevant_contents() {
                RelevantContent::Normal(contents) => {
                    let iter = contents.filter_map(move |content| {
                        let online_data = content.online_data.as_ref()?;
                        let seconds =
                            online_data.position_in_seconds(&content.clip, timeline_tempo);
                        let event = SlotChangeEvent::Continuous {
                            proportional: online_data.proportional_position().unwrap_or_default(),
                            seconds,
                            peak: online_data.peak(),
                        };
                        content.notify_pos_changed(temp_project, timeline_tempo, seconds);
                        Some((row, event))
                    });
                    Either::Left(iter)
                }
                RelevantContent::Recording(runtime_data) => {
                    let event = SlotChangeEvent::Continuous {
                        proportional: runtime_data.proportional_position().unwrap_or_default(),
                        seconds: runtime_data.position_in_seconds_during_recording(timeline_tempo),
                        peak: runtime_data.peak(),
                    };
                    Either::Right(iter::once((row, event)))
                }
            };
            Either::Left(iter)
        });
        change_events.extend(continuous_clip_events);
        change_events
    }

    /// Clears the given slot.
    ///
    /// # Errors
    ///
    /// Returns an error if the slot doesn't exist.
    pub fn clear_slot(&mut self, slot_index: usize) -> ClipEngineResult<()> {
        self.get_slot_mut(slot_index)?.clear();
        self.rt_command_sender.clear_slot(slot_index);
        Ok(())
    }

    /// Freezes the complete column.
    pub async fn freeze(&mut self, _column_index: usize) -> ClipEngineResult<()> {
        let playback_track = self.playback_track()?.clone();
        for slot in self.slots.values_mut() {
            // TODO-high-clip-matrix implement
            let _ = slot.freeze(&playback_track).await;
        }
        Ok(())
    }

    /// Adds the given clips to the slot or replaces all existing ones.
    ///
    /// Immediately syncs to real-time column.
    pub(crate) fn fill_slot(
        &mut self,
        slot_index: usize,
        api_clips: Vec<api::Clip>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &MatrixSettings,
        mode: FillSlotMode,
    ) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, slot_index)?;
        slot.fill(
            api_clips,
            chain_equipment,
            recorder_request_sender,
            matrix_settings,
            &self.rt_settings,
            &self.rt_command_sender,
            self.project,
            mode,
        );
        Ok(())
    }

    pub(crate) fn replace_slot_clips_with_selected_item(
        &mut self,
        slot_index: usize,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &MatrixSettings,
    ) -> ClipEngineResult<()> {
        let project = self.project.or_current_project();
        let item = project.first_selected_item().ok_or("no item selected")?;
        let source = source_util::create_api_source_from_item(item, false)
            .map_err(|_| "couldn't create source from item")?;
        let clip = api::Clip {
            id: Default::default(),
            name: None,
            source,
            frozen_source: None,
            active_source: Default::default(),
            // TODO-high-clip-engine Derive whether time or beat from item/track/project
            time_base: ClipTimeBase::Beat(BeatTimeBase {
                // TODO-high-clip-engine Correctly determine audio tempo, only if audio
                audio_tempo: Some(api::Bpm::new(project.tempo().bpm().get())?),
                // TODO-high-clip-engine Correctly determine time signature at item position
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                // TODO-high-clip-engine Correctly determine by looking at snap offset
                downbeat: PositiveBeat::default(),
            }),
            start_timing: None,
            stop_timing: None,
            // TODO-high-clip-engine Check if item itself is looped or not
            looped: true,
            // TODO-high-clip-engine Derive from item take volume
            volume: api::Db::ZERO,
            // TODO-high-clip-engine Derive from item color
            color: ClipColor::PlayTrackColor,
            // TODO-high-clip-engine Derive from item cut
            section: Section {
                start_pos: PositiveSecond::default(),
                length: None,
            },
            audio_settings: ClipAudioSettings {
                apply_source_fades: true,
                // TODO-high-clip-engine Derive from item time stretch mode
                time_stretch_mode: None,
                // TODO-high-clip-engine Derive from item resample mode
                resample_mode: None,
                cache_behavior: None,
            },
            midi_settings: preferred_clip_midi_settings(),
        };
        self.fill_slot(
            slot_index,
            vec![clip],
            chain_equipment,
            recorder_request_sender,
            matrix_settings,
            FillSlotMode::Replace,
        )
    }

    pub(crate) fn play_scene(&self, args: ColumnPlayRowArgs) {
        self.rt_command_sender.play_row(args);
    }

    pub(crate) fn play_slot(&self, args: ColumnPlaySlotArgs) {
        self.rt_command_sender.play_slot(args);
    }

    pub(crate) fn stop_slot(&self, args: ColumnStopSlotArgs) {
        self.rt_command_sender.stop_slot(args);
    }

    pub(crate) fn duplicate_slot(
        &mut self,
        slot_index: usize,
        rt_equipment: ColumnRtEquipment,
    ) -> ClipEngineResult<()> {
        if slot_index >= self.slots.len() {
            return Err("slot to be removed doesn't exist");
        }
        let (_, slot) = self
            .slots
            .get_index(slot_index)
            .ok_or("slot doesn't exist")?;
        let duplicate_slot = slot.duplicate(slot_index + 1);
        let new_index = duplicate_slot.index();
        self.slots.insert(duplicate_slot.rt_id(), duplicate_slot);
        self.slots.move_index(self.slots.len() - 1, new_index);
        self.reindex_slots();
        self.resync_slots_to_rt_column(rt_equipment);
        Ok(())
    }

    pub(crate) fn insert_slot(
        &mut self,
        slot_index: usize,
        rt_equipment: ColumnRtEquipment,
    ) -> ClipEngineResult<()> {
        if slot_index > self.slots.len() {
            return Err("slot index too large");
        }
        let new_slot = Slot::new(SlotId::random(), slot_index);
        self.slots.insert(new_slot.rt_id(), new_slot);
        self.slots.move_index(self.slots.len() - 1, slot_index);
        self.reindex_slots();
        self.resync_slots_to_rt_column(rt_equipment);
        Ok(())
    }

    pub(crate) fn remove_slot(&mut self, slot_index: usize) -> ClipEngineResult<()> {
        if slot_index >= self.slots.len() {
            return Err("slot to be removed doesn't exist");
        }
        self.slots.shift_remove_index(slot_index);
        self.reindex_slots();
        self.rt_command_sender.remove_slot(slot_index);
        Ok(())
    }

    fn reindex_slots(&mut self) {
        for (i, slot) in self.slots.values_mut().enumerate() {
            slot.set_index(i);
        }
    }

    pub(crate) fn stop(&self, args: ColumnStopArgs) {
        self.rt_command_sender.stop(args);
    }

    pub fn panic(&self) {
        self.rt_command_sender.panic();
    }

    pub(crate) fn pause_slot(&self, slot_index: usize) {
        self.rt_command_sender.pause_slot(slot_index);
    }

    pub(crate) fn seek_slot(&self, slot_index: usize, desired_pos: UnitValue) {
        self.rt_command_sender.seek_slot(slot_index, desired_pos);
    }

    pub fn set_clip_volume(
        &mut self,
        slot_index: usize,
        volume: Db,
    ) -> ClipEngineResult<ClipChangeEvent> {
        let slot = get_slot_mut(&mut self.slots, slot_index)?;
        slot.set_volume(volume, &self.rt_command_sender)
    }

    pub fn slots(&self) -> impl Iterator<Item = &Slot> + '_ {
        self.slots.values()
    }

    /// Returns whether some slots in this column are currently playing/recording.
    pub fn is_stoppable(&self) -> bool {
        self.slots.values().any(|slot| slot.is_stoppable())
    }

    pub fn is_armed_for_recording(&self) -> bool {
        self.effective_recording_track()
            .map(|t| t.is_armed(true))
            .unwrap_or(false)
    }

    pub fn effective_recording_track(&self) -> ClipEngineResult<Track> {
        let playback_track = self.playback_track()?;
        resolve_recording_track(&self.settings.clip_record_settings, playback_track)
    }

    /// Sets the playback track of this column.
    pub fn set_playback_track(&mut self, track_id: Option<&TrackId>) -> ClipEngineResult<()> {
        let track = if let Some(id) = track_id {
            Some(self.resolve_track_by_id(id)?)
        } else {
            None
        };
        self.init_preview_register_if_necessary(track);
        Ok(())
    }

    /// Returns the playback track of this column.
    pub fn playback_track(&self) -> ClipEngineResult<&Track> {
        self.preview_register
            .as_ref()
            .ok_or("column inactive")?
            .track
            .as_ref()
            .ok_or("no playback track set")
    }

    pub fn follows_scene(&self) -> bool {
        self.rt_settings.play_mode.follows_scene()
    }

    pub fn is_recording(&self) -> bool {
        self.slots.values().any(|s| s.is_recording())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_slot<H: ClipMatrixHandler>(
        &mut self,
        slot_index: usize,
        matrix_record_settings: &MatrixClipRecordSettings,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        handler: &H,
        containing_track: Option<&Track>,
        overridable_matrix_settings: &OverridableMatrixSettings,
    ) -> ClipEngineResult<()> {
        let recording_track = &self.effective_recording_track()?;
        // Insert slot if it doesn't exist already.
        let slot = get_slot_mut_insert(&mut self.slots, slot_index);
        slot.record_clip(
            matrix_record_settings,
            &self.settings.clip_record_settings,
            &self.rt_settings,
            chain_equipment,
            recorder_request_sender,
            handler,
            containing_track,
            overridable_matrix_settings,
            recording_track,
            &self.rt_column,
            &self.rt_command_sender,
        )
    }
}

impl Drop for PlayingPreviewRegister {
    fn drop(&mut self) {
        self.stop_playing_preview();
    }
}

impl PlayingPreviewRegister {
    pub fn new(source: impl CustomPcmSource + 'static, track: Option<Track>) -> Self {
        let mut register = OwnedPreviewRegister::default();
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        let (out_chan, preview_track) = if let Some(t) = track.as_ref() {
            (-1, Some(t.raw()))
        } else {
            (0, None)
        };
        register.set_out_chan(out_chan);
        register.set_preview_track(preview_track);
        let source = create_custom_owned_pcm_source(source);
        register.set_src(Some(FlexibleOwnedPcmSource::Custom(source)));
        let preview_register = Arc::new(ReaperMutex::new(register));
        let play_handle = start_playing_preview(&preview_register, track.as_ref());
        Self {
            _preview_register: preview_register,
            play_handle,
            track,
        }
    }

    fn stop_playing_preview(&mut self) {
        if let Some(track) = &self.track {
            // Check prevents error message on project close.
            let project = track.project();
            // If not successful this probably means it was stopped already, so okay.
            let _ = Reaper::get()
                .medium_session()
                .stop_track_preview_2(project.context(), self.play_handle);
        } else {
            // If not successful this probably means it was stopped already, so okay.
            let _ = Reaper::get()
                .medium_session()
                .stop_preview(self.play_handle);
        };
    }
}

fn start_playing_preview(
    reg: &SharedRegister,
    track: Option<&Track>,
) -> NonNull<preview_register_t> {
    debug!("Starting preview on track {:?}", &track);
    let buffering_behavior = BitFlags::empty();
    let measure_alignment = MeasureAlignment::PlayImmediately;
    let result = if let Some(track) = track {
        Reaper::get().medium_session().play_track_preview_2_ex(
            track.project().context(),
            reg.clone(),
            buffering_behavior,
            measure_alignment,
        )
    } else {
        panic!("Attempting to initialize column without track. Not yet supported.")
        // Reaper::get().medium_session().play_preview_ex(
        //     reg.clone(),
        //     buffering_behavior,
        //     measure_alignment,
        // )
    };
    result.unwrap()
}

fn get_slot_mut(slots: &mut Slots, index: usize) -> ClipEngineResult<&mut Slot> {
    Ok(slots.get_index_mut(index).ok_or(SLOT_DOESNT_EXIST)?.1)
}

fn get_slot_mut_insert(slots: &mut Slots, slot_index: usize) -> &mut Slot {
    upsize_if_necessary(slots, slot_index + 1);
    slots.get_index_mut(slot_index).unwrap().1
}

fn upsize_if_necessary(slots: &mut Slots, row_count: usize) {
    let current_row_count = slots.len();
    if current_row_count < row_count {
        let missing_rows = (current_row_count..row_count).map(|i| {
            let slot = Slot::new(SlotId::random(), i);
            (slot.rt_id(), slot)
        });
        slots.extend(missing_rows);
    }
}

const SLOT_DOESNT_EXIST: &str = "slot doesn't exist";

fn resolve_recording_track(
    column_settings: &ColumnClipRecordSettings,
    playback_track: &Track,
) -> ClipEngineResult<Track> {
    if let Some(track_id) = &column_settings.track {
        let track_guid = Guid::from_string_without_braces(track_id.get())?;
        let track = playback_track.project().track_by_guid(&track_guid)?;
        if track.is_available() {
            Ok(track)
        } else {
            Err("track not available")
        }
    } else {
        Ok(playback_track.clone())
    }
}

pub(crate) struct SlotKit<'a> {
    pub slot: &'a mut Slot,
    pub sender: &'a ColumnCommandSender,
}

#[derive(Copy, Clone)]
pub struct ColumnRtEquipment<'a> {
    pub chain_equipment: &'a ChainEquipment,
    pub recorder_request_sender: &'a Sender<RecorderRequest>,
    pub matrix_settings: &'a MatrixSettings,
}
