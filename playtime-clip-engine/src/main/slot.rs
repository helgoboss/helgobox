use crate::main::Clip;
use crate::rt::ClipPlayState;
use crate::ClipEngineResult;

#[derive(Clone, Debug, Default)]
pub struct Slot {
    state: SlotState,
}

impl Slot {
    pub fn state(&self) -> &SlotState {
        &self.state
    }

    pub fn clip(&self) -> Option<&Clip> {
        match &self.state {
            SlotState::Empty => None,
            SlotState::RecordingFromScratch => None,
            SlotState::Filled(clip) => Some(clip),
        }
    }

    pub fn clip_mut(&mut self) -> Option<&mut Clip> {
        match &mut self.state {
            SlotState::Empty => None,
            SlotState::RecordingFromScratch => None,
            SlotState::Filled(clip) => Some(clip),
        }
    }

    pub fn play_state(&self) -> ClipEngineResult<ClipPlayState> {
        match &self.state {
            SlotState::Empty => Err("slot empty"),
            SlotState::RecordingFromScratch => Ok(ClipPlayState::Recording),
            SlotState::Filled(clip) => clip.play_state(),
        }
    }

    pub fn fill_with(&mut self, clip: Clip) {
        self.state = SlotState::Filled(clip);
    }

    /// Important to call on recording in order to allow for idempotence.
    pub fn mark_recording(&mut self) {
        use SlotState::*;
        match &mut self.state {
            Empty => {
                self.state = RecordingFromScratch;
            }
            RecordingFromScratch => {}
            Filled(clip) => {
                clip.mark_recording();
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum SlotState {
    /// Slot is empty.
    Empty,
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
