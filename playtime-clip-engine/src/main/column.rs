use crate::main::{Clip, ClipMatrixHandler, MatrixSettings, Slot};
use crate::rt::supplier::{ChainEquipment, RecorderRequest};
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ColumnCommandSender, ColumnEvent, ColumnFillSlotArgs,
    ColumnPlayClipArgs, ColumnStopArgs, ColumnStopClipArgs, OverridableMatrixSettings,
    SharedColumn, WeakColumn,
};
use crate::{clip_timeline, rt, ClipEngineResult};
use crossbeam_channel::{Receiver, Sender};
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ColumnClipPlayAudioSettings, ColumnClipPlaySettings, ColumnClipRecordSettings, Db,
    MatrixClipRecordSettings,
};
use reaper_high::{Guid, OrCurrentProject, Project, Reaper, Track};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, Bpm, CustomPcmSource, FlexibleOwnedPcmSource, HelpMode,
    MeasureAlignment, OwnedPreviewRegister, PositionInSeconds, ReaperMutex, ReaperVolumeValue,
};
use std::ptr::NonNull;
use std::sync::Arc;

pub type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, Debug)]
pub struct Column {
    settings: ColumnSettings,
    rt_settings: rt::ColumnSettings,
    rt_command_sender: ColumnCommandSender,
    rt_column: SharedColumn,
    preview_register: Option<PlayingPreviewRegister>,
    slots: Vec<Slot>,
    event_receiver: Receiver<ColumnEvent>,
    project: Option<Project>,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    pub clip_record_settings: ColumnClipRecordSettings,
}

#[derive(Clone, Debug)]
struct PlayingPreviewRegister {
    _preview_register: SharedRegister,
    play_handle: NonNull<preview_register_t>,
    track: Option<Track>,
}

impl Column {
    pub fn new(permanent_project: Option<Project>) -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(500);
        let (event_sender, event_receiver) = crossbeam_channel::bounded(500);
        let source = rt::Column::new(permanent_project, command_receiver, event_sender);
        let shared_source = SharedColumn::new(source);
        Self {
            settings: Default::default(),
            rt_settings: Default::default(),
            // preview_register: {
            //     PlayingPreviewRegister::new(shared_source.clone(), track.as_ref())
            // },
            preview_register: None,
            rt_column: shared_source,
            rt_command_sender: ColumnCommandSender::new(command_sender),
            slots: vec![],
            event_receiver,
            project: permanent_project,
        }
    }

    pub fn rt_command_sender(&self) -> ColumnCommandSender {
        self.rt_command_sender.clone()
    }

    pub fn load(
        &mut self,
        api_column: api::Column,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &MatrixSettings,
    ) -> ClipEngineResult<()> {
        self.clear_slots();
        // Track
        let track = if let Some(id) = api_column.clip_play_settings.track.as_ref() {
            let guid = Guid::from_string_without_braces(id.get())?;
            Some(self.project.or_current_project().track_by_guid(&guid))
        } else {
            None
        };
        self.preview_register = Some(PlayingPreviewRegister::new(self.rt_column.clone(), track));
        // Settings
        self.settings.clip_record_settings = api_column.clip_record_settings;
        self.rt_settings.audio_resample_mode =
            api_column.clip_play_settings.audio_settings.resample_mode;
        self.rt_settings.audio_time_stretch_mode = api_column
            .clip_play_settings
            .audio_settings
            .time_stretch_mode;
        self.rt_settings.audio_cache_behavior =
            api_column.clip_play_settings.audio_settings.cache_behavior;
        self.rt_settings.play_mode = api_column.clip_play_settings.mode.unwrap_or_default();
        self.rt_settings.clip_play_start_timing = api_column.clip_play_settings.start_timing;
        self.rt_settings.clip_play_stop_timing = api_column.clip_play_settings.stop_timing;
        self.rt_command_sender
            .update_settings(self.rt_settings.clone());
        self.rt_command_sender
            .update_matrix_settings(matrix_settings.overridable.clone());
        // Slots
        for api_slot in api_column.slots.unwrap_or_default() {
            if let Some(api_clip) = api_slot.clip {
                let clip = Clip::load(api_clip);
                let slot = get_slot_mut_insert(&mut self.slots, api_slot.row);
                fill_slot_internal(
                    slot,
                    clip,
                    chain_equipment,
                    recorder_request_sender,
                    matrix_settings,
                    &self.rt_settings,
                    &self.rt_command_sender,
                    self.project,
                )?;
            }
        }
        Ok(())
    }

    pub fn clear_slots(&mut self) {
        self.slots.clear();
        self.rt_command_sender.clear_slots();
    }

    /// Is mutable because empty slots are created lazily up to `row_count`.
    pub(super) fn slot(&mut self, index: usize, row_count: usize) -> Option<&Slot> {
        upsize_if_necessary(&mut self.slots, row_count);
        self.slots.get(index)
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
                    .iter()
                    .filter_map(|slot| slot.save(self.project))
                    .collect();
                Some(slots)
            },
        }
    }

    pub fn rt_column(&self) -> WeakColumn {
        self.rt_column.downgrade()
    }

    pub fn poll(&mut self, _timeline_tempo: Bpm) -> Vec<(usize, ClipChangedEvent)> {
        // Process source events and generate clip change events
        let mut change_events = vec![];
        while let Ok(evt) = self.event_receiver.try_recv() {
            use ColumnEvent::*;
            let change_event = match evt {
                ClipPlayStateChanged {
                    slot_index,
                    play_state,
                } => {
                    if let Some(slot) = self.slots.get_mut(slot_index) {
                        let _ = slot.update_play_state(play_state);
                    }
                    Some((slot_index, ClipChangedEvent::PlayState(play_state)))
                }
                ClipMaterialInfoChanged {
                    slot_index,
                    material_info,
                } => {
                    if let Some(slot) = self.slots.get_mut(slot_index) {
                        let _ = slot.update_material_info(material_info);
                    }
                    None
                }
                Dispose(_) => None,
                RecordRequestAcknowledged {
                    slot_index, result, ..
                } => {
                    if let Some(slot) = self.slots.get_mut(slot_index) {
                        slot.notify_recording_request_acknowledged(result).unwrap();
                    }
                    None
                }
                MidiOverdubFinished {
                    slot_index,
                    mirror_source,
                } => {
                    if let Some(slot) = self.slots.get_mut(slot_index) {
                        slot.notify_midi_overdub_finished(mirror_source, self.project)
                            .unwrap();
                    }
                    None
                }
                NormalRecordingFinished {
                    slot_index,
                    outcome,
                } => {
                    if let Some(slot) = self.slots.get_mut(slot_index) {
                        let event = slot
                            .notify_normal_recording_finished(outcome, self.project)
                            .unwrap();
                        event.map(|e| (slot_index, e))
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
                SlotCleared { slot_index, .. } => {
                    if let Some(slot) = self.slots.get_mut(slot_index) {
                        slot.slot_cleared().map(|e| (slot_index, e))
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
        let pos_change_events = self.slots.iter().enumerate().filter_map(|(row, slot)| {
            if slot.clip_play_state().ok()?.is_advancing() {
                let proportional_pos = slot.proportional_pos().unwrap_or(UnitValue::MIN);
                let event = ClipChangedEvent::ClipPosition(proportional_pos);
                Some((row, event))
            } else {
                None
            }
        });
        change_events.extend(pos_change_events);
        change_events
    }

    pub fn clear_slot(&self, slot_index: usize) {
        self.rt_command_sender.clear_slot(slot_index);
    }

    pub fn start_editing_clip(&self, slot_index: usize) -> ClipEngineResult<()> {
        let slot = self.get_slot(slot_index)?;
        slot.start_editing_clip(self.project.or_current_project())
    }

    pub fn stop_editing_clip(&self, slot_index: usize) -> ClipEngineResult<()> {
        let slot = self.get_slot(slot_index)?;
        slot.stop_editing_clip(self.project.or_current_project())
    }

    pub fn is_editing_clip(&self, slot_index: usize) -> bool {
        if let Some(slot) = self.slots.get(slot_index) {
            slot.is_editing_clip(self.project.or_current_project())
        } else {
            false
        }
    }

    // TODO-high Implement
    pub fn fill_slot_with_selected_item(
        &mut self,
        _slot_index: usize,
        _chain_equipment: &ChainEquipment,
        _recorder_request_sender: &Sender<RecorderRequest>,
        _matrix_settings: &MatrixSettings,
    ) -> ClipEngineResult<()> {
        // let slot = get_slot_mut_insert(&mut self.slots, slot_index);
        // if !slot.is_empty() {
        //     return Err("slot is not empty");
        // }
        // let item = self
        //     .project
        //     .or_current_project()
        //     .first_selected_item()
        //     .ok_or("no item selected")?;
        // let clip = api::Clip {
        //     source: source_util::create_api_source_from_item(item, false)
        //         .map_err(|_| "couldn't create source from item")?,
        //     time_base: todo!(),
        //     start_timing: todo!(),
        //     stop_timing: todo!(),
        //     looped: todo!(),
        //     volume: api::Db::ZERO,
        //     color: ClipColor::PlayTrackColor,
        //     section: todo!(),
        //     audio_settings: todo!(),
        //     midi_settings: todo!(),
        // };
        // let clip = Clip::load(clip);
        // fill_slot_internal(
        //     slot,
        //     clip,
        //     chain_equipment,
        //     recorder_request_sender,
        //     matrix_settings,
        //     &self.rt_settings,
        //     &self.rt_command_sender,
        //     self.project,
        // )
        Err("not yet implemented")
    }

    pub fn play_clip(&self, args: ColumnPlayClipArgs) {
        self.rt_command_sender.play_clip(args);
    }

    pub fn stop_clip(&self, args: ColumnStopClipArgs) {
        self.rt_command_sender.stop_clip(args);
    }

    pub fn stop(&self, args: ColumnStopArgs) {
        self.rt_command_sender.stop(args);
    }

    pub fn pause_clip(&self, slot_index: usize) {
        self.rt_command_sender.pause_clip(slot_index);
    }

    pub fn seek_clip(&self, slot_index: usize, desired_pos: UnitValue) {
        self.rt_command_sender.seek_clip(slot_index, desired_pos);
    }

    pub fn set_clip_volume(&mut self, slot_index: usize, volume: Db) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, slot_index)?;
        slot.set_clip_volume(volume, &self.rt_command_sender)
    }

    pub fn toggle_clip_looped(&mut self, slot_index: usize) -> ClipEngineResult<ClipChangedEvent> {
        let slot = get_slot_mut(&mut self.slots, slot_index)?;
        slot.toggle_clip_looped(&self.rt_command_sender)
    }

    pub fn slot_position_in_seconds(
        &self,
        slot_index: usize,
    ) -> ClipEngineResult<PositionInSeconds> {
        let slot = self.get_slot(slot_index)?;
        let timeline = clip_timeline(self.project, false);
        slot.position_in_seconds(&timeline)
    }

    fn get_slot(&self, index: usize) -> ClipEngineResult<&Slot> {
        self.slots.get(index).ok_or(SLOT_DOESNT_EXIST)
    }

    pub fn clip_volume(&self, slot_index: usize) -> ClipEngineResult<Db> {
        self.get_slot(slot_index)?.clip_volume()
    }

    pub fn is_playing_something(&self) -> bool {
        self.slots.iter().any(|slot| slot.is_playing_something())
    }

    pub fn clip_play_state(&self, slot_index: usize) -> ClipEngineResult<ClipPlayState> {
        self.get_slot(slot_index)?.clip_play_state()
    }

    pub fn clip_looped(&self, slot_index: usize) -> ClipEngineResult<bool> {
        self.get_slot(slot_index)?.clip_looped()
    }

    pub fn proportional_slot_position(&self, slot_index: usize) -> ClipEngineResult<UnitValue> {
        self.get_slot(slot_index)?.proportional_pos()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_clip<H: ClipMatrixHandler>(
        &mut self,
        slot_index: usize,
        matrix_record_settings: &MatrixClipRecordSettings,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        handler: &H,
        containing_track: Option<&Track>,
        overridable_matrix_settings: &OverridableMatrixSettings,
    ) -> ClipEngineResult<()> {
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
            self.preview_register
                .as_ref()
                .ok_or("column inactive")?
                .track
                .as_ref()
                .ok_or("no playback track set")?,
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

fn get_slot_mut(slots: &mut Vec<Slot>, slot_index: usize) -> ClipEngineResult<&mut Slot> {
    slots.get_mut(slot_index).ok_or(SLOT_DOESNT_EXIST)
}

fn get_slot_mut_insert(slots: &mut Vec<Slot>, slot_index: usize) -> &mut Slot {
    upsize_if_necessary(slots, slot_index + 1);
    slots.get_mut(slot_index).unwrap()
}

fn upsize_if_necessary(slots: &mut Vec<Slot>, row_count: usize) {
    let mut current_row_count = slots.len();
    if current_row_count < row_count {
        slots.resize_with(row_count, || {
            let slot = Slot::new(current_row_count);
            current_row_count += 1;
            slot
        });
    }
}

const SLOT_DOESNT_EXIST: &str = "slot doesn't exist";

#[allow(clippy::too_many_arguments)]
fn fill_slot_internal(
    slot: &mut Slot,
    mut clip: Clip,
    chain_equipment: &ChainEquipment,
    recorder_request_sender: &Sender<RecorderRequest>,
    matrix_settings: &MatrixSettings,
    column_settings: &rt::ColumnSettings,
    rt_command_sender: &ColumnCommandSender,
    project: Option<Project>,
) -> ClipEngineResult<()> {
    let (rt_clip, pooled_midi_source) = clip.create_real_time_clip(
        project,
        chain_equipment,
        recorder_request_sender,
        &matrix_settings.overridable,
        column_settings,
    )?;
    slot.fill_with(clip, &rt_clip, pooled_midi_source);
    let args = ColumnFillSlotArgs {
        slot_index: slot.index(),
        clip: rt_clip,
    };
    rt_command_sender.fill_slot(Box::new(Some(args)));
    Ok(())
}
