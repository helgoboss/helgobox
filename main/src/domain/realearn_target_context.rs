use crate::base::{NamedChannelSender, SenderToNormalThread};
use crate::domain::{
    nks, AdditionalFeedbackEvent, FxSnapshotLoadedEvent, NksStateChangedEvent,
    ParameterAutomationTouchStateChangedEvent, TouchedTrackParameterType,
};
use reaper_high::{Fx, Track};
use reaper_medium::{GangBehavior, MediaTrack};
use std::collections::{HashMap, HashSet};

/// Feedback for most targets comes from REAPER itself but there are some targets for which ReaLearn
/// holds the state. It's in this struct.
pub struct RealearnTargetState {
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    // For "Load FX snapshot" target.
    fx_snapshot_chunk_hash_by_fx: HashMap<Fx, u64>,
    // For "Touch automation state" target.
    touched_things: HashSet<TouchedThing>,
    nks_state: nks::State,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct TouchedThing {
    track: MediaTrack,
    parameter_type: TouchedTrackParameterType,
}

impl TouchedThing {
    pub fn new(track: MediaTrack, parameter_type: TouchedTrackParameterType) -> Self {
        Self {
            track,
            parameter_type,
        }
    }
}

impl RealearnTargetState {
    pub fn new(
        additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    ) -> Self {
        Self {
            fx_snapshot_chunk_hash_by_fx: Default::default(),
            additional_feedback_event_sender,
            touched_things: Default::default(),
            nks_state: Default::default(),
        }
    }

    pub fn nks_state(&self) -> &nks::State {
        &self.nks_state
    }

    pub fn set_sound_index(&mut self, index: u32) {
        self.nks_state.set_sound_index(index);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::NksStateChanged(NksStateChangedEvent::SoundIndexChanged {
                index,
            }),
        );
    }

    pub fn current_fx_snapshot_chunk_hash(&self, fx: &Fx) -> Option<u64> {
        self.fx_snapshot_chunk_hash_by_fx.get(fx).copied()
    }

    pub fn load_fx_snapshot(
        &mut self,
        fx: Fx,
        chunk: &str,
        chunk_hash: u64,
    ) -> Result<(), &'static str> {
        fx.set_tag_chunk(chunk)?;
        self.fx_snapshot_chunk_hash_by_fx
            .insert(fx.clone(), chunk_hash);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::FxSnapshotLoaded(FxSnapshotLoadedEvent { fx }),
        );
        Ok(())
    }

    pub fn touch_automation_parameter(
        &mut self,
        track: &Track,
        parameter_type: TouchedTrackParameterType,
    ) {
        self.touched_things
            .insert(TouchedThing::new(track.raw(), parameter_type));
        self.post_process_touch(track, parameter_type);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                ParameterAutomationTouchStateChangedEvent {
                    track: track.raw(),
                    parameter_type,
                    new_value: true,
                },
            ),
        );
    }

    pub fn untouch_automation_parameter(
        &mut self,
        track: &Track,
        parameter_type: TouchedTrackParameterType,
    ) {
        self.touched_things
            .remove(&TouchedThing::new(track.raw(), parameter_type));
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                ParameterAutomationTouchStateChangedEvent {
                    track: track.raw(),
                    parameter_type,
                    new_value: false,
                },
            ),
        );
    }

    fn post_process_touch(&mut self, track: &Track, parameter_type: TouchedTrackParameterType) {
        match parameter_type {
            TouchedTrackParameterType::Volume => {
                track.set_volume(track.volume(), GangBehavior::DenyGang);
            }
            TouchedTrackParameterType::Pan => {
                track.set_pan(track.pan(), GangBehavior::DenyGang);
            }
            TouchedTrackParameterType::Width => {
                track.set_width(track.width(), GangBehavior::DenyGang);
            }
        }
    }

    pub fn automation_parameter_is_touched(
        &self,
        track: MediaTrack,
        parameter_type: TouchedTrackParameterType,
    ) -> bool {
        self.touched_things
            .contains(&TouchedThing::new(track, parameter_type))
    }
}
