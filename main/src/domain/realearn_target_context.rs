use crate::domain::{
    AdditionalFeedbackEvent, FxSnapshotLoadedEvent, ParameterAutomationTouchStateChangedEvent,
    TouchedTrackParameterType,
};
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use base::{NamedChannelSender, SenderToNormalThread};
use reaper_high::{Fx, Track};
use reaper_medium::{GangBehavior, MediaTrack, NotificationBehavior, ValueChange};

/// Feedback for most targets comes from REAPER itself but there are some targets for which ReaLearn
/// holds the state. It's in this struct.
///
/// Some of this state can be persistent. This raises the question which ReaLearn instance should
/// be responsible for saving it. If you need persistent state, first think about if it shouldn't
/// rather be part of `InstanceState`. Then it's owned by a particular instance, which is then also
/// responsible for saving it. But we also have global REAPER things such as additional FX state.
/// In this case, we should put it here and track for each state which instance is responsible for
/// saving it!
pub struct RealearnTargetState {
    /// For notifying ReaLearn about state changes.
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    /// Memorizes for each FX the hash of its last FX snapshot loaded via "Load FX snapshot" target.
    ///
    /// Persistent.
    // TODO-high-pot Restore on load (by looking up snapshot chunk)
    fx_snapshot_chunk_hash_by_fx: NonCryptoHashMap<Fx, u64>,
    /// Memorizes for each FX some infos about its last loaded Pot preset.
    ///
    /// Persistent.
    // TODO-high-pot Restore on load (by looking up DB)
    current_pot_preset_by_fx: NonCryptoHashMap<Fx, pot::CurrentPreset>,
    /// Memorizes all currently touched track parameters.
    ///
    /// For "Touch automation state" target.
    ///
    /// Not persistent.
    touched_things: NonCryptoHashSet<TouchedThing>,
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
            additional_feedback_event_sender,
            fx_snapshot_chunk_hash_by_fx: Default::default(),
            touched_things: Default::default(),
            current_pot_preset_by_fx: Default::default(),
        }
    }

    pub fn current_fx_preset(&self, fx: &Fx) -> Option<&pot::CurrentPreset> {
        let fx = fx.guid_based()?;
        self.current_pot_preset_by_fx.get(&fx)
    }

    pub fn set_current_fx_preset(&mut self, fx: Fx, current_preset: pot::CurrentPreset) {
        // reaper_high::Fx is really not a good candidate for hashing. At the moment, we must
        // "normalize" it to be guid-based to really match. But it's still dirty, even the FX
        // chain and track matching. We need several distinct types!
        let Some(fx) = fx.guid_based() else {
            return;
        };
        self.current_pot_preset_by_fx.insert(fx, current_preset);
        self.additional_feedback_event_sender
            .send_complaining(AdditionalFeedbackEvent::MappedFxParametersChanged);
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
        // fx.set_vst_chunk_encoded(chunk.to_string())?;
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
        let Ok(raw_track) = track.raw() else {
            return;
        };
        self.touched_things
            .insert(TouchedThing::new(raw_track, parameter_type));
        self.post_process_touch(track, parameter_type);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                ParameterAutomationTouchStateChangedEvent {
                    track: raw_track,
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
        let Ok(raw_track) = track.raw() else {
            return;
        };
        self.touched_things
            .remove(&TouchedThing::new(raw_track, parameter_type));
        self.post_process_touch(track, parameter_type);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                ParameterAutomationTouchStateChangedEvent {
                    track: raw_track,
                    parameter_type,
                    new_value: false,
                },
            ),
        );
    }

    fn post_process_touch(&mut self, track: &Track, parameter_type: TouchedTrackParameterType) {
        match parameter_type {
            TouchedTrackParameterType::Volume => {
                let resulting_value = track.csurf_on_volume_change_ex(
                    ValueChange::Absolute(track.volume()),
                    GangBehavior::DenyGang,
                );
                if let Ok(v) = resulting_value {
                    let _ = track.csurf_set_surface_volume(v, NotificationBehavior::NotifyAll);
                }
            }
            TouchedTrackParameterType::Pan => {
                let resulting_value = track.csurf_on_pan_change_ex(
                    ValueChange::Absolute(track.pan().reaper_value()),
                    GangBehavior::DenyGang,
                );
                if let Ok(v) = resulting_value {
                    let _ = track.csurf_set_surface_pan(v, NotificationBehavior::NotifyAll);
                }
            }
            TouchedTrackParameterType::Width => {
                let result = track.csurf_on_width_change_ex(
                    ValueChange::Absolute(track.width().reaper_value()),
                    GangBehavior::DenyGang,
                );
                if result.is_ok() {
                    let _ = track.csurf_set_surface_pan(
                        track.pan().reaper_value(),
                        NotificationBehavior::NotifyAll,
                    );
                }
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
