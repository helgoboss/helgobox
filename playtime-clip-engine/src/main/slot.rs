use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::main::Clip;
use crate::rt::supplier::{MaterialInfo, MIDI_BASE_BPM};
use crate::rt::tempo_util::calc_tempo_factor;
use crate::rt::{ClipChangedEvent, ClipPlayState, NormalRecordingOutcome, SharedPos};
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use helgoboss_learn::UnitValue;
use reaper_high::Project;
use reaper_medium::{OwnedPcmSource, PositionInSeconds};

#[derive(Clone, Debug, Default)]
pub struct Slot {
    state: SlotState,
    // We keep this in the slot (vs. in the clip) because we want to have full runtime feedback
    // even while a new clip is being recorded and thus the slot is not yet filled with a
    // full-blown clip.
    runtime_data: Option<SlotRuntimeData>,
}

#[derive(Clone, Debug)]
struct SlotRuntimeData {
    play_state: ClipPlayState,
    pos: SharedPos,
    material_info: MaterialInfo,
}

impl SlotRuntimeData {
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

impl Slot {
    pub fn state(&self) -> &SlotState {
        &self.state
    }

    pub fn clip(&self) -> Option<&Clip> {
        if let SlotState::Filled(clip) = &self.state {
            Some(clip)
        } else {
            None
        }
    }

    pub fn material_info(&self) -> Option<&MaterialInfo> {
        let runtime_data = self.runtime_data.as_ref()?;
        Some(&runtime_data.material_info)
    }

    pub fn clip_mut(&mut self) -> Option<&mut Clip> {
        if let SlotState::Filled(clip) = &mut self.state {
            Some(clip)
        } else {
            None
        }
    }

    pub fn play_state(&self) -> ClipEngineResult<ClipPlayState> {
        use SlotState::*;
        match &self.state {
            Empty => Err("slot empty"),
            RecordingFromScratchRequested => Ok(ClipPlayState::ScheduledForRecordingStart),
            // TODO-high CONTINUE It would be nice if we could get real state updates from the clip
            //  already. For this, the clip must already be sent here at the time of acknowledgement.
            RecordingFromScratch | Filled(_) => Ok(self.runtime_data()?.play_state),
        }
    }

    pub fn update_play_state(&mut self, play_state: ClipPlayState) -> ClipEngineResult<()> {
        self.runtime_data_mut()?.play_state = play_state;
        Ok(())
    }

    pub fn update_material_info(&mut self, material_info: MaterialInfo) -> ClipEngineResult<()> {
        self.runtime_data_mut()?.material_info = material_info;
        Ok(())
    }

    pub fn proportional_pos(&self) -> ClipEngineResult<UnitValue> {
        let runtime_data = self.runtime_data()?;
        let pos = runtime_data.pos.get();
        if pos < 0 {
            return Err("count-in phase");
        }
        let frame_count = runtime_data.material_info.frame_count();
        if frame_count == 0 {
            return Err("frame count is zero");
        }
        let mod_pos = pos as usize % frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Ok(proportional)
    }

    pub fn position_in_seconds(
        &self,
        timeline: &HybridTimeline,
    ) -> ClipEngineResult<PositionInSeconds> {
        let runtime_data = self.runtime_data()?;
        let pos_in_source_frames = runtime_data.mod_frame();
        let pos_in_secs = convert_position_in_frames_to_seconds(
            pos_in_source_frames,
            runtime_data.material_info.frame_rate(),
        );
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        let is_midi = runtime_data.material_info.is_midi();
        let tempo_factor = if let Some(clip) = self.clip() {
            clip.tempo_factor(timeline_tempo, is_midi)
        } else {
            if is_midi {
                calc_tempo_factor(MIDI_BASE_BPM, timeline_tempo)
            } else {
                // When recording audio, we have tempo factor 1.0 (original recording tempo).
                1.0
            }
        };
        let tempo_adjusted_secs = adjust_pos_in_secs_anti_proportionally(pos_in_secs, tempo_factor);
        Ok(tempo_adjusted_secs)
    }

    pub fn fill_with(&mut self, clip: Clip, rt_clip: &rt::Clip) {
        let runtime_data = SlotRuntimeData {
            play_state: Default::default(),
            pos: rt_clip.shared_pos(),
            material_info: rt_clip.material_info().unwrap(),
        };
        self.state = SlotState::Filled(clip);
        self.runtime_data = Some(runtime_data);
    }

    /// Important to call on recording in order to allow for idempotence.
    pub fn notify_recording_requested(&mut self) -> ClipEngineResult<()> {
        use SlotState::*;
        match &mut self.state {
            Empty => {
                self.state = RecordingFromScratchRequested;
                Ok(())
            }
            Filled(clip) => {
                clip.notify_recording_requested()?;
                if self
                    .play_state()
                    .map(|ps| ps.is_as_good_as_recording())
                    .unwrap_or(false)
                {
                    return Err("already recording");
                }
                Ok(())
            }
            RecordingFromScratchRequested => Err("recording has already been requested"),
            RecordingFromScratch => Err("is recording already"),
        }
    }

    pub fn notify_recording_request_acknowledged(
        &mut self,
        successful: bool,
    ) -> ClipEngineResult<()> {
        use SlotState::*;
        match &mut self.state {
            Empty => Err("recording was not requested"),
            RecordingFromScratchRequested => {
                self.state = if successful {
                    RecordingFromScratch
                } else {
                    Empty
                };
                Ok(())
            }
            RecordingFromScratch => Err("recording already"),
            Filled(clip) => {
                clip.notify_recording_request_acknowledged();
                Ok(())
            }
        }
    }

    pub fn notify_midi_overdub_finished(
        &mut self,
        mirror_source: OwnedPcmSource,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<()> {
        use SlotState::*;
        match &mut self.state {
            Filled(clip) => clip.notify_midi_overdub_finished(mirror_source, temporary_project),
            _ => Err("slot was not filled and thus couldn't have been MIDI-overdubbed"),
        }
    }

    pub fn notify_normal_recording_finished(
        &mut self,
        outcome: NormalRecordingOutcome,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<Option<ClipChangedEvent>> {
        match outcome {
            NormalRecordingOutcome::Committed(recording) => {
                let clip = Clip::from_recording(
                    recording.kind_specific,
                    recording.clip_settings,
                    temporary_project,
                )?;
                let runtime_data = SlotRuntimeData {
                    play_state: recording.play_state,
                    pos: recording.shared_pos,
                    material_info: recording.material_info,
                };
                debug!("Fill slot with clip: {:#?}", &clip);
                self.state = SlotState::Filled(clip);
                self.runtime_data = Some(runtime_data);
                Ok(None)
            }
            NormalRecordingOutcome::Canceled => {
                debug!("Recording canceled");
                use SlotState::*;
                match &mut self.state {
                    Filled(clip) => {
                        clip.notify_recording_canceled();
                        Ok(None)
                    }
                    _ => {
                        self.state = SlotState::Empty;
                        Ok(Some(ClipChangedEvent::Removed))
                    }
                }
            }
        }
    }

    fn runtime_data(&self) -> ClipEngineResult<&SlotRuntimeData> {
        get_runtime_data(&self.runtime_data)
    }

    fn runtime_data_mut(&mut self) -> ClipEngineResult<&mut SlotRuntimeData> {
        get_runtime_data_mut(&mut self.runtime_data)
    }
}

#[derive(Clone, Debug)]
pub enum SlotState {
    /// Slot is empty.
    Empty,
    /// Slot is still kind of empty but recording a totally new clip has been requested.
    RecordingFromScratchRequested,
    /// Slot is still kind of empty but a totally new clip is being recorded right now.
    RecordingFromScratch,
    /// Slot has a clip.
    ///
    /// This means one of the following things:
    ///
    /// - The clip is active and can be playing, stopped etc.
    /// - The clip is active and is currently being MIDI-overdubbed.
    /// - The clip is inactive, which means it's about to be replaced with different clip content
    ///   that's in the process of being recorded right now.
    Filled(Clip),
}

impl Default for SlotState {
    fn default() -> Self {
        SlotState::Empty
    }
}

fn get_runtime_data(runtime_data: &Option<SlotRuntimeData>) -> ClipEngineResult<&SlotRuntimeData> {
    runtime_data.as_ref().ok_or(SLOT_RUNTIME_DATA_UNAVAILABLE)
}

fn get_runtime_data_mut(
    runtime_data: &mut Option<SlotRuntimeData>,
) -> ClipEngineResult<&mut SlotRuntimeData> {
    runtime_data.as_mut().ok_or(SLOT_RUNTIME_DATA_UNAVAILABLE)
}

const SLOT_RUNTIME_DATA_UNAVAILABLE: &str = "clip slot runtime data unavailable";
