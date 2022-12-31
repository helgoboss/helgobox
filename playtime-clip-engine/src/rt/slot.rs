use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::metrics_util::measure_time;
use crate::rt::supplier::{MaterialInfo, WriteAudioRequest, WriteMidiRequest};
use crate::rt::{
    Clip, ClipProcessArgs, ClipRecordingPollArgs, ColumnProcessTransportChangeArgs, ColumnSettings,
    FillClipMode, HandleSlotEvent, InternalClipPlayState, OverridableMatrixSettings, SharedPeak,
    SharedPos, SlotInstruction, SlotPlayArgs, SlotRecordInstruction, SlotStopArgs,
};
use crate::{ClipEngineResult, ErrorWithPayload};
use helgoboss_learn::UnitValue;
use playtime_api::persistence::ClipPlayStopTiming;
use playtime_api::runtime::ClipPlayState;
use reaper_medium::{Bpm, PlayState, PositionInSeconds};
use std::mem;

#[derive(Debug, Default)]
pub struct Slot {
    clips: Vec<Clip>,
    runtime_data: InternalRuntimeData,
}

#[derive(Debug, Default)]
struct InternalRuntimeData {
    last_play_state: InternalClipPlayState,
    stop_was_caused_by_transport_change: bool,
}

impl Slot {
    /// Returns the index at which the clip landed.
    pub fn fill(&mut self, clip: Clip, mode: FillClipMode) -> usize {
        // TODO-medium Suspend previous clip if playing.
        match mode {
            FillClipMode::Add => {
                self.clips.push(clip);
                self.clips.len() - 1
            }
            FillClipMode::Replace => {
                self.clips.clear();
                self.clips.push(clip);
                0
            }
        }
    }

    pub fn is_filled(&self) -> bool {
        !self.clips.is_empty()
    }

    pub fn find_clip(&self, index: usize) -> Option<&Clip> {
        self.clips.get(index)
    }

    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// See [`Clip::recording_poll`].
    pub fn recording_poll<H: HandleSlotEvent>(
        &mut self,
        args: ClipRecordingPollArgs,
        event_handler: &H,
    ) -> bool {
        match self.get_clip_mut(0) {
            Ok(clip) => clip.recording_poll(args, event_handler),
            Err(_) => false,
        }
    }

    /// Plays all clips in this slot.
    pub fn play(&mut self, args: SlotPlayArgs) -> ClipEngineResult<()> {
        for clip in self.get_clips_mut()? {
            clip.play(args)?;
        }
        Ok(())
    }

    /// Stops all clips in this slot.
    pub fn stop<H: HandleSlotEvent>(
        &mut self,
        args: SlotStopArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        self.runtime_data.stop_was_caused_by_transport_change = false;
        let mut instruction = None;
        for clip in self.get_clips_mut()? {
            let inst = clip.stop(args, event_handler)?;
            if let Some(inst) = inst {
                instruction = Some(inst);
            }
        }
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
        if self.clips.is_empty() {
            return Err("already empty");
        }
        let mut all_clips_removed = true;
        for clip in &mut self.clips {
            if !clip.initiate_removal()? {
                all_clips_removed = false;
            }
        }
        if all_clips_removed {
            self.clear_internal(event_handler);
        }
        Ok(())
    }

    fn clear_internal<H: HandleSlotEvent>(&mut self, event_handler: &H) {
        debug!("Clearing real-time slot");
        if self.clips.is_empty() {
            return;
        }
        let old_clips = mem::replace(&mut self.clips, vec![]);
        event_handler.slot_cleared(old_clips);
        self.runtime_data = InternalRuntimeData::default();
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
                if !self.clips.is_empty() {
                    return Err(ErrorWithPayload::new(
                        "slot not empty",
                        NewClip(instruction),
                    ));
                }
                let clip = Clip::recording(instruction);
                let runtime_data = SlotRuntimeData::from_recording_clip(&clip);
                self.clips.push(clip);
                Ok(Some(runtime_data))
            }
            ExistingClip(args) => {
                debug!("Record with existing clip");
                let clip = match self.clips.first_mut() {
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
                let clip = match self.clips.first_mut() {
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

    pub fn pause(&mut self) -> ClipEngineResult<()> {
        for clip in self.get_clips_mut()? {
            clip.pause();
        }
        Ok(())
    }

    pub fn seek(&mut self, desired_pos: UnitValue) -> ClipEngineResult<()> {
        for clip in self.get_clips_mut()? {
            clip.seek(desired_pos)?;
        }
        Ok(())
    }

    pub fn write_clip_midi(&mut self, request: WriteMidiRequest) -> ClipEngineResult<()> {
        self.get_clip_mut(0)?.write_midi(request);
        Ok(())
    }

    pub fn write_clip_audio(&mut self, request: impl WriteAudioRequest) -> ClipEngineResult<()> {
        self.get_clip_mut(0)?.write_audio(request);
        Ok(())
    }

    pub fn get_clip_mut(&mut self, index: usize) -> ClipEngineResult<&mut Clip> {
        self.clips.get_mut(index).ok_or(CLIP_DOESNT_EXIST)
    }

    pub fn process_transport_change<H: HandleSlotEvent>(
        &mut self,
        args: &SlotProcessTransportChangeArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        let mut instruction = None;
        {
            for clip in &mut self.clips {
                let inst = match args.column_args.change {
                    TransportChange::PlayState(rel_change) => {
                        // We have a relevant transport change.
                        let state = clip.play_state();
                        use ClipPlayState::*;
                        use RelevantPlayStateChange::*;
                        match rel_change {
                            PlayAfterStop => {
                                match state.get() {
                                    Stopped
                                        if self
                                            .runtime_data
                                            .stop_was_caused_by_transport_change =>
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
                            StopAfterPlay => match state.get() {
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
                            play_state.get(),
                            ScheduledForPlayStart | Playing | ScheduledForPlayStop
                        ) {
                            return Ok(());
                        }
                        play_clip_by_transport(clip, args)
                    }
                };
                if let Some(inst) = inst {
                    // Right now there's only one instruction we can have, so this is okay.
                    instruction = Some(inst);
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
            let clip = self.get_clip_mut(args.clip_index)?;
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
        self.clips.iter().any(|c| c.play_state().is_stoppable())
    }

    fn get_clips_mut(&mut self) -> ClipEngineResult<&mut [Clip]> {
        if self.clips.is_empty() {
            return Err(SLOT_NOT_FILLED);
        }
        Ok(&mut self.clips)
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
        let args = SlotStopArgs {
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
const CLIP_DOESNT_EXIST: &str = "clip doesn't exist";

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
    pub changed_play_state: Option<InternalClipPlayState>,
    pub num_audio_frames_written: usize,
}

fn play_clip_by_transport(
    clip: &mut Clip,
    args: &SlotProcessTransportChangeArgs,
) -> Option<SlotInstruction> {
    let args = SlotPlayArgs {
        timeline: &args.column_args.timeline,
        ref_pos: Some(args.column_args.timeline_cursor_pos),
        matrix_settings: args.matrix_settings,
        column_settings: args.column_settings,
        start_timing: None,
    };
    clip.play(args).unwrap();
    None
}

#[derive(Clone, Debug)]
pub struct SlotRuntimeData {
    pub play_state: InternalClipPlayState,
    pub pos: SharedPos,
    pub peak: SharedPeak,
    /// The frame count in this material info is supposed to take the section bounds into account.
    pub material_info: MaterialInfo,
}

impl SlotRuntimeData {
    pub fn from_recording_clip(clip: &Clip) -> Self {
        Self {
            play_state: clip.play_state(),
            pos: clip.shared_pos(),
            peak: clip.shared_peak(),
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

    pub fn proportional_position(&self) -> ClipEngineResult<UnitValue> {
        let pos = self.pos.get();
        if pos < 0 {
            return Err("count-in phase");
        }
        let frame_count = self.material_info.frame_count();
        if frame_count == 0 {
            return Err("frame count is zero");
        }
        let mod_pos = pos as usize % frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Ok(proportional)
    }

    pub fn position_in_seconds_during_recording(&self, timeline_tempo: Bpm) -> PositionInSeconds {
        let tempo_factor = self
            .material_info
            .tempo_factor_during_recording(timeline_tempo);
        self.position_in_seconds(tempo_factor)
    }

    pub fn position_in_seconds(&self, tempo_factor: f64) -> PositionInSeconds {
        let pos_in_source_frames = self.mod_frame();
        let pos_in_secs = convert_position_in_frames_to_seconds(
            pos_in_source_frames,
            self.material_info.frame_rate(),
        );
        adjust_pos_in_secs_anti_proportionally(pos_in_secs, tempo_factor)
    }

    pub fn peak(&self) -> UnitValue {
        self.peak.reset()
    }
}
