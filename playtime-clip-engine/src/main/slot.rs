use crate::main::Clip;
use crate::rt::{ClipPlayState, NormalRecordingOutcome};
use crate::ClipEngineResult;
use reaper_high::Project;
use reaper_medium::OwnedPcmSource;

#[derive(Clone, Debug, Default)]
pub struct Slot {
    state: SlotState,
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
            RecordingFromScratchRequested | RecordingFromScratch => Ok(ClipPlayState::Recording),
            Filled(clip) => clip.play_state(),
        }
    }

    pub fn fill_with(&mut self, clip: Clip) {
        self.state = SlotState::Filled(clip);
    }

    /// Important to call on recording in order to allow for idempotence.
    pub fn notify_recording_requested(&mut self) -> ClipEngineResult<()> {
        use SlotState::*;
        match &mut self.state {
            Empty => {
                self.state = RecordingFromScratchRequested;
                Ok(())
            }
            Filled(clip) => clip.notify_recording_requested(),
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
    ) -> ClipEngineResult<()> {
        match outcome {
            NormalRecordingOutcome::Committed(recording) => {
                let clip = Clip::from_recording(recording, temporary_project)?;
                debug!("Fill slot with clip: {:#?}", &clip);
                self.state = SlotState::Filled(clip);
                Ok(())
            }
            NormalRecordingOutcome::Cancelled => {
                debug!("Recording cancelled");
                use SlotState::*;
                if matches!(
                    &self.state,
                    RecordingFromScratch | RecordingFromScratchRequested
                ) {
                    self.state = SlotState::Empty;
                }
                Ok(())
            }
        }
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
