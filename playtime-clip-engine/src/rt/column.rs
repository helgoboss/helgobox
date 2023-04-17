use crate::mutex_util::{blocking_lock, non_blocking_lock};
use crate::rt::supplier::{ClipSource, MaterialInfo, WriteAudioRequest, WriteMidiRequest};
use crate::rt::{
    AudioBufMut, BasicAudioRequestProps, Clip, ClipProcessArgs, ClipRecordingPollArgs,
    HandleSlotEvent, InternalClipPlayState, NormalRecordingOutcome, OwnedAudioBuffer, Slot,
    SlotPlayArgs, SlotProcessTransportChangeArgs, SlotRecordInstruction, SlotRuntimeData,
    SlotStopArgs, TransportChange,
};
use crate::timeline::{clip_timeline, HybridTimeline, Timeline};
use crate::ClipEngineResult;
use assert_no_alloc::assert_no_alloc;
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    AudioCacheBehavior, AudioTimeStretchMode, ClipPlayStartTiming, ClipPlayStopTiming,
    ColumnPlayMode, Db, VirtualResampleMode,
};
use reaper_high::Project;
use reaper_medium::{
    reaper_str, CustomPcmSource, DurationInBeats, DurationInSeconds, ExtendedArgs, GetPeakInfoArgs,
    GetSamplesArgs, Hz, LoadStateArgs, OwnedPcmSource, PcmSource, PeaksClearArgs,
    PositionInSeconds, PropertiesWindowArgs, ReaperStr, SaveStateArgs, SetAvailableArgs,
    SetFileNameArgs, SetSourceArgs,
};
use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// Only such methods are public which are allowed to use from real-time threads. Other ones
/// are private and called from the method that processes the incoming commands.
#[derive(Debug)]
pub struct Column {
    matrix_settings: OverridableMatrixSettings,
    settings: ColumnSettings,
    slots: Vec<Slot>,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
    command_receiver: Receiver<ColumnCommand>,
    event_sender: Sender<ColumnEvent>,
    /// Enough reserved memory to hold one audio block of an arbitrary size.
    mix_buffer_chunk: Vec<f64>,
    timeline_was_paused_in_last_block: bool,
}

#[derive(Clone, Debug)]
pub struct SharedColumn(Arc<Mutex<Column>>);

#[derive(Clone, Debug)]
pub struct WeakColumn(Weak<Mutex<Column>>);

impl SharedColumn {
    pub fn new(column_source: Column) -> Self {
        Self(Arc::new(Mutex::new(column_source)))
    }

    pub fn lock(&self) -> MutexGuard<Column> {
        non_blocking_lock(&self.0, "real-time column")
    }

    pub fn lock_allow_blocking(&self) -> MutexGuard<Column> {
        blocking_lock(&self.0)
    }

    pub fn downgrade(&self) -> WeakColumn {
        WeakColumn(Arc::downgrade(&self.0))
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl WeakColumn {
    pub fn upgrade(&self) -> Option<SharedColumn> {
        self.0.upgrade().map(SharedColumn)
    }
}

#[derive(Clone, Debug)]
pub struct ColumnCommandSender {
    command_sender: Sender<ColumnCommand>,
}

impl ColumnCommandSender {
    pub fn new(command_sender: Sender<ColumnCommand>) -> Self {
        Self { command_sender }
    }

    pub fn clear_slots(&self) {
        self.send_task(ColumnCommand::ClearSlots);
    }

    pub fn update_settings(&self, settings: ColumnSettings) {
        self.send_task(ColumnCommand::UpdateSettings(settings));
    }

    pub fn update_matrix_settings(&self, settings: OverridableMatrixSettings) {
        self.send_task(ColumnCommand::UpdateMatrixSettings(settings));
    }

    pub fn fill_slot_with_clip(&self, args: Box<Option<ColumnFillSlotArgs>>) {
        self.send_task(ColumnCommand::FillSlot(args));
    }

    pub fn process_transport_change(&self, args: ColumnProcessTransportChangeArgs) {
        self.send_task(ColumnCommand::ProcessTransportChange(args));
    }

    pub fn clear_slot(&self, slot_index: usize) {
        self.send_task(ColumnCommand::ClearSlot(slot_index));
    }

    pub fn play_slot(&self, args: ColumnPlaySlotArgs) {
        self.send_task(ColumnCommand::PlaySlot(args));
    }

    pub fn play_row(&self, args: ColumnPlayRowArgs) {
        self.send_task(ColumnCommand::PlayRow(args));
    }

    pub fn stop_slot(&self, args: ColumnStopSlotArgs) {
        self.send_task(ColumnCommand::StopSlot(args));
    }

    pub fn stop(&self, args: ColumnStopArgs) {
        self.send_task(ColumnCommand::Stop(args));
    }

    pub fn set_clip_looped(&self, args: ColumnSetClipLoopedArgs) {
        self.send_task(ColumnCommand::SetClipLooped(args));
    }

    pub fn pause_slot(&self, index: usize) {
        let args = ColumnPauseSlotArgs { index };
        self.send_task(ColumnCommand::PauseSlot(args));
    }

    pub fn seek_slot(&self, index: usize, desired_pos: UnitValue) {
        let args = ColumnSeekSlotArgs { index, desired_pos };
        self.send_task(ColumnCommand::SeekSlot(args));
    }

    pub fn set_clip_volume(&self, slot_index: usize, clip_index: usize, volume: Db) {
        let args = ColumnSetClipVolumeArgs {
            slot_index,
            clip_index,
            volume,
        };
        self.send_task(ColumnCommand::SetClipVolume(args));
    }

    pub fn set_clip_section(&self, slot_index: usize, clip_index: usize, section: api::Section) {
        let args = ColumnSetClipSectionArgs {
            slot_index,
            clip_index,
            section,
        };
        self.send_task(ColumnCommand::SetClipSection(args));
    }

    pub fn record_clip(&self, slot_index: usize, instruction: SlotRecordInstruction) {
        let args = ColumnRecordClipArgs {
            slot_index,
            instruction,
        };
        self.send_task(ColumnCommand::RecordClip(Box::new(Some(args))));
    }

    fn send_task(&self, task: ColumnCommand) {
        self.command_sender.try_send(task).unwrap();
    }
}

#[derive(Debug)]
pub enum ColumnCommand {
    ClearSlots,
    ClearSlot(usize),
    UpdateSettings(ColumnSettings),
    UpdateMatrixSettings(OverridableMatrixSettings),
    // Boxed because comparatively large.
    FillSlot(Box<Option<ColumnFillSlotArgs>>),
    ProcessTransportChange(ColumnProcessTransportChangeArgs),
    PlaySlot(ColumnPlaySlotArgs),
    PlayRow(ColumnPlayRowArgs),
    StopSlot(ColumnStopSlotArgs),
    Stop(ColumnStopArgs),
    PauseSlot(ColumnPauseSlotArgs),
    SeekSlot(ColumnSeekSlotArgs),
    SetClipVolume(ColumnSetClipVolumeArgs),
    SetClipLooped(ColumnSetClipLoopedArgs),
    SetClipSection(ColumnSetClipSectionArgs),
    RecordClip(Box<Option<ColumnRecordClipArgs>>),
}

pub trait ColumnEventSender {
    fn slot_play_state_changed(&self, slot_index: usize, play_state: InternalClipPlayState);

    fn clip_material_info_changed(
        &self,
        slot_index: usize,
        clip_index: usize,
        material_info: MaterialInfo,
    );

    fn slot_cleared(&self, slot_index: usize, clips: Vec<Clip>);

    fn record_request_acknowledged(
        &self,
        slot_index: usize,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    );

    fn midi_overdub_finished(&self, slot_index: usize, mirror_source: ClipSource);

    fn normal_recording_finished(&self, slot_index: usize, outcome: NormalRecordingOutcome);

    fn interaction_failed(&self, failure: InteractionFailure);

    fn dispose(&self, garbage: ColumnGarbage);

    fn send_event(&self, event: ColumnEvent);
}

impl ColumnEventSender for Sender<ColumnEvent> {
    fn slot_play_state_changed(&self, slot_index: usize, play_state: InternalClipPlayState) {
        let event = ColumnEvent::SlotPlayStateChanged {
            slot_index,
            play_state,
        };
        self.send_event(event);
    }

    fn clip_material_info_changed(
        &self,
        slot_index: usize,
        clip_index: usize,
        material_info: MaterialInfo,
    ) {
        let event = ColumnEvent::ClipMaterialInfoChanged {
            slot_index,
            clip_index,
            material_info,
        };
        self.send_event(event);
    }

    fn slot_cleared(&self, slot_index: usize, clips: Vec<Clip>) {
        let event = ColumnEvent::SlotCleared { slot_index, clips };
        self.send_event(event);
    }

    fn record_request_acknowledged(
        &self,
        slot_index: usize,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    ) {
        let event = ColumnEvent::RecordRequestAcknowledged { slot_index, result };
        self.send_event(event);
    }

    fn midi_overdub_finished(&self, slot_index: usize, mirror_source: ClipSource) {
        let event = ColumnEvent::MidiOverdubFinished {
            slot_index,
            mirror_source,
        };
        self.send_event(event);
    }

    fn normal_recording_finished(&self, slot_index: usize, outcome: NormalRecordingOutcome) {
        let event = ColumnEvent::NormalRecordingFinished {
            slot_index,
            outcome,
        };
        self.send_event(event);
    }

    fn dispose(&self, garbage: ColumnGarbage) {
        self.send_event(ColumnEvent::Dispose(garbage));
    }

    fn interaction_failed(&self, failure: InteractionFailure) {
        self.send_event(ColumnEvent::InteractionFailed(failure));
    }

    fn send_event(&self, event: ColumnEvent) {
        self.try_send(event).unwrap();
    }
}

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    pub clip_play_start_timing: Option<ClipPlayStartTiming>,
    pub clip_play_stop_timing: Option<ClipPlayStopTiming>,
    pub audio_time_stretch_mode: Option<AudioTimeStretchMode>,
    pub audio_resample_mode: Option<VirtualResampleMode>,
    pub audio_cache_behavior: Option<AudioCacheBehavior>,
    pub play_mode: ColumnPlayMode,
}

#[derive(Clone, Debug, Default)]
pub struct OverridableMatrixSettings {
    pub clip_play_start_timing: ClipPlayStartTiming,
    pub clip_play_stop_timing: ClipPlayStopTiming,
    pub audio_time_stretch_mode: AudioTimeStretchMode,
    pub audio_resample_mode: VirtualResampleMode,
    pub audio_cache_behavior: AudioCacheBehavior,
}

const MAX_AUDIO_CHANNEL_COUNT: usize = 64;
const MAX_BLOCK_SIZE: usize = 2048;

/// At the time of this writing, a slot is just around 900 byte, so 100 slots take roughly 90 kB.
const MAX_SLOT_COUNT_WITHOUT_REALLOCATION: usize = 100;

impl Column {
    pub fn new(
        permanent_project: Option<Project>,
        command_receiver: Receiver<ColumnCommand>,
        event_sender: Sender<ColumnEvent>,
    ) -> Self {
        debug!("Slot size: {}", std::mem::size_of::<Slot>());
        Self {
            matrix_settings: Default::default(),
            settings: Default::default(),
            slots: Vec::with_capacity(MAX_SLOT_COUNT_WITHOUT_REALLOCATION),
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

    fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        let material_info = args.clip.material_info().unwrap();
        let clip_index =
            get_slot_mut_insert(&mut self.slots, args.slot_index).fill(args.clip, args.mode);
        self.event_sender
            .clip_material_info_changed(args.slot_index, clip_index, material_info);
    }

    pub fn slot(&self, index: usize) -> ClipEngineResult<&Slot> {
        get_slot(&self.slots, index)
    }

    pub fn slot_mut(&mut self, index: usize) -> ClipEngineResult<&mut Slot> {
        self.slots.get_mut(index).ok_or(SLOT_DOESNT_EXIST)
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
        let slot = get_slot_mut_insert(&mut self.slots, args.slot_index);
        if slot.is_filled() {
            slot.play(slot_args)?;
            if self.settings.play_mode.is_exclusive() {
                self.stop_all_clips(
                    audio_request_props,
                    ref_pos,
                    &args.timeline,
                    Some(args.slot_index),
                );
            }
            Ok(())
        } else if args.options.stop_column_if_slot_empty {
            self.stop_all_clips(audio_request_props, ref_pos, &args.timeline, None);
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
            );
        }
        let play_args = ColumnPlaySlotArgs {
            slot_index: args.slot_index,
            timeline: args.timeline,
            ref_pos: Some(args.ref_pos),
            options: ColumnPlayClipOptions {
                stop_column_if_slot_empty: true,
                start_timing: None,
            },
        };
        self.play_slot(play_args, audio_request_props)
    }

    pub fn stop(&mut self, args: ColumnStopArgs, audio_request_props: BasicAudioRequestProps) {
        let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
        self.stop_all_clips(audio_request_props, ref_pos, &args.timeline, None);
    }

    fn stop_all_clips(
        &mut self,
        audio_request_props: BasicAudioRequestProps,
        ref_pos: PositionInSeconds,
        timeline: &HybridTimeline,
        except: Option<usize>,
    ) {
        for (i, slot) in self
            .slots
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| except.map(|e| e != *i).unwrap_or(true))
        {
            let stop_args = SlotStopArgs {
                stop_timing: None,
                timeline,
                ref_pos: Some(ref_pos),
                enforce_play_stop: true,
                matrix_settings: &self.matrix_settings,
                column_settings: &self.settings,
                audio_request_props,
            };
            let event_handler = ClipEventHandler::new(&self.event_sender, i);
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
        let event_handler = ClipEventHandler::new(&self.event_sender, args.slot_index);
        slot.stop(clip_args, &event_handler)
    }

    pub fn set_clip_looped(&mut self, args: ColumnSetClipLoopedArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.slot_index)
            .get_clip_mut(args.clip_index)?
            .set_looped(args.looped)
    }

    pub fn set_clip_section(&mut self, args: ColumnSetClipSectionArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.slot_index)
            .get_clip_mut(args.clip_index)?
            .set_section(args.section)
    }

    /// See [`Clip::recording_poll`].
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
                let event_handler = ClipEventHandler::new(&self.event_sender, slot_index);
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
        let slot = get_slot_mut_insert(&mut self.slots, slot_index);
        let result = slot.record_clip(instruction, &self.matrix_settings, &self.settings);
        let (informative_result, ack_result) = match result {
            Ok(slot_runtime_data) => {
                if self.settings.play_mode.is_exclusive() {
                    let timeline = clip_timeline(self.project, false);
                    let ref_pos = timeline.cursor_pos();
                    self.stop_all_clips(audio_request_props, ref_pos, &timeline, Some(slot_index));
                }
                (Ok(()), Ok(slot_runtime_data))
            }
            Err(e) => (Err(e.message), Err(e.payload)),
        };
        self.event_sender
            .record_request_acknowledged(slot_index, ack_result);
        informative_result
    }

    pub fn is_stoppable(&self) -> bool {
        self.slots.iter().any(|slot| slot.is_stoppable())
    }

    pub fn pause_slot(&mut self, args: ColumnPauseSlotArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.index).pause()
    }

    fn seek_clip(&mut self, args: ColumnSeekSlotArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.index).seek(args.desired_pos)
    }

    pub fn write_clip_midi(
        &mut self,
        index: usize,
        request: WriteMidiRequest,
    ) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).write_clip_midi(request)
    }

    pub fn write_clip_audio(
        &mut self,
        slot_index: usize,
        request: impl WriteAudioRequest,
    ) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, slot_index).write_clip_audio(request)
    }

    fn set_clip_volume(&mut self, args: ColumnSetClipVolumeArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.slot_index)
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
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let event_handler = ClipEventHandler::new(&self.event_sender, i);
            let _ = slot.process_transport_change(&args, &event_handler);
        }
    }

    fn duration(&self) -> DurationInSeconds {
        DurationInSeconds::MAX
    }

    pub fn clear_slots(&mut self) {
        self.slots.clear();
    }

    pub fn clear_slot(&mut self, index: usize) -> ClipEngineResult<()> {
        let slot = get_slot_mut(&mut self.slots, index)?;
        let event_handler = ClipEventHandler::new(&self.event_sender, index);
        slot.clear(&event_handler)?;
        Ok(())
    }

    fn process_commands(&mut self, audio_request_props: BasicAudioRequestProps) {
        while let Ok(task) = self.command_receiver.try_recv() {
            use ColumnCommand::*;
            match task {
                ClearSlots => {
                    self.clear_slots();
                }
                ClearSlot(slot_index) => {
                    let result = self.clear_slot(slot_index);
                    self.notify_user_about_failed_interaction(result);
                }
                UpdateSettings(s) => {
                    self.settings = s;
                }
                UpdateMatrixSettings(s) => {
                    self.matrix_settings = s;
                }
                FillSlot(mut boxed_args) => {
                    let args = boxed_args.take().unwrap();
                    self.fill_slot(args);
                    self.event_sender
                        .dispose(ColumnGarbage::FillSlotArgs(boxed_args));
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
                Stop(args) => {
                    self.stop(args, audio_request_props);
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
                        .dispose(ColumnGarbage::RecordClipArgs(boxed_args));
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
            let output_channel_count = args.block.nch() as usize;
            let output_frame_count = args.block.length() as usize;
            let mut output_buffer = unsafe {
                AudioBufMut::from_raw(
                    args.block.samples(),
                    output_channel_count,
                    output_frame_count,
                )
            };
            // rt_debug!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
            //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
            for (row, slot) in self.slots.iter_mut().enumerate() {
                // Our strategy is to always write all available source channels into the mix
                // buffer. From a performance perspective, it would actually be enough to take
                // only as many channels as we need (= track channel count). However, always using
                // the source channel count as reference is much simpler, in particular when it
                // comes to caching and pre-buffering. Also, in practice this is rarely an issue.
                // Most samples out there used in typical stereo track setups have no more than 2
                // channels. And if they do, the user can always down-mix to the desired channel
                // count up-front.
                let clip_count = slot.clip_count();
                if clip_count == 0 {
                    // If the slot doesn't have any clip, there's nothing useful it can process.
                    continue;
                }
                // TODO-high-clip-engine We should move this logic to the slot. It's not just for better
                //  encapsulation but also for more performance (maybe we can use iterators
                //  instead of bound checks).
                for i in 0..clip_count {
                    let clip_channel_count = {
                        let clip = slot.find_clip(i).unwrap();
                        match clip.material_info() {
                            Ok(info) => info.channel_count(),
                            // If the clip doesn't have material, it's probably recording. We still
                            // allow the slot to process because it could propagate some play state
                            // changes. With a channel count of zero though.
                            Err(_) => 0,
                        }
                    };
                    let mut mix_buffer = AudioBufMut::from_slice(
                        &mut self.mix_buffer_chunk,
                        clip_channel_count,
                        output_frame_count,
                    )
                    .unwrap();
                    let mut inner_args = ClipProcessArgs {
                        clip_index: i,
                        dest_buffer: &mut mix_buffer,
                        dest_sample_rate: args.block.sample_rate(),
                        midi_event_list: args
                            .block
                            .midi_event_list_mut()
                            .expect("no MIDI event list available"),
                        timeline: &timeline,
                        timeline_cursor_pos,
                        timeline_tempo,
                        resync,
                        matrix_settings: &self.matrix_settings,
                        column_settings: &self.settings,
                    };
                    let event_handler = ClipEventHandler::new(&self.event_sender, row);
                    if let Ok(outcome) = slot.process(&mut inner_args, &event_handler) {
                        if outcome.num_audio_frames_written > 0 {
                            output_buffer
                                .slice_mut(0..outcome.num_audio_frames_written)
                                .modify_frames(|sample| {
                                    // TODO-high-performance This is a hot code path. We might want to skip bound checks
                                    //  in sample_value_at().
                                    if sample.index.channel < clip_channel_count {
                                        sample.value
                                            + mix_buffer.sample_value_at(sample.index).unwrap()
                                    } else {
                                        // Clip doesn't have material on this channel.
                                        0.0
                                    }
                                })
                        }
                        if let Some(changed_play_state) = outcome.changed_play_state {
                            self.event_sender
                                .slot_play_state_changed(row, changed_play_state);
                        }
                    }
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

impl CustomPcmSource for SharedColumn {
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

#[derive(Debug)]
pub struct ColumnFillSlotArgs {
    pub slot_index: usize,
    pub clip: Clip,
    pub mode: FillClipMode,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FillClipMode {
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
    pub options: ColumnPlayClipOptions,
}

#[derive(Clone, Debug)]
pub struct ColumnPlayRowArgs {
    pub slot_index: usize,
    pub timeline: HybridTimeline,
    pub ref_pos: PositionInSeconds,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnPlayClipOptions {
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

fn get_slot(slots: &[Slot], index: usize) -> ClipEngineResult<&Slot> {
    slots.get(index).ok_or(SLOT_DOESNT_EXIST)
}

fn get_slot_mut(slots: &mut [Slot], index: usize) -> ClipEngineResult<&mut Slot> {
    slots.get_mut(index).ok_or(SLOT_DOESNT_EXIST)
}

const SLOT_DOESNT_EXIST: &str = "slot doesn't exist";

fn get_slot_mut_insert(slots: &mut Vec<Slot>, index: usize) -> &mut Slot {
    if index >= slots.len() {
        slots.resize_with(index + 1, Default::default);
    }
    slots.get_mut(index).unwrap()
}

#[derive(Debug)]
pub enum ColumnEvent {
    SlotPlayStateChanged {
        slot_index: usize,
        play_state: InternalClipPlayState,
    },
    ClipMaterialInfoChanged {
        slot_index: usize,
        clip_index: usize,
        material_info: MaterialInfo,
    },
    SlotCleared {
        slot_index: usize,
        clips: Vec<Clip>,
    },
    RecordRequestAcknowledged {
        slot_index: usize,
        /// Slot runtime data is returned only if it's a recording from scratch (slot was not
        /// filled before).
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    },
    MidiOverdubFinished {
        slot_index: usize,
        mirror_source: ClipSource,
    },
    NormalRecordingFinished {
        slot_index: usize,
        outcome: NormalRecordingOutcome,
    },
    Dispose(ColumnGarbage),
    InteractionFailed(InteractionFailure),
}

#[derive(Debug)]
pub struct InteractionFailure {
    pub message: &'static str,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ColumnGarbage {
    FillSlotArgs(Box<Option<ColumnFillSlotArgs>>),
    Clip(Clip),
    RecordClipArgs(Box<Option<ColumnRecordClipArgs>>),
}

struct ClipEventHandler<'a> {
    slot_index: usize,
    event_sender: &'a Sender<ColumnEvent>,
}

impl<'a> ClipEventHandler<'a> {
    pub fn new(event_sender: &'a Sender<ColumnEvent>, slot_index: usize) -> Self {
        Self {
            slot_index,
            event_sender,
        }
    }
}

impl<'a> HandleSlotEvent for ClipEventHandler<'a> {
    fn midi_overdub_finished(&self, mirror_source: ClipSource) {
        self.event_sender
            .midi_overdub_finished(self.slot_index, mirror_source);
    }

    fn normal_recording_finished(&self, outcome: NormalRecordingOutcome) {
        self.event_sender
            .normal_recording_finished(self.slot_index, outcome);
    }

    fn slot_cleared(&self, clips: Vec<Clip>) {
        self.event_sender.slot_cleared(self.slot_index, clips);
    }
}

#[derive(Clone, Debug)]
pub struct ColumnProcessTransportChangeArgs {
    pub change: TransportChange,
    pub timeline: HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub audio_request_props: BasicAudioRequestProps,
}
