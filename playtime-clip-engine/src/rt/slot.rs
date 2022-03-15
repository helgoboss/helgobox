use crate::metrics_util::measure_time;
use crate::rt::supplier::{WriteAudioRequest, WriteMidiRequest};
use crate::rt::StopSlotInstruction::KeepSlot;
use crate::rt::{
    Clip, ClipPlayArgs, ClipPlayState, ClipProcessArgs, ClipRecordingPollArgs, ClipStopArgs,
    ColumnProcessTransportChangeArgs, ColumnSettings, HandleStopEvent, OverridableMatrixSettings,
    SlotRecordInstruction, StopSlotInstruction,
};
use crate::{ClipEngineResult, ErrorWithPayload};
use helgoboss_learn::UnitValue;
use playtime_api::{ClipPlayStopTiming, Db};
use reaper_medium::{Bpm, PlayState};

#[derive(Debug, Default)]
pub struct Slot {
    clip: Option<Clip>,
    runtime_data: RuntimeData,
}

#[derive(Debug, Default)]
struct RuntimeData {
    last_play_state: ClipPlayState,
    stop_was_caused_by_transport_change: bool,
}

impl Slot {
    pub fn fill(&mut self, clip: Clip) {
        // TODO-medium Suspend previous clip if playing.
        self.clip = Some(clip);
    }

    pub fn clip(&self) -> ClipEngineResult<&Clip> {
        self.clip_internal()
    }

    /// See [`Clip::recording_poll`].
    pub fn recording_poll<H: HandleStopEvent>(
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
    pub fn stop_clip<H: HandleStopEvent>(
        &mut self,
        args: ClipStopArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        self.runtime_data.stop_was_caused_by_transport_change = false;
        let instruction = self.clip_mut_internal()?.stop(args, event_handler);
        self.process_stop_instruction(instruction);
        Ok(())
    }

    fn process_stop_instruction(&mut self, instruction: StopSlotInstruction) {
        use StopSlotInstruction::*;
        match instruction {
            KeepSlot => {}
            ClearSlot => {
                self.clip = None;
            }
        }
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
    ) -> Result<(), ErrorWithPayload<SlotRecordInstruction>> {
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
                self.clip = Some(clip);
                Ok(())
            }
            ExistingClip(args) => {
                debug!("Record existing clip");
                let clip = match self.clip.as_mut() {
                    None => {
                        return Err(ErrorWithPayload::new("slot empty", ExistingClip(args)));
                    }
                    Some(c) => c,
                };
                clip.record(args, matrix_settings, column_settings)
                    .map_err(|e| e.map_payload(ExistingClip))
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
                clip.midi_overdub(instruction)
                    .map_err(|e| e.map_payload(MidiOverdub))
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

    pub fn write_clip_audio(&mut self, request: WriteAudioRequest) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.write_audio(request);
        Ok(())
    }

    pub fn set_clip_volume(&mut self, volume: Db) -> ClipEngineResult<()> {
        self.clip_mut_internal()?.set_volume(volume);
        Ok(())
    }

    pub fn process_transport_change<H: HandleStopEvent>(
        &mut self,
        args: &SlotProcessTransportChangeArgs,
        event_handler: &H,
    ) {
        let instruction = {
            let clip = match &mut self.clip {
                None => return,
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
                                ScheduledForPlay | Playing | ScheduledForStop => {
                                    // Retrigger (timeline switch)
                                    play_clip_by_transport(clip, args)
                                }
                                Stopped | Paused | Recording => {
                                    // Stop and forget.
                                    self.runtime_data.stop_clip_by_transport(
                                        clip,
                                        args,
                                        false,
                                        event_handler,
                                    )
                                }
                            }
                        }
                        StopAfterPlay => match state {
                            ScheduledForPlay | Playing | ScheduledForStop | Recording => {
                                // Stop and memorize
                                self.runtime_data.stop_clip_by_transport(
                                    clip,
                                    args,
                                    true,
                                    event_handler,
                                )
                            }
                            _ => {
                                // Stop and forget
                                self.runtime_data.stop_clip_by_transport(
                                    clip,
                                    args,
                                    false,
                                    event_handler,
                                )
                            }
                        },
                        StopAfterPause => self.runtime_data.stop_clip_by_transport(
                            clip,
                            args,
                            false,
                            event_handler,
                        ),
                    }
                }
                TransportChange::PlayCursorJump => {
                    // The play cursor was repositioned.
                    let play_state = clip.play_state();
                    use ClipPlayState::*;
                    if !matches!(play_state, ScheduledForPlay | Playing | ScheduledForStop) {
                        return;
                    }
                    clip.play(ClipPlayArgs {
                        timeline: args.column_args.timeline,
                        ref_pos: Some(args.column_args.timeline_cursor_pos),
                        matrix_settings: args.matrix_settings,
                        column_settings: args.column_settings,
                    })
                    .unwrap();
                    KeepSlot
                }
            }
        };
        self.process_stop_instruction(instruction)
    }

    pub fn process<H: HandleStopEvent>(
        &mut self,
        args: &mut ClipProcessArgs,
        event_handler: &H,
    ) -> ClipEngineResult<SlotProcessingOutcome> {
        measure_time("slot.process.time", || {
            let clip = self.clip_mut_internal()?;
            let clip_outcome = clip.process(args, event_handler);
            let play_state = clip.play_state();
            let last_play_state = self.runtime_data.last_play_state;
            let changed_play_state = if play_state == last_play_state {
                None
            } else {
                debug!("Clip state changed: {:?}", play_state);
                self.runtime_data.last_play_state = play_state;
                Some(play_state)
            };
            let outcome = SlotProcessingOutcome {
                changed_play_state,
                num_audio_frames_written: clip_outcome.num_audio_frames_written,
            };
            Ok(outcome)
        })
    }

    fn clip_internal(&self) -> ClipEngineResult<&Clip> {
        self.clip.as_ref().ok_or(SLOT_NOT_FILLED)
    }

    fn clip_mut_internal(&mut self) -> ClipEngineResult<&mut Clip> {
        self.clip.as_mut().ok_or(SLOT_NOT_FILLED)
    }
}

impl RuntimeData {
    fn stop_clip_by_transport<H: HandleStopEvent>(
        &mut self,
        clip: &mut Clip,
        args: &SlotProcessTransportChangeArgs,
        keep_starting_with_transport: bool,
        event_handler: &H,
    ) -> StopSlotInstruction {
        self.stop_was_caused_by_transport_change = keep_starting_with_transport;
        let args = ClipStopArgs {
            stop_timing: Some(ClipPlayStopTiming::Immediately),
            timeline: args.column_args.timeline,
            ref_pos: Some(args.column_args.timeline_cursor_pos),
            enforce_play_stop: true,
            matrix_settings: args.matrix_settings,
            column_settings: args.column_settings,
            audio_request_props: args.column_args.audio_request_props,
        };
        clip.stop(args, event_handler)
    }
}

pub struct SlotPollArgs {
    pub timeline_tempo: Bpm,
}

#[derive(Clone, Debug)]
pub struct SlotProcessTransportChangeArgs<'a> {
    pub column_args: ColumnProcessTransportChangeArgs<'a>,
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
) -> StopSlotInstruction {
    let args = ClipPlayArgs {
        timeline: args.column_args.timeline,
        ref_pos: Some(args.column_args.timeline_cursor_pos),
        matrix_settings: args.matrix_settings,
        column_settings: args.column_settings,
    };
    clip.play(args).unwrap();
    StopSlotInstruction::KeepSlot
}
