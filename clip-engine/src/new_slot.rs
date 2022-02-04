use crate::{
    ClipChangedEvent, ClipPlayArgs, ClipPlayState, ClipProcessArgs, ClipStopArgs, NewClip, Timeline,
};
use helgoboss_learn::UnitValue;
use reaper_medium::{Bpm, PcmSourceTransfer, PositionInSeconds};

#[derive(Debug, Default)]
pub struct Slot {
    clip: Option<NewClip>,
    last_play_state: ClipPlayState,
}

impl Slot {
    pub fn fill(&mut self, clip: NewClip) {
        // TODO-medium Suspend previous clip if playing.
        self.clip = Some(clip);
    }

    pub fn clip(&self) -> Result<&NewClip, &'static str> {
        self.get_clip()
    }

    pub fn play_clip(&mut self, args: ClipPlayArgs) -> Result<(), &'static str> {
        let clip = self.get_clip_mut()?;
        clip.play(args);
        Ok(())
    }

    pub fn stop_clip(&mut self, args: ClipStopArgs) -> Result<(), &'static str> {
        let clip = self.get_clip_mut()?;
        clip.stop(args);
        Ok(())
    }

    pub fn set_clip_repeated(&mut self, repeated: bool) -> Result<(), &'static str> {
        let clip = self.get_clip_mut()?;
        clip.set_repeated(repeated);
        Ok(())
    }

    pub fn toggle_clip_repeated(&mut self) -> Result<ClipChangedEvent, &'static str> {
        let clip = self.get_clip_mut()?;
        let event = clip.toggle_repeated();
        Ok(event)
    }

    pub fn poll(&mut self, args: SlotPollArgs) -> Option<ClipChangedEvent> {
        let last_play_state = self.last_play_state;
        let clip = self.get_clip_mut().ok()?;
        let play_state = clip.play_state();
        let next_event = if play_state == last_play_state {
            // Play state has not changed. The position might or might not have changed. Even if
            // not, we are already being polled anyway. So just emit it!
            let prop_pos = clip.proportional_position().unwrap_or(UnitValue::MIN);
            Some(ClipChangedEvent::ClipPosition(prop_pos))
        } else {
            // Play state has changed. Emit this instead of a position change.
            println!("Clip state changed: {:?}", play_state);
            Some(ClipChangedEvent::PlayState(play_state))
        };
        self.last_play_state = play_state;
        next_event
    }

    pub fn process(&mut self, args: ClipProcessArgs<impl Timeline>) -> Result<(), &'static str> {
        let clip = self.get_clip_mut()?;
        clip.process(args);
        Ok(())
    }

    fn get_clip(&self) -> Result<&NewClip, &'static str> {
        self.clip.as_ref().ok_or(SLOT_NOT_FILLED)
    }

    fn get_clip_mut(&mut self) -> Result<&mut NewClip, &'static str> {
        self.clip.as_mut().ok_or(SLOT_NOT_FILLED)
    }
}

pub struct SlotPollArgs {
    pub timeline_tempo: Bpm,
}

const SLOT_NOT_FILLED: &str = "slot not filled";
