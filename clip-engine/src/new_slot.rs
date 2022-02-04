use crate::{ClipPlayArgs, ClipProcessArgs, ClipStopArgs, NewClip, Timeline};
use reaper_medium::PcmSourceTransfer;

#[derive(Debug, Default)]
pub struct Slot {
    clip: Option<NewClip>,
}

impl Slot {
    pub fn fill(&mut self, clip: NewClip) {
        // TODO-medium Suspend previous clip if playing.
        self.clip = Some(clip);
    }

    pub fn play(&mut self, args: ClipPlayArgs) -> Result<(), &'static str> {
        let clip = self.get_clip()?;
        clip.play(args);
        Ok(())
    }

    pub fn stop(&mut self, args: ClipStopArgs) -> Result<(), &'static str> {
        let clip = self.get_clip()?;
        clip.stop(args);
        Ok(())
    }

    pub fn process(&mut self, args: ClipProcessArgs<impl Timeline>) -> Result<(), &'static str> {
        let clip = self.get_clip()?;
        clip.process(args);
        Ok(())
    }

    fn get_clip(&mut self) -> Result<&mut NewClip, &'static str> {
        self.clip.as_mut().ok_or("slot not filled")
    }
}
