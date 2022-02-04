use crate::{
    ClipChangedEvent, ClipPlayArgs, ClipPlayState, ClipProcessArgs, ClipRecordSourceType,
    ClipStopArgs, ClipStopBehavior, NewClip, RecordBehavior, RecordKind, RelevantPlayStateChange,
    Timeline, TimelineMoment, TransportChange, WriteAudioRequest, WriteMidiRequest,
};
use helgoboss_learn::UnitValue;
use reaper_medium::{Bpm, PcmSourceTransfer, PositionInSeconds, ReaperVolumeValue};

#[derive(Debug, Default)]
pub struct Slot {
    clip: Option<NewClip>,
    runtime_data: RuntimeData,
}

#[derive(Debug, Default)]
struct RuntimeData {
    last_play_state: ClipPlayState,
    last_play: Option<LastPlay>,
    stop_was_caused_by_transport_change: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct LastPlay {
    was_synced_to_bar: bool,
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
        self.runtime_data.last_play = Some(LastPlay {
            was_synced_to_bar: args.from_bar.is_some(),
        });
        self.get_clip_mut()?.play(args);
        Ok(())
    }

    pub fn stop_clip(&mut self, args: ClipStopArgs) -> Result<(), &'static str> {
        self.runtime_data.stop_was_caused_by_transport_change = false;
        self.get_clip_mut()?.stop(args);
        Ok(())
    }

    pub fn set_clip_repeated(&mut self, repeated: bool) -> Result<(), &'static str> {
        self.get_clip_mut()?.set_repeated(repeated);
        Ok(())
    }

    pub fn toggle_clip_repeated(&mut self) -> Result<ClipChangedEvent, &'static str> {
        Ok(self.get_clip_mut()?.toggle_repeated())
    }

    pub fn record_clip(&mut self, behavior: RecordBehavior) -> Result<(), &'static str> {
        self.get_clip_mut()?.record(behavior);
        Ok(())
    }

    pub fn pause_clip(&mut self) -> Result<(), &'static str> {
        self.get_clip_mut()?.pause();
        Ok(())
    }

    pub fn seek_clip(&mut self, desired_pos: UnitValue) -> Result<(), &'static str> {
        self.get_clip_mut()?.seek(desired_pos);
        Ok(())
    }

    pub fn clip_record_source_type(&self) -> Option<ClipRecordSourceType> {
        self.get_clip().ok()?.record_source_type()
    }

    pub fn write_clip_midi(&mut self, request: WriteMidiRequest) -> Result<(), &'static str> {
        self.get_clip_mut()?.write_midi(request);
        Ok(())
    }

    pub fn write_clip_audio(&mut self, request: WriteAudioRequest) -> Result<(), &'static str> {
        self.get_clip_mut()?.write_audio(request);
        Ok(())
    }

    pub fn set_clip_volume(
        &mut self,
        volume: ReaperVolumeValue,
    ) -> Result<ClipChangedEvent, &'static str> {
        Ok(self.get_clip_mut()?.set_volume(volume))
    }

    pub fn poll(&mut self, args: SlotPollArgs) -> Option<ClipChangedEvent> {
        let last_play_state = self.runtime_data.last_play_state;
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
        self.runtime_data.last_play_state = play_state;
        next_event
    }

    pub fn process_transport_change(&mut self, args: &SlotProcessTransportChangeArgs) {
        let clip = match &mut self.clip {
            None => return,
            Some(c) => c,
        };
        match args.change {
            TransportChange::PlayState(rel_change) => {
                // We have a relevant transport change.
                let last_play = match self.runtime_data.last_play {
                    None => return,
                    Some(a) => a,
                };
                // Clip was started at least once already.
                let state = clip.play_state();
                use ClipPlayState::*;
                use RelevantPlayStateChange::*;
                match rel_change {
                    PlayAfterStop => {
                        match state {
                            Stopped if self.runtime_data.stop_was_caused_by_transport_change => {
                                // REAPER transport was started from stopped state. Clip is stopped
                                // as well and was put in that state due to a previous transport
                                // stop. Play the clip!
                                let args = ClipPlayArgs {
                                    from_bar: Some(args.moment.next_bar()),
                                };
                                clip.play(args);
                            }
                            _ => {
                                // Stop and forget (because we have a timeline switch).
                                self.runtime_data.stop_clip_by_transport(clip, args, false);
                            }
                        }
                    }
                    StopAfterPlay => match state {
                        ScheduledForPlay | Playing | ScheduledForStop
                            if last_play.was_synced_to_bar =>
                        {
                            // Stop and memorize
                            self.runtime_data.stop_clip_by_transport(clip, args, true);
                        }
                        _ => {
                            // Stop and forget
                            self.runtime_data.stop_clip_by_transport(clip, args, false);
                        }
                    },
                    StopAfterPause => {
                        self.runtime_data.stop_clip_by_transport(clip, args, false);
                    }
                }
            }
            TransportChange::PlayCursorJump => {
                // The play cursor was repositioned.
                let last_play = match self.runtime_data.last_play {
                    None => return,
                    Some(a) => a,
                };
                if !last_play.was_synced_to_bar {
                    return;
                }
                let play_state = clip.play_state();
                use ClipPlayState::*;
                if !matches!(play_state, ScheduledForPlay | Playing | ScheduledForStop) {
                    return;
                }
                clip.play(ClipPlayArgs {
                    from_bar: Some(args.moment.next_bar()),
                });
            }
        }
    }

    pub fn process(&mut self, args: ClipProcessArgs<impl Timeline>) -> Result<(), &'static str> {
        self.get_clip_mut()?.process(args);
        Ok(())
    }

    fn get_clip(&self) -> Result<&NewClip, &'static str> {
        self.clip.as_ref().ok_or(SLOT_NOT_FILLED)
    }

    fn get_clip_mut(&mut self) -> Result<&mut NewClip, &'static str> {
        self.clip.as_mut().ok_or(SLOT_NOT_FILLED)
    }
}

impl RuntimeData {
    fn stop_clip_by_transport(
        &mut self,
        clip: &mut NewClip,
        args: &SlotProcessTransportChangeArgs,
        keep_starting_with_transport: bool,
    ) {
        self.stop_was_caused_by_transport_change = keep_starting_with_transport;
        clip.stop(ClipStopArgs {
            stop_behavior: ClipStopBehavior::Immediately,
            timeline_cursor_pos: args.moment.cursor_pos(),
            timeline: args.timeline,
        });
    }
}

pub struct SlotPollArgs {
    pub timeline_tempo: Bpm,
}

pub struct SlotProcessTransportChangeArgs<'a> {
    pub change: TransportChange,
    pub moment: TimelineMoment,
    pub timeline: &'a dyn Timeline,
}

const SLOT_NOT_FILLED: &str = "slot not filled";
