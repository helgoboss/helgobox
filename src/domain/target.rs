use crate::domain::ActionInvocationType;
use helgoboss_learn::{Target, UnitValue};
use reaper_high::{Action, Fx, FxParameter, Track, TrackSend};
use reaper_medium::CommandId;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TargetCharacter {
    Trigger,
    Switch,
    Discrete(UnitValue),
    Continuous,
}

impl TargetCharacter {
    pub fn is_discrete(self) -> bool {
        matches!(self, TargetCharacter::Discrete(_))
    }
}

/// This is a ReaLearn target.
///
/// Unlike TargetModel, the real target has everything resolved already (e.g. track and FX) and
/// is immutable.
pub enum ReaperTarget {
    Action {
        action: Action,
        invocation_type: ActionInvocationType,
    },
    FxParameter {
        param: FxParameter,
    },
    TrackVolume {
        track: Track,
    },
    TrackSendVolume {
        send: TrackSend,
    },
    TrackPan {
        track: Track,
    },
    TrackArm {
        track: Track,
    },
    TrackSelection {
        track: Track,
        select_exclusively: bool,
    },
    TrackMute {
        track: Track,
    },
    TrackSolo {
        track: Track,
    },
    TrackSendPan {
        send: TrackSend,
    },
    Tempo,
    Playrate,
    FxEnable {
        fx: Fx,
    },
    FxPreset {
        fx: Fx,
    },
}

impl ReaperTarget {
    pub fn character(&self) -> TargetCharacter {
        // TODO
        TargetCharacter::Trigger
    }

    pub fn can_be_discrete(&self) -> bool {
        !self.character().is_discrete() && self.step_size().is_some()
    }
}

impl Target for ReaperTarget {
    fn current_value(&self) -> UnitValue {
        // TODO
        UnitValue::MIN
    }

    fn step_size(&self) -> Option<UnitValue> {
        // TODO
        None
    }

    fn wants_increments(&self) -> bool {
        // TODO
        false
    }
}
