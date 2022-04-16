use crate::metrics_util::measure_time;
use crate::rt::supplier::{MaterialInfo, WriteAudioRequest, WriteMidiRequest};
use crate::rt::{
    Clip, ClipPlayArgs, ClipPlayState, ClipProcessArgs, ClipRecordingPollArgs, ClipStopArgs,
    ColumnProcessTransportChangeArgs, ColumnSettings, HandleSlotEvent, OverridableMatrixSettings,
    SharedPos, SlotInstruction, SlotRecordInstruction,
};
use crate::{ClipEngineResult, ErrorWithPayload};
use helgoboss_learn::UnitValue;
use playtime_api::{ClipPlayStopTiming, Db};
use reaper_medium::PlayState;

#[derive(Debug, Default)]
pub struct Slot {
    clip: Option<Clip>,
    runtime_data: InternalRuntimeData,
}

#[derive(Debug, Default)]
struct InternalRuntimeData {
    last_play_state: ClipPlayState,
    stop_was_caused_by_transport_change: bool,
}

impl Slot {
    pub fn fill(&mut self, clip: Clip) {
        // TODO-medium Suspend previous clip if playing.
        self.clip = Some(clip);
    }

    pub fn is_filled(&self) -> bool {
        self.clip.is_some()
    }

    pub fn clip(&self) -> ClipEngineResult<&Clip> {
        self.clip_internal()
    }

    /// See [`Clip::recording_poll`].
    pub fn recording_poll<H: HandleSlotEvent>(
        &mut self,
        args: ClipRecordingPollArgs,
        event_handler: &H,
    ) -> bool {
        match self.clip_mut_internal() {
            Ok(clip) => clip.recording_poll(args, event_handler),
            Err(_) => false,
        }
    }

    pub fn clip_mut(&mut self) -> ClipEngineResult<&mut Clip> {
        self.clip_mut_internal()
    }

    /// Plays the clip if this slot contains one.
    pub fn play_clip(&mut self, args: ClipPlayArgs) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.play(args)?;
        Ok(())
    }

    /// Stops the clip if this slot contains one.
    pub fn stop_clip<H: HandleSlotEvent>(
        &mut self,
        args: ClipStopArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        self.runtime_data.stop_was_caused_by_transport_change = false;
        let instruction = self.clip_mut_internal()?.stop(args, event_handler)?;
        if let Some(instruction) = instruction {
            self.process_instruction(instruction, event_handler);
        }
        Ok(())
    }

    fn process_instruction<H: HandleSlotEvent>(
        &mut self,
        instruction: SlotInstruction,
        event_handler: &H,
    ) {
        use SlotInstruction::*;
        match instruction {
            ClearSlot => {
                self.clear_internal(event_handler);
            }
        }
    }

    pub fn clear<H: HandleSlotEvent>(&mut self, event_handler: &H) -> ClipEngineResult<()> {
        let clip = match &mut self.clip {
            None => return Err("already empty"),
            Some(c) => c,
        };
        if clip.initiate_removal()? {
            self.clear_internal(event_handler);
        }
        Ok(())
    }

    fn clear_internal<H: HandleSlotEvent>(&mut self, event_handler: &H) {
        debug!("Clearing real-time slot");
        if let Some(clip) = self.clip.take() {
            event_handler.slot_cleared(clip);
        };
        self.runtime_data = InternalRuntimeData::default();
    }

    pub fn set_clip_looped(&mut self, repeated: bool) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.set_looped(repeated)
    }

    /// # Errors
    ///
    /// Returns an error either if the instruction is to record on the given new clip but the slot
    /// is not empty, or if the instruction is to record on an existing clip but the slot is empty.
    ///
    /// In both cases, it returns the instruction itself so it can be disposed appropriately.
    pub fn record_clip(
        &mut self,
        instruction: SlotRecordInstruction,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
    ) -> Result<Option<SlotRuntimeData>, ErrorWithPayload<SlotRecordInstruction>> {
        use SlotRecordInstruction::*;
        match instruction {
            NewClip(instruction) => {
                debug!("Record new clip");
                if self.clip.is_some() {
                    return Err(ErrorWithPayload::new(
                        "slot not empty",
                        NewClip(instruction),
                    ));
                }
                let clip = Clip::recording(instruction);
                let runtime_data = SlotRuntimeData::from_recording_clip(&clip);
                self.clip = Some(clip);
                Ok(Some(runtime_data))
            }
            ExistingClip(args) => {
                debug!("Record with existing clip");
                let clip = match self.clip.as_mut() {
                    None => {
                        return Err(ErrorWithPayload::new("slot empty", ExistingClip(args)));
                    }
                    Some(c) => c,
                };
                match clip.record(args, matrix_settings, column_settings) {
                    Ok(_) => {
                        let runtime_data = SlotRuntimeData::from_recording_clip(clip);
                        Ok(Some(runtime_data))
                    }
                    Err(e) => Err(e.map_payload(ExistingClip)),
                }
            }
            MidiOverdub(instruction) => {
                debug!("MIDI overdub");
                let clip = match self.clip.as_mut() {
                    None => {
                        return Err(ErrorWithPayload::new(
                            "slot empty",
                            MidiOverdub(instruction),
                        ));
                    }
                    Some(c) => c,
                };
                match clip.midi_overdub(instruction) {
                    Ok(_) => Ok(None),
                    Err(e) => Err(e.map_payload(MidiOverdub)),
                }
            }
        }
    }

    pub fn pause_clip(&mut self) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.pause();
        Ok(())
    }

    pub fn seek_clip(&mut self, desired_pos: UnitValue) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.seek(desired_pos)
    }

    pub fn write_clip_midi(&mut self, request: WriteMidiRequest) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.write_midi(request);
        Ok(())
    }

    pub fn write_clip_audio(&mut self, request: impl WriteAudioRequest) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.write_audio(request);
        Ok(())
    }

    pub fn set_clip_volume(&mut self, volume: Db) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.set_volume(volume);
        Ok(())
    }

    pub fn process_transport_change<H: HandleSlotEvent>(
        &mut self,
        args: &SlotProcessTransportChangeArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        let instruction = {
            let clip = match &mut self.clip {
                None => return Ok(()),
                Some(c) => c,
            };
            match args.column_args.change {
                TransportChange::PlayState(rel_change) => {
                    // We have a relevant transport change.
                    let state = clip.play_state();
                    use ClipPlayState::*;
                    use RelevantPlayStateChange::*;
                    match rel_change {
                        PlayAfterStop => {
                            match state {
                                Stopped
                                    if self.runtime_data.stop_was_caused_by_transport_change =>
                                {
                                    // REAPER transport was started from stopped state. Clip is stopped
                                    // as well and was put in that state due to a previous transport
                                    // stop. Play the clip!
                                    play_clip_by_transport(clip, args)
                                }
                                ScheduledForPlayStart | Playing | ScheduledForPlayStop => {
                                    // Retrigger (timeline switch)
                                    play_clip_by_transport(clip, args)
                                }
                                Stopped
                                | Paused
                                | Recording
                                | ScheduledForRecordingStart
                                | ScheduledForRecordingStop => {
                                    // Stop and forget.
                                    self.runtime_data.stop_clip_by_transport(
                                        clip,
                                        args,
                                        false,
                                        event_handler,
                                    )?
                                }
                            }
                        }
                        StopAfterPlay => match state {
                            ScheduledForPlayStart
                            | Playing
                            | ScheduledForPlayStop
                            | Recording
                            | ScheduledForRecordingStart
                            | ScheduledForRecordingStop => {
                                // Stop and memorize
                                self.runtime_data.stop_clip_by_transport(
                                    clip,
                                    args,
                                    true,
                                    event_handler,
                                )?
                            }

                            Stopped | Paused => {
                                // Stop and forget
                                self.runtime_data.stop_clip_by_transport(
                                    clip,
                                    args,
                                    false,
                                    event_handler,
                                )?
                            }
                        },
                        StopAfterPause => self.runtime_data.stop_clip_by_transport(
                            clip,
                            args,
                            false,
                            event_handler,
                        )?,
                    }
                }
                TransportChange::PlayCursorJump => {
                    // The play cursor was repositioned.
                    let play_state = clip.play_state();
                    use ClipPlayState::*;
                    if !matches!(
                        play_state,
                        ScheduledForPlayStart | Playing | ScheduledForPlayStop
                    ) {
                        return Ok(());
                    }
                    clip.play(ClipPlayArgs {
                        timeline: &args.column_args.timeline,
                        ref_pos: Some(args.column_args.timeline_cursor_pos),
                        matrix_settings: args.matrix_settings,
                        column_settings: args.column_settings,
                    })?;
                    None
                }
            }
        };
        if let Some(instruction) = instruction {
            self.process_instruction(instruction, event_handler);
        }
        Ok(())
    }

    pub fn process<H: HandleSlotEvent>(
        &mut self,
        args: &mut ClipProcessArgs,
        event_handler: &H,
    ) -> ClipEngineResult<SlotProcessingOutcome> {
        measure_time("slot.process.time", || {
            let clip = self.clip_mut_internal()?;
            let clip_outcome = clip.process(args);
            let changed_play_state = if clip_outcome.clear_slot {
                self.clear_internal(event_handler);
                None
            } else {
                let play_state = clip.play_state();
                let last_play_state = self.runtime_data.last_play_state;
                if play_state == last_play_state {
                    None
                } else {
                    debug!("Clip state changed: {:?}", play_state);
                    self.runtime_data.last_play_state = play_state;
                    Some(play_state)
                }
            };
            let outcome = SlotProcessingOutcome {
                changed_play_state,
                num_audio_frames_written: clip_outcome.num_audio_frames_written,
            };
            Ok(outcome)
        })
    }

    pub fn is_stoppable(&self) -> bool {
        self.clip
            .as_ref()
            .map(|c| c.play_state().is_stoppable())
            .unwrap_or(false)
    }

    fn clip_internal(&self) -> ClipEngineResult<&Clip> {
        self.clip.as_ref().ok_or(SLOT_NOT_FILLED)
    }

    fn clip_mut_internal(&mut self) -> ClipEngineResult<&mut Clip> {
        self.clip.as_mut().ok_or(SLOT_NOT_FILLED)
    }
}

impl InternalRuntimeData {
    fn stop_clip_by_transport<H: HandleSlotEvent>(
        &mut self,
        clip: &mut Clip,
        args: &SlotProcessTransportChangeArgs,
        keep_starting_with_transport: bool,
        event_handler: &H,
    ) -> ClipEngineResult<Option<SlotInstruction>> {
        self.stop_was_caused_by_transport_change = keep_starting_with_transport;
        let args = ClipStopArgs {
            stop_timing: Some(ClipPlayStopTiming::Immediately),
            timeline: &args.column_args.timeline,
            ref_pos: Some(args.column_args.timeline_cursor_pos),
            enforce_play_stop: true,
            matrix_settings: args.matrix_settings,
            column_settings: args.column_settings,
            audio_request_props: args.column_args.audio_request_props,
        };
        clip.stop(args, event_handler)
    }
}

#[derive(Clone, Debug)]
pub struct SlotProcessTransportChangeArgs<'a> {
    pub column_args: &'a ColumnProcessTransportChangeArgs,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a ColumnSettings,
}

const SLOT_NOT_FILLED: &str = "slot not filled";

#[derive(Copy, Clone, Debug)]
pub enum TransportChange {
    PlayState(RelevantPlayStateChange),
    PlayCursorJump,
}
#[derive(Copy, Clone, Debug)]
pub enum RelevantPlayStateChange {
    PlayAfterStop,
    StopAfterPlay,
    StopAfterPause,
}

impl RelevantPlayStateChange {
    pub fn from_play_state_change(old: PlayState, new: PlayState) -> Option<Self> {
        use RelevantPlayStateChange::*;
        let change = if !old.is_paused && !old.is_playing && new.is_playing {
            PlayAfterStop
        } else if old.is_playing && !new.is_playing && !new.is_paused {
            StopAfterPlay
        } else if old.is_paused && !new.is_playing && !new.is_paused {
            StopAfterPause
        } else {
            return None;
        };
        Some(change)
    }
}

pub struct SlotProcessingOutcome {
    pub changed_play_state: Option<ClipPlayState>,
    pub num_audio_frames_written: usize,
}

fn play_clip_by_transport(
    clip: &mut Clip,
    args: &SlotProcessTransportChangeArgs,
) -> Option<SlotInstruction> {
    let args = ClipPlayArgs {
        timeline: &args.column_args.timeline,
        ref_pos: Some(args.column_args.timeline_cursor_pos),
        matrix_settings: args.matrix_settings,
        column_settings: args.column_settings,
    };
    clip.play(args).unwrap();
    None
}

#[derive(Clone, Debug)]
pub struct SlotRuntimeData {
    pub play_state: ClipPlayState,
    pub pos: SharedPos,
    pub material_info: MaterialInfo,
}

impl SlotRuntimeData {
    pub fn from_recording_clip(clip: &Clip) -> Self {
        Self {
            play_state: clip.play_state(),
            pos: clip.shared_pos(),
            material_info: clip
                .recording_material_info()
                .expect("recording clip should return recording material info"),
        }
    }

    pub fn mod_frame(&self) -> isize {
        let frame = self.pos.get();
        if frame < 0 {
            frame
        } else if self.material_info.frame_count() > 0 {
            frame % self.material_info.frame_count() as isize
        } else {
            0
        }
    }
}
