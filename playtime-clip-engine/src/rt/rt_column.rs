use crate::mutex_util::{blocking_lock, non_blocking_lock};
use crate::rt::supplier::{MaterialInfo, MidiOverdubOutcome, WriteAudioRequest, WriteMidiRequest};
use crate::rt::{
    BasicAudioRequestProps, ClipRecordingPollArgs, HandleSlotEvent, InternalClipPlayState,
    NormalRecordingOutcome, OwnedAudioBuffer, RtClip, RtClipId, RtClipSettings, RtClips, RtSlot,
    RtSlotId, SetClipSettingsArgs, SlotLoadArgs, SlotLoadClipArgs, SlotPlayArgs, SlotProcessArgs,
    SlotProcessTransportChangeArgs, SlotRecordInstruction, SlotRuntimeData, SlotStopArgs,
    TransportChange,
};
use crate::timeline::{clip_timeline, HybridTimeline, Timeline};
use crate::ClipEngineResult;
use assert_no_alloc::assert_no_alloc;
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use indexmap::IndexMap;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    AudioCacheBehavior, AudioTimeStretchMode, ClipPlayStartTiming, ClipPlayStopTiming,
    ColumnClipPlaySettings, ColumnPlayMode, Db, MatrixClipPlaySettings, VirtualResampleMode,
};
use reaper_high::Project;
use reaper_medium::{
    reaper_str, CustomPcmSource, DurationInBeats, DurationInSeconds, ExtendedArgs, GetPeakInfoArgs,
    GetSamplesArgs, Hz, LoadStateArgs, OwnedPcmSource, PcmSource, PeaksClearArgs,
    PositionInSeconds, PropertiesWindowArgs, ReaperStr, SaveStateArgs, SetAvailableArgs,
    SetFileNameArgs, SetSourceArgs,
};
use std::error::Error;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use xxhash_rust::xxh3::Xxh3Builder;

/// Only such methods are public which are allowed to use from real-time threads. Other ones
/// are private and called from the method that processes the incoming commands.
#[derive(Debug)]
pub struct RtColumn {
    matrix_settings: OverridableMatrixSettings,
    settings: RtColumnSettings,
    slots: RtSlots,
    /// Slots end up here when removed.
    ///
    /// They stay there until they have faded out (prevents abrupt stops).
    retired_slots: Vec<RtSlot>,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
    command_receiver: Receiver<RtColumnCommand>,
    event_sender: Sender<RtColumnEvent>,
    /// Enough reserved memory to hold one audio block of an arbitrary size.
    mix_buffer_chunk: Vec<f64>,
    timeline_was_paused_in_last_block: bool,
}

#[derive(Clone, Debug)]
pub struct SharedRtColumn(Arc<Mutex<RtColumn>>);

#[derive(Clone, Debug)]
pub struct WeakRtColumn(Weak<Mutex<RtColumn>>);

impl SharedRtColumn {
    pub fn new(column_source: RtColumn) -> Self {
        Self(Arc::new(Mutex::new(column_source)))
    }

    pub fn lock(&self) -> MutexGuard<RtColumn> {
        non_blocking_lock(&self.0, "real-time column")
    }

    pub fn lock_allow_blocking(&self) -> MutexGuard<RtColumn> {
        blocking_lock(&self.0)
    }

    pub fn downgrade(&self) -> WeakRtColumn {
        WeakRtColumn(Arc::downgrade(&self.0))
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl WeakRtColumn {
    pub fn upgrade(&self) -> Option<SharedRtColumn> {
        self.0.upgrade().map(SharedRtColumn)
    }
}

#[derive(Clone, Debug)]
pub struct ColumnCommandSender {
    /// Whether the sender should indeed send the commands or discard them.
    ///
    /// If the column doesn't have an associated track, then it doesn't have a preview register.
    /// If no preview is running, real-time column commands can't be processed on the real-time
    /// side. Sending them anyway would make the tasks accumulate and get out-of-date. That's why
    /// we should discard them in this case. A full resync will be done as soon as a track is
    /// associated again.
    is_enabled: bool,
    command_sender: Sender<RtColumnCommand>,
}

impl ColumnCommandSender {
    pub fn new(command_sender: Sender<RtColumnCommand>) -> Self {
        Self {
            is_enabled: false,
            command_sender,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    pub fn move_slot_contents(&self, args: ColumnMoveSlotContentsArgs) {
        self.send_task(RtColumnCommand::MoveSlotContents(args));
    }

    pub fn reorder_slots(&self, args: ColumnReorderSlotsArgs) {
        self.send_task(RtColumnCommand::ReorderSlots(args));
    }

    pub fn clear_slots(&self) {
        self.send_task(RtColumnCommand::ClearSlots);
    }

    pub fn load(&self, args: ColumnLoadArgs) {
        self.send_task(RtColumnCommand::Load(args));
    }

    pub fn update_settings(&self, settings: RtColumnSettings) {
        self.send_task(RtColumnCommand::UpdateColumnSettings(settings));
    }

    pub fn update_matrix_settings(&self, settings: OverridableMatrixSettings) {
        self.send_task(RtColumnCommand::UpdateMatrixSettings(settings));
    }

    pub fn load_slot(&self, args: Box<Option<ColumnLoadSlotArgs>>) {
        self.send_task(RtColumnCommand::LoadSlot(args));
    }

    pub fn load_clip(&self, args: ColumnLoadClipArgs) {
        self.send_task(RtColumnCommand::LoadClip(Box::new(args)));
    }

    pub fn process_transport_change(&self, args: ColumnProcessTransportChangeArgs) {
        self.send_task(RtColumnCommand::ProcessTransportChange(args));
    }

    pub fn clear_slot(&self, slot_index: usize) {
        self.send_task(RtColumnCommand::ClearSlot(slot_index));
    }

    pub fn play_slot(&self, args: ColumnPlaySlotArgs) {
        self.send_task(RtColumnCommand::PlaySlot(args));
    }

    pub fn play_row(&self, args: ColumnPlayRowArgs) {
        self.send_task(RtColumnCommand::PlayRow(args));
    }

    pub fn stop_slot(&self, args: ColumnStopSlotArgs) {
        self.send_task(RtColumnCommand::StopSlot(args));
    }

    pub fn remove_slot(&self, index: usize) {
        self.send_task(RtColumnCommand::RemoveSlot(index));
    }

    pub fn stop(&self, args: ColumnStopArgs) {
        self.send_task(RtColumnCommand::Stop(args));
    }

    pub fn panic(&self) {
        self.send_task(RtColumnCommand::Panic);
    }

    pub fn panic_slot(&self, slot_index: usize) {
        self.send_task(RtColumnCommand::PanicSlot(slot_index));
    }

    pub fn set_clip_settings(&self, args: ColumnSetClipSettingsArgs) {
        self.send_task(RtColumnCommand::SetClipSettings(args));
    }

    pub fn set_clip_looped(&self, args: ColumnSetClipLoopedArgs) {
        self.send_task(RtColumnCommand::SetClipLooped(args));
    }

    pub fn pause_slot(&self, index: usize) {
        let args = ColumnPauseSlotArgs { index };
        self.send_task(RtColumnCommand::PauseSlot(args));
    }

    pub fn seek_slot(&self, index: usize, desired_pos: UnitValue) {
        let args = ColumnSeekSlotArgs { index, desired_pos };
        self.send_task(RtColumnCommand::SeekSlot(args));
    }

    pub fn set_clip_volume(&self, slot_index: usize, clip_index: usize, volume: Db) {
        let args = ColumnSetClipVolumeArgs {
            slot_index,
            clip_index,
            volume,
        };
        self.send_task(RtColumnCommand::SetClipVolume(args));
    }

    pub fn set_clip_section(&self, slot_index: usize, clip_index: usize, section: api::Section) {
        let args = ColumnSetClipSectionArgs {
            slot_index,
            clip_index,
            section,
        };
        self.send_task(RtColumnCommand::SetClipSection(args));
    }

    pub fn record_clip(&self, slot_index: usize, instruction: SlotRecordInstruction) {
        let args = ColumnRecordClipArgs {
            slot_index,
            instruction,
        };
        self.send_task(RtColumnCommand::RecordClip(Box::new(Some(args))));
    }

    fn send_task(&self, task: RtColumnCommand) {
        if !self.is_enabled {
            return;
        }
        self.command_sender.try_send(task).unwrap();
    }
}

#[derive(Debug)]
pub enum RtColumnCommand {
    ClearSlots,
    Panic,
    PanicSlot(usize),
    Load(ColumnLoadArgs),
    MoveSlotContents(ColumnMoveSlotContentsArgs),
    ReorderSlots(ColumnReorderSlotsArgs),
    ClearSlot(usize),
    RemoveSlot(usize),
    UpdateColumnSettings(RtColumnSettings),
    UpdateMatrixSettings(OverridableMatrixSettings),
    // Boxed because comparatively large.
    LoadSlot(Box<Option<ColumnLoadSlotArgs>>),
    LoadClip(Box<ColumnLoadClipArgs>),
    ProcessTransportChange(ColumnProcessTransportChangeArgs),
    PlaySlot(ColumnPlaySlotArgs),
    PlayRow(ColumnPlayRowArgs),
    StopSlot(ColumnStopSlotArgs),
    Stop(ColumnStopArgs),
    PauseSlot(ColumnPauseSlotArgs),
    SeekSlot(ColumnSeekSlotArgs),
    SetClipSettings(ColumnSetClipSettingsArgs),
    SetClipVolume(ColumnSetClipVolumeArgs),
    SetClipLooped(ColumnSetClipLoopedArgs),
    SetClipSection(ColumnSetClipSectionArgs),
    RecordClip(Box<Option<ColumnRecordClipArgs>>),
}

pub trait RtColumnEventSender {
    fn slot_play_state_changed(&self, slot_id: RtSlotId, play_state: InternalClipPlayState);

    fn clip_material_info_changed(
        &self,
        slot_id: RtSlotId,
        clip_id: RtClipId,
        material_info: MaterialInfo,
    );

    fn slot_cleared(&self, slot_id: RtSlotId, clips: RtClips);

    fn record_request_acknowledged(
        &self,
        slot_id: RtSlotId,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    );

    fn midi_overdub_finished(
        &self,
        slot_id: RtSlotId,
        clip_id: RtClipId,
        outcome: MidiOverdubOutcome,
    );

    fn normal_recording_finished(&self, slot_id: RtSlotId, outcome: NormalRecordingOutcome);

    fn interaction_failed(&self, failure: InteractionFailure);

    fn dispose(&self, garbage: RtColumnGarbage);

    fn send_event(&self, event: RtColumnEvent);
}

impl RtColumnEventSender for Sender<RtColumnEvent> {
    fn slot_play_state_changed(&self, slot_id: RtSlotId, play_state: InternalClipPlayState) {
        let event = RtColumnEvent::SlotPlayStateChanged {
            slot_id,
            play_state,
        };
        self.send_event(event);
    }

    fn clip_material_info_changed(
        &self,
        slot_id: RtSlotId,
        clip_id: RtClipId,
        material_info: MaterialInfo,
    ) {
        let event = RtColumnEvent::ClipMaterialInfoChanged {
            slot_id,
            clip_id,
            material_info,
        };
        self.send_event(event);
    }

    fn slot_cleared(&self, slot_id: RtSlotId, clips: RtClips) {
        let event = RtColumnEvent::SlotCleared { slot_id, clips };
        self.send_event(event);
    }

    fn record_request_acknowledged(
        &self,
        slot_id: RtSlotId,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    ) {
        let event = RtColumnEvent::RecordRequestAcknowledged { slot_id, result };
        self.send_event(event);
    }

    fn midi_overdub_finished(
        &self,
        slot_id: RtSlotId,
        clip_id: RtClipId,
        outcome: MidiOverdubOutcome,
    ) {
        let event = RtColumnEvent::MidiOverdubFinished {
            slot_id,
            clip_id,
            outcome,
        };
        self.send_event(event);
    }

    fn normal_recording_finished(&self, slot_id: RtSlotId, outcome: NormalRecordingOutcome) {
        let event = RtColumnEvent::NormalRecordingFinished { slot_id, outcome };
        self.send_event(event);
    }

    fn dispose(&self, garbage: RtColumnGarbage) {
        self.send_event(RtColumnEvent::Dispose(garbage));
    }

    fn interaction_failed(&self, failure: InteractionFailure) {
        self.send_event(RtColumnEvent::InteractionFailed(failure));
    }

    fn send_event(&self, event: RtColumnEvent) {
        self.try_send(event).unwrap();
    }
}

#[derive(Clone, Debug, Default)]
pub struct RtColumnSettings {
    pub clip_play_start_timing: Option<ClipPlayStartTiming>,
    pub clip_play_stop_timing: Option<ClipPlayStopTiming>,
    pub audio_time_stretch_mode: Option<AudioTimeStretchMode>,
    pub audio_resample_mode: Option<VirtualResampleMode>,
    pub audio_cache_behavior: Option<AudioCacheBehavior>,
    pub play_mode: ColumnPlayMode,
}

impl RtColumnSettings {
    pub fn from_api(settings: &ColumnClipPlaySettings) -> Self {
        Self {
            clip_play_start_timing: settings.start_timing,
            clip_play_stop_timing: settings.stop_timing,
            audio_time_stretch_mode: settings.audio_settings.time_stretch_mode,
            audio_resample_mode: settings.audio_settings.resample_mode,
            audio_cache_behavior: settings.audio_settings.cache_behavior,
            play_mode: settings.mode.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OverridableMatrixSettings {
    pub clip_play_start_timing: ClipPlayStartTiming,
    pub clip_play_stop_timing: ClipPlayStopTiming,
    pub audio_time_stretch_mode: AudioTimeStretchMode,
    pub audio_resample_mode: VirtualResampleMode,
    pub audio_cache_behavior: AudioCacheBehavior,
}

impl OverridableMatrixSettings {
    pub fn from_api(clip_play_settings: &MatrixClipPlaySettings) -> Self {
        OverridableMatrixSettings {
            clip_play_start_timing: clip_play_settings.start_timing,
            clip_play_stop_timing: clip_play_settings.stop_timing,
            audio_time_stretch_mode: clip_play_settings.audio_settings.time_stretch_mode,
            audio_resample_mode: clip_play_settings.audio_settings.resample_mode,
            audio_cache_behavior: clip_play_settings.audio_settings.cache_behavior,
        }
    }
}

const MAX_AUDIO_CHANNEL_COUNT: usize = 64;
const MAX_BLOCK_SIZE: usize = 2048;

/// At the time of this writing, a slot is just around 900 byte, so 100 slots take roughly 90 kB.
/// TODO-high-clip-engine 100 slots in one column is a lot ... but don't we want to guarantee
///  "no allocation" even for thousands of slots?
const MAX_SLOT_COUNT_WITHOUT_REALLOCATION: usize = 100;

impl RtColumn {
    pub fn new(
        permanent_project: Option<Project>,
        command_receiver: Receiver<RtColumnCommand>,
        event_sender: Sender<RtColumnEvent>,
    ) -> Self {
        debug!("Slot size: {}", std::mem::size_of::<RtSlot>());
        let hash_builder = base::hash_util::create_non_crypto_hash_builder();
        Self {
            matrix_settings: Default::default(),
            settings: Default::default(),
            slots: RtSlots::with_capacity_and_hasher(
                MAX_SLOT_COUNT_WITHOUT_REALLOCATION,
                hash_builder,
            ),
            retired_slots: Vec::with_capacity(MAX_SLOT_COUNT_WITHOUT_REALLOCATION),
            project: permanent_project,
            command_receiver,
            event_sender,
            // Sized to hold pretty any audio block imaginable. Vastly oversized for the majority
            // of use cases but 1 MB memory per column ... okay for now, on the safe side.
            mix_buffer_chunk: OwnedAudioBuffer::new(MAX_AUDIO_CHANNEL_COUNT, MAX_BLOCK_SIZE)
                .into_inner(),
            timeline_was_paused_in_last_block: false,
        }
    }

    fn load_slot(&mut self, args: ColumnLoadSlotArgs) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, args.slot_index)?;
        let slot_args = SlotLoadArgs {
            new_clips: args.clips,
            event_sender: &self.event_sender,
            matrix_settings: &self.matrix_settings,
            column_settings: &self.settings,
        };
        slot.load(slot_args);
        Ok(())
    }

    fn load_clip(&mut self, args: &mut ColumnLoadClipArgs) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, args.slot_index)?;
        let slot_args = SlotLoadClipArgs {
            clip_index: args.clip_index,
            new_clip: &mut args.clip,
            event_sender: &self.event_sender,
            matrix_settings: &self.matrix_settings,
            column_settings: &self.settings,
        };
        slot.load_clip(slot_args)?;
        Ok(())
    }

    pub fn slot(&self, index: usize) -> ClipEngineResult<&RtSlot> {
        get_slot(&self.slots, index)
    }

    /// # Errors
    ///
    /// Returns an error if the slot doesn't exist, doesn't have any clip or is currently recording.
    pub fn play_slot(
        &mut self,
        args: ColumnPlaySlotArgs,
        audio_request_props: BasicAudioRequestProps,
    ) -> ClipEngineResult<()> {
        let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
        let slot_args = SlotPlayArgs {
            timeline: &args.timeline,
            ref_pos: Some(ref_pos),
            matrix_settings: &self.matrix_settings,
            column_settings: &self.settings,
            start_timing: args.options.start_timing,
        };
        let slot = get_slot_mut(&mut self.slots, args.slot_index)?;
        if slot.is_filled() {
            slot.play(slot_args)?;
            if self.settings.play_mode.is_exclusive() {
                self.stop_all_clips(
                    audio_request_props,
                    ref_pos,
                    &args.timeline,
                    Some(args.slot_index),
                    None,
                );
            }
            Ok(())
        } else if args.options.stop_column_if_slot_empty {
            self.stop_all_clips(audio_request_props, ref_pos, &args.timeline, None, None);
            Ok(())
        } else {
            Err("slot is empty")
        }
    }

    /// # Errors
    ///
    /// Returns an error if the row doesn't exist.
    pub fn play_row(
        &mut self,
        args: ColumnPlayRowArgs,
        audio_request_props: BasicAudioRequestProps,
    ) -> ClipEngineResult<()> {
        if !self.settings.play_mode.follows_scene() {
            return Ok(());
        }
        if !self.settings.play_mode.is_exclusive() {
            // When in column play mode "NonExclusiveFollowingScene", playing the clip itself
            // doesn't take care of stopping the other clips. But when playing scenes, we want
            // other clips to stop (otherwise they would accumulate). Do it manually.
            self.stop_all_clips(
                audio_request_props,
                args.ref_pos,
                &args.timeline,
                Some(args.slot_index),
                None,
            );
        }
        let play_args = ColumnPlaySlotArgs {
            slot_index: args.slot_index,
            timeline: args.timeline,
            ref_pos: Some(args.ref_pos),
            options: ColumnPlaySlotOptions {
                stop_column_if_slot_empty: true,
                start_timing: None,
            },
        };
        self.play_slot(play_args, audio_request_props)
    }

    pub fn stop(&mut self, args: ColumnStopArgs, audio_request_props: BasicAudioRequestProps) {
        let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
        self.stop_all_clips(
            audio_request_props,
            ref_pos,
            &args.timeline,
            None,
            args.stop_timing,
        );
    }

    fn stop_all_clips(
        &mut self,
        audio_request_props: BasicAudioRequestProps,
        ref_pos: PositionInSeconds,
        timeline: &HybridTimeline,
        except: Option<usize>,
        stop_timing: Option<ClipPlayStopTiming>,
    ) {
        for (_, slot) in self
            .slots
            .values_mut()
            .enumerate()
            .filter(|(i, _)| except.map(|e| e != *i).unwrap_or(true))
        {
            let stop_args = SlotStopArgs {
                stop_timing,
                timeline,
                ref_pos: Some(ref_pos),
                enforce_play_stop: true,
                matrix_settings: &self.matrix_settings,
                column_settings: &self.settings,
                audio_request_props,
            };
            let event_handler = SlotEventHandler::new(&self.event_sender, slot.id());
            let _ = slot.stop(stop_args, &event_handler);
        }
    }

    /// # Errors
    ///
    /// Returns an error if the slot doesn't exist or doesn't have any clip.
    pub fn stop_slot(
        &mut self,
        args: ColumnStopSlotArgs,
        audio_request_props: BasicAudioRequestProps,
    ) -> ClipEngineResult<()> {
        let clip_args = SlotStopArgs {
            stop_timing: args.stop_timing,
            timeline: &args.timeline,
            ref_pos: args.ref_pos,
            enforce_play_stop: false,
            matrix_settings: &self.matrix_settings,
            column_settings: &self.settings,
            audio_request_props,
        };
        let slot = get_slot_mut(&mut self.slots, args.slot_index)?;
        let event_handler = SlotEventHandler::new(&self.event_sender, slot.id());
        slot.stop(clip_args, &event_handler)
    }

    pub fn panic(&mut self) {
        for slot in self.slots.values_mut() {
            slot.panic();
        }
    }

    pub fn panic_slot(&mut self, index: usize) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, index)?.panic();
        Ok(())
    }

    pub fn remove_slot(&mut self, index: usize) -> ClipEngineResult<()> {
        let (_, slot) = self
            .slots
            .shift_remove_index(index)
            .ok_or("slot to be removed doesn't exist")?;
        self.retire_slot(slot);
        Ok(())
    }

    fn retire_slot(&mut self, mut slot: RtSlot) {
        slot.initiate_removal();
        self.retired_slots.push(slot);
    }

    pub fn set_clip_settings(&mut self, args: ColumnSetClipSettingsArgs) -> ClipEngineResult<()> {
        let clip_args = SetClipSettingsArgs {
            clip_settings: args.settings,
            matrix_settings: &self.matrix_settings,
            column_settings: &self.settings,
        };
        get_slot_mut(&mut self.slots, args.slot_index)?
            .get_clip_mut(args.clip_index)?
            .set_settings(clip_args)
    }

    pub fn set_clip_looped(&mut self, args: ColumnSetClipLoopedArgs) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, args.slot_index)?
            .get_clip_mut(args.clip_index)?
            .set_looped(args.looped)
    }

    pub fn set_clip_section(&mut self, args: ColumnSetClipSectionArgs) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, args.slot_index)?
            .get_clip_mut(args.clip_index)?
            .set_section(args.section)
    }

    /// See [`RtClip::recording_poll`].
    pub fn recording_poll(
        &mut self,
        slot_index: usize,
        audio_request_props: BasicAudioRequestProps,
    ) -> bool {
        match get_slot_mut(&mut self.slots, slot_index) {
            Ok(slot) => {
                let args = ClipRecordingPollArgs {
                    matrix_settings: &self.matrix_settings,
                    column_settings: &self.settings,
                    audio_request_props,
                };
                let event_handler = SlotEventHandler::new(&self.event_sender, slot.id());
                slot.recording_poll(args, &event_handler)
            }
            Err(_) => false,
        }
    }

    fn record_clip(
        &mut self,
        slot_index: usize,
        instruction: SlotRecordInstruction,
        audio_request_props: BasicAudioRequestProps,
    ) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, slot_index)?;
        let slot_id = slot.id();
        let result = slot.record_clip(instruction, &self.matrix_settings, &self.settings);
        let (informative_result, ack_result) = match result {
            Ok(slot_runtime_data) => {
                if self.settings.play_mode.is_exclusive() {
                    let timeline = clip_timeline(self.project, false);
                    let ref_pos = timeline.cursor_pos();
                    self.stop_all_clips(
                        audio_request_props,
                        ref_pos,
                        &timeline,
                        Some(slot_index),
                        None,
                    );
                }
                (Ok(()), Ok(slot_runtime_data))
            }
            Err(e) => (Err(e.message), Err(e.payload)),
        };
        self.event_sender
            .record_request_acknowledged(slot_id, ack_result);
        informative_result
    }

    pub fn is_stoppable(&self) -> bool {
        self.slots.values().any(|slot| slot.is_stoppable())
    }

    pub fn pause_slot(&mut self, args: ColumnPauseSlotArgs) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, args.index)?.pause()
    }

    fn seek_clip(&mut self, args: ColumnSeekSlotArgs) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, args.index)?.seek(args.desired_pos)
    }

    pub fn write_clip_midi(
        &mut self,
        index: usize,
        request: WriteMidiRequest,
    ) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, index)?.write_clip_midi(request)
    }

    pub fn write_clip_audio(
        &mut self,
        slot_index: usize,
        request: impl WriteAudioRequest,
    ) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, slot_index)?.write_clip_audio(request)
    }

    fn set_clip_volume(&mut self, args: ColumnSetClipVolumeArgs) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, args.slot_index)?
            .get_clip_mut(args.clip_index)?
            .set_volume(args.volume);
        Ok(())
    }

    fn process_transport_change(&mut self, args: ColumnProcessTransportChangeArgs) {
        let args = SlotProcessTransportChangeArgs {
            column_args: &args,
            matrix_settings: &self.matrix_settings,
            column_settings: &self.settings,
        };
        for slot in self.slots.values_mut() {
            let event_handler = SlotEventHandler::new(&self.event_sender, slot.id());
            let _ = slot.process_transport_change(&args, &event_handler);
        }
    }

    /// The duration of this column source is infinite.
    fn duration(&self) -> DurationInSeconds {
        DurationInSeconds::MAX
    }

    /// Clears all the slots in this column, fading out still playing clips.
    pub fn clear_slots(&mut self) {
        for slot in self.slots.values_mut() {
            slot.clear();
        }
    }

    /// Replaces the slots in this column with the given ones but keeps unchanged slots playing
    /// if possible and fades out still playing old slots.
    pub fn load(&mut self, mut args: ColumnLoadArgs) {
        // Take old slots out
        let mut old_slots = mem::take(&mut self.slots);
        // For each new slot, check if there's a corresponding old slot. In that case, update
        // the old slot instead of completely replacing it with the new one. This keeps unchanged
        // playing slots playing.
        for new_slot in args.new_slots.values_mut() {
            if let Some(mut old_slot) = old_slots.remove(&new_slot.id()) {
                // We have an old slot with the same ID. Reuse it for smooth transition!
                // Load the new slot's clips into the old clip by the slot's terms. After this, the
                // new slot doesn't have clips and should not be used anymore.
                let slot_args = SlotLoadArgs {
                    new_clips: mem::take(&mut new_slot.clips),
                    event_sender: &self.event_sender,
                    matrix_settings: &self.matrix_settings,
                    column_settings: &self.settings,
                };
                old_slot.load(slot_args);
                // Declare the old slot to be the new slot
                let obsolete_slot = mem::replace(new_slot, old_slot);
                // Dispose the obsolete slot
                self.event_sender
                    .dispose(RtColumnGarbage::Slot(obsolete_slot));
            }
        }
        // Declare the mixture of updated and new slots as the new slot collection!
        self.slots = args.new_slots;
        // Retire old and now unused slots
        for (_, slot) in old_slots.drain(..) {
            self.retire_slot(slot);
        }
        // Dispose old and now empty slot collection
        self.event_sender.dispose(RtColumnGarbage::Slots(old_slots));
    }

    /// Clears the clips in the given slot, fading out still playing clips.
    pub fn clear_slot(&mut self, index: usize) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, index)?;
        slot.clear();
        Ok(())
    }

    pub fn move_slot_contents(&mut self, args: ColumnMoveSlotContentsArgs) -> ClipEngineResult<()> {
        if args.source_index >= self.slots.len() {
            return Err("source index out of bounds");
        }
        if args.dest_index >= self.slots.len() {
            return Err("destination index out of bounds");
        }
        self.slots.swap_indices(args.source_index, args.dest_index);
        Ok(())
    }

    pub fn reorder_slots(&mut self, args: ColumnReorderSlotsArgs) -> ClipEngineResult<()> {
        if args.source_index >= self.slots.len() {
            return Err("source index out of bounds");
        }
        if args.dest_index >= self.slots.len() {
            return Err("destination index out of bounds");
        }
        self.slots.move_index(args.source_index, args.dest_index);
        Ok(())
    }

    fn process_commands(&mut self, audio_request_props: BasicAudioRequestProps) {
        while let Ok(task) = self.command_receiver.try_recv() {
            use RtColumnCommand::*;
            match task {
                ClearSlots => {
                    self.clear_slots();
                }
                Load(args) => {
                    self.load(args);
                }
                ClearSlot(slot_index) => {
                    let result = self.clear_slot(slot_index);
                    self.notify_user_about_failed_interaction(result);
                }
                MoveSlotContents(args) => {
                    self.move_slot_contents(args).unwrap();
                }
                ReorderSlots(args) => {
                    self.reorder_slots(args).unwrap();
                }
                UpdateColumnSettings(s) => {
                    self.settings = s;
                }
                UpdateMatrixSettings(s) => {
                    self.matrix_settings = s;
                }
                LoadSlot(mut boxed_args) => {
                    let args = boxed_args.take().unwrap();
                    self.load_slot(args).unwrap();
                    self.event_sender
                        .dispose(RtColumnGarbage::LoadSlotArgs(boxed_args));
                }
                LoadClip(mut boxed_args) => {
                    self.load_clip(&mut *boxed_args).unwrap();
                    self.event_sender
                        .dispose(RtColumnGarbage::LoadClipArgs(boxed_args));
                }
                PlaySlot(args) => {
                    let result = self.play_slot(args, audio_request_props);
                    self.notify_user_about_failed_interaction(result);
                }
                PlayRow(args) => {
                    let result = self.play_row(args, audio_request_props);
                    self.notify_user_about_failed_interaction(result);
                }
                ProcessTransportChange(args) => {
                    self.process_transport_change(args);
                }
                StopSlot(args) => {
                    let result = self.stop_slot(args, audio_request_props);
                    self.notify_user_about_failed_interaction(result);
                }
                RemoveSlot(index) => {
                    self.remove_slot(index).unwrap();
                }
                Stop(args) => {
                    self.stop(args, audio_request_props);
                }
                Panic => {
                    self.panic();
                }
                PanicSlot(slot_index) => {
                    self.panic_slot(slot_index).unwrap();
                }
                PauseSlot(args) => {
                    self.pause_slot(args).unwrap();
                }
                SetClipVolume(args) => {
                    self.set_clip_volume(args).unwrap();
                }
                SeekSlot(args) => {
                    self.seek_clip(args).unwrap();
                }
                SetClipSettings(args) => {
                    self.set_clip_settings(args).unwrap();
                }
                SetClipLooped(args) => {
                    self.set_clip_looped(args).unwrap();
                }
                SetClipSection(args) => {
                    self.set_clip_section(args).unwrap();
                }
                RecordClip(mut boxed_args) => {
                    let args = boxed_args.take().unwrap();
                    let result =
                        self.record_clip(args.slot_index, args.instruction, audio_request_props);
                    self.notify_user_about_failed_interaction(result);
                    self.event_sender
                        .dispose(RtColumnGarbage::RecordClipArgs(boxed_args));
                }
            }
        }
    }

    fn notify_user_about_failed_interaction<T>(&self, result: ClipEngineResult<T>) {
        if let Err(message) = result {
            let failure = InteractionFailure { message };
            debug!("Failed clip interaction: {}", message);
            self.event_sender.interaction_failed(failure);
        }
    }

    fn get_samples(&mut self, args: GetSamplesArgs) {
        // We have code, e.g. triggered by crossbeam_channel that requests the ID of the
        // current thread. This operation needs an allocation at the first time it's executed
        // on a specific thread. If Live FX multi-processing is enabled, get_samples() will be
        // called from a non-sticky worker thread instead of the audio interface thread (for which
        // we already do this), so we execute this here again in order to initialize the current
        // thread outside of assert_no_alloc.
        let _ = std::thread::current().id();
        assert_no_alloc(|| {
            let request_props = BasicAudioRequestProps::from_transfer(args.block);
            // Super important that commands are processed before getting samples from clips.
            // That's what guarantees that we act immediately to changes and also don't miss any
            // samples after finishing recording.
            self.process_commands(request_props);
            // Make sure that in any case, we are only queried once per time, without retries.
            // TODO-medium This mechanism of advancing the position on every call by
            //  the block duration relies on the fact that the preview
            //  register timeline calls us continuously and never twice per block.
            //  It would be better not to make that assumption and make this more
            //  stable by actually looking at the diff between the currently requested
            //  time_s and the previously requested time_s. If this diff is zero or
            //  doesn't correspond to the non-tempo-adjusted block duration, we know
            //  something is wrong.
            unsafe {
                args.block.set_samples_out(args.block.length());
            }
            // Get main timeline info
            let timeline = clip_timeline(self.project, false);
            // Handle sync to project pause
            if !timeline.is_running() {
                // Main timeline is paused.
                self.timeline_was_paused_in_last_block = true;
                return;
            }
            let resync = if self.timeline_was_paused_in_last_block {
                self.timeline_was_paused_in_last_block = false;
                true
            } else {
                false
            };
            // Get samples
            let timeline_cursor_pos = timeline.cursor_pos();
            let timeline_tempo = timeline.tempo_at(timeline_cursor_pos);
            // rt_debug!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
            //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
            let mut slot_args = SlotProcessArgs {
                block: &mut *args.block,
                mix_buffer_chunk: &mut self.mix_buffer_chunk,
                timeline: &timeline,
                timeline_cursor_pos,
                timeline_tempo,
                resync,
                matrix_settings: &self.matrix_settings,
                column_settings: &self.settings,
                event_sender: &self.event_sender,
            };
            // Fade out retired slots
            self.retired_slots.retain_mut(|slot| {
                let outcome = slot.process(&mut slot_args);
                // As long as the slot still wrote audio frames, we keep it in memory. But as soon
                // as no audio frames are written anymore, we can safely assume it's stopped and
                // drop it.
                let keep = outcome.num_audio_frames_written > 0;
                // If done, dispose the slot in order to avoid deallocation in real-time thread
                if !keep {
                    self.event_sender
                        .dispose(RtColumnGarbage::Slot(mem::take(slot)));
                }
                keep
            });
            // Play current slots
            for slot in self.slots.values_mut() {
                let outcome = slot.process(&mut slot_args);
                if let Some(changed_play_state) = outcome.changed_play_state {
                    self.event_sender
                        .slot_play_state_changed(slot.id(), changed_play_state);
                }
            }
        });
        debug_assert_eq!(args.block.samples_out(), args.block.length());
    }

    fn extended(&mut self, _args: ExtendedArgs) -> i32 {
        // TODO-medium Maybe implement PCM_SOURCE_EXT_NOTIFYPREVIEWPLAYPOS. This is the only
        //  extended call done by the preview register, at least for type WAVE.
        0
    }
}

impl CustomPcmSource for SharedRtColumn {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        unimplemented!()
    }

    fn is_available(&mut self) -> bool {
        unimplemented!()
    }

    fn set_available(&mut self, _: SetAvailableArgs) {
        unimplemented!()
    }

    fn get_type(&mut self) -> &ReaperStr {
        // This is not relevant for usage in preview registers, but it will be called.
        // TODO-medium Return something less misleading here.
        reaper_str!("WAVE")
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unimplemented!()
    }

    fn set_file_name(&mut self, _: SetFileNameArgs) -> bool {
        unimplemented!()
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        unimplemented!()
    }

    fn set_source(&mut self, _: SetSourceArgs) {
        unimplemented!()
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        // This will only be called if the preview register is played without track.
        unimplemented!("track-less columns not yet supported")
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        unimplemented!()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        self.lock().duration()
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        unimplemented!()
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        unimplemented!()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        unimplemented!()
    }

    fn properties_window(&mut self, _: PropertiesWindowArgs) -> i32 {
        unimplemented!()
    }

    fn get_samples(&mut self, args: GetSamplesArgs) {
        self.lock().get_samples(args)
    }

    fn get_peak_info(&mut self, _: GetPeakInfoArgs) {
        unimplemented!()
    }

    fn save_state(&mut self, _: SaveStateArgs) {
        unimplemented!()
    }

    fn load_state(&mut self, _: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unimplemented!()
    }

    fn peaks_clear(&mut self, _: PeaksClearArgs) {
        unimplemented!()
    }

    fn peaks_build_begin(&mut self) -> bool {
        unimplemented!()
    }

    fn peaks_build_run(&mut self) -> bool {
        unimplemented!()
    }

    fn peaks_build_finish(&mut self) {
        unimplemented!()
    }

    unsafe fn extended(&mut self, args: ExtendedArgs) -> i32 {
        self.lock().extended(args)
    }
}

pub type RtSlots = IndexMap<RtSlotId, RtSlot, Xxh3Builder>;

#[derive(Debug)]
pub struct ColumnLoadArgs {
    pub new_slots: RtSlots,
}

#[derive(Debug)]
pub struct ColumnMoveSlotContentsArgs {
    pub source_index: usize,
    pub dest_index: usize,
}

#[derive(Debug)]
pub struct ColumnReorderSlotsArgs {
    pub source_index: usize,
    pub dest_index: usize,
}

#[derive(Debug)]
pub struct ColumnLoadSlotArgs {
    pub slot_index: usize,
    pub clips: RtClips,
}

#[derive(Debug)]
pub struct ColumnLoadClipArgs {
    pub slot_index: usize,
    pub clip_index: usize,
    pub clip: RtClip,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FillSlotMode {
    Add,
    Replace,
}

#[derive(Debug)]
pub struct ColumnSetClipAudioResampleModeArgs {
    pub slot_index: usize,
    pub mode: VirtualResampleMode,
}

#[derive(Debug)]
pub struct ColumnSetClipAudioTimeStretchModeArgs {
    pub slot_index: usize,
    pub mode: AudioTimeStretchMode,
}

#[derive(Clone, Debug)]
pub struct ColumnPlaySlotArgs {
    pub slot_index: usize,
    pub timeline: HybridTimeline,
    /// Set this if you already have the current timeline position or want to play a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
    pub options: ColumnPlaySlotOptions,
}

#[derive(Clone, Debug)]
pub struct ColumnPlayRowArgs {
    pub slot_index: usize,
    pub timeline: HybridTimeline,
    pub ref_pos: PositionInSeconds,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnPlaySlotOptions {
    /// If the slot to be played is empty and this is `false`, nothing happens. If it's `true`,
    /// it acts like a column stop button (good for matrix controllers without column stop button).
    pub stop_column_if_slot_empty: bool,
    pub start_timing: Option<ClipPlayStartTiming>,
}

#[derive(Debug)]
pub struct ColumnStopSlotArgs {
    pub slot_index: usize,
    pub timeline: HybridTimeline,
    /// Set this if you already have the current timeline position or want to stop a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
    pub stop_timing: Option<ClipPlayStopTiming>,
}

#[derive(Clone, Debug)]
pub struct ColumnStopArgs {
    pub timeline: HybridTimeline,
    /// Set this if you already have the current timeline position or want to stop a batch of columns.
    pub ref_pos: Option<PositionInSeconds>,
    pub stop_timing: Option<ClipPlayStopTiming>,
}

#[derive(Debug)]
pub struct ColumnPauseSlotArgs {
    pub index: usize,
}

#[derive(Debug)]
pub struct ColumnSeekSlotArgs {
    pub index: usize,
    pub desired_pos: UnitValue,
}

#[derive(Debug)]
pub struct ColumnSetClipSettingsArgs {
    pub slot_index: usize,
    pub clip_index: usize,
    pub settings: RtClipSettings,
}

#[derive(Debug)]
pub struct ColumnSetClipVolumeArgs {
    pub slot_index: usize,
    pub clip_index: usize,
    pub volume: Db,
}

#[derive(Debug)]
pub struct ColumnRecordClipArgs {
    pub slot_index: usize,
    pub instruction: SlotRecordInstruction,
}

#[derive(Debug)]
pub struct ColumnSetClipLoopedArgs {
    pub slot_index: usize,
    pub clip_index: usize,
    pub looped: bool,
}

#[derive(Debug)]
pub struct ColumnSetClipSectionArgs {
    pub slot_index: usize,
    pub clip_index: usize,
    pub section: api::Section,
}

pub struct ColumnWithSlotArgs<'a> {
    pub index: usize,
    pub use_slot: &'a dyn Fn(),
}

fn get_slot(slots: &RtSlots, index: usize) -> ClipEngineResult<&RtSlot> {
    Ok(slots.get_index(index).ok_or(SLOT_DOESNT_EXIST)?.1)
}

fn get_slot_mut(slots: &mut RtSlots, index: usize) -> ClipEngineResult<&mut RtSlot> {
    Ok(slots.get_index_mut(index).ok_or(SLOT_DOESNT_EXIST)?.1)
}

const SLOT_DOESNT_EXIST: &str = "slot doesn't exist";

#[derive(Debug)]
pub enum RtColumnEvent {
    SlotPlayStateChanged {
        slot_id: RtSlotId,
        play_state: InternalClipPlayState,
    },
    ClipMaterialInfoChanged {
        slot_id: RtSlotId,
        clip_id: RtClipId,
        material_info: MaterialInfo,
    },
    SlotCleared {
        slot_id: RtSlotId,
        clips: RtClips,
    },
    RecordRequestAcknowledged {
        slot_id: RtSlotId,
        /// Slot runtime data is returned only if it's a recording from scratch (slot was not
        /// filled before).
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    },
    MidiOverdubFinished {
        slot_id: RtSlotId,
        clip_id: RtClipId,
        outcome: MidiOverdubOutcome,
    },
    NormalRecordingFinished {
        slot_id: RtSlotId,
        outcome: NormalRecordingOutcome,
    },
    Dispose(RtColumnGarbage),
    InteractionFailed(InteractionFailure),
}

#[derive(Debug)]
pub struct InteractionFailure {
    pub message: &'static str,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum RtColumnGarbage {
    Slot(RtSlot),
    LoadSlotArgs(Box<Option<ColumnLoadSlotArgs>>),
    LoadClipArgs(Box<ColumnLoadClipArgs>),
    Clip(Option<RtClip>),
    RecordClipArgs(Box<Option<ColumnRecordClipArgs>>),
    Slots(RtSlots),
    Clips(RtClips),
}

struct SlotEventHandler<'a> {
    event_sender: &'a Sender<RtColumnEvent>,
    slot_id: RtSlotId,
}

impl<'a> SlotEventHandler<'a> {
    pub fn new(event_sender: &'a Sender<RtColumnEvent>, slot_id: RtSlotId) -> Self {
        Self {
            slot_id,
            event_sender,
        }
    }
}

impl<'a> HandleSlotEvent for SlotEventHandler<'a> {
    fn midi_overdub_finished(&self, clip_id: RtClipId, outcome: MidiOverdubOutcome) {
        self.event_sender
            .midi_overdub_finished(self.slot_id, clip_id, outcome);
    }

    fn normal_recording_finished(&self, outcome: NormalRecordingOutcome) {
        self.event_sender
            .normal_recording_finished(self.slot_id, outcome);
    }

    fn slot_cleared(&self, clips: RtClips) {
        self.event_sender.slot_cleared(self.slot_id, clips);
    }
}

#[derive(Clone, Debug)]
pub struct ColumnProcessTransportChangeArgs {
    pub change: TransportChange,
    pub timeline: HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub audio_request_props: BasicAudioRequestProps,
}
