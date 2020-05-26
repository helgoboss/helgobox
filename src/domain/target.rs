use crate::domain::ActionInvocationType;
use helgoboss_learn::{Target, UnitValue};
use reaper_high::{
    Action, ActionCharacter, Fx, FxParameter, FxParameterCharacter, Pan, PlayRate, Tempo, Track,
    TrackSend, Volume,
};
use reaper_medium::{CommandId, Db, NormalizedPlayRate};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TargetCharacter {
    Trigger,
    Switch,
    Discrete,
    Continuous,
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
        use ReaperTarget::*;
        use TargetCharacter::*;
        match self {
            Action {
                action,
                invocation_type,
            } => match action.character() {
                ActionCharacter::Toggle => Trigger,
                ActionCharacter::Trigger => Switch,
            },
            FxParameter { param } => match param.character() {
                FxParameterCharacter::Toggle => Switch,
                FxParameterCharacter::Discrete => Discrete,
                FxParameterCharacter::Continuous => Continuous,
            },
            TrackVolume { .. } => Continuous,
            TrackSendVolume { .. } => Continuous,
            TrackPan { .. } => Continuous,
            TrackArm { .. } => Switch,
            TrackSelection { .. } => Switch,
            TrackMute { .. } => Switch,
            TrackSolo { .. } => Switch,
            TrackSendPan { .. } => Continuous,
            Tempo => Continuous,
            Playrate => Continuous,
            FxEnable { .. } => Switch,
            FxPreset { .. } => Discrete,
        }
    }

    pub fn can_be_discrete(&self) -> bool {
        self.character() != TargetCharacter::Discrete && self.step_size().is_some()
    }

    /// Formats the value completely (including a possible unit).
    pub fn format_value(&self, value: UnitValue) -> String {
        "".to_string()
        // TODO
    }

    /// Formats the value without unit.
    pub fn format_value_without_unit(&self, value: UnitValue) -> String {
        use ReaperTarget::*;
        match self {
            TrackVolume { .. } | TrackSendVolume { .. } => format_as_db_without_unit(value),
            TrackPan { .. } | TrackSendPan { .. } => format_as_pan_without_unit(value),
            Tempo => format_as_bpm_without_unit(value),
            Playrate => format_as_playback_speed_factor_without_unit(value),
            _ => format_as_percent_without_unit(value),
        }
    }

    /// This converts the given normalized value to a discrete value.
    ///
    /// Should be used for discrete targets only, e.g. FX preset. This target reports a step size
    /// which is 1.0 divided by the number of presets (because step sizes are reported as normalized
    /// values). If we want to display an increment or a particular value for this target, we
    /// don't show normalized values of course but a discrete number of presets, by using this
    /// function.
    ///
    /// # Errors
    ///
    /// Returns an error if this target doesn't report a step size.
    pub fn convert_value_to_discrete_value(&self, value: UnitValue) -> Result<u32, &'static str> {
        // Example (target step size = 0.10):
        // - 0    => 0
        // - 0.05 => 1
        // - 0.10 => 1
        // - 0.15 => 2
        // - 0.20 => 2
        let target_step_size = self.step_size().ok_or("target doesn't report step size")?;
        Ok((value.get() / target_step_size.get()).round() as _)
    }

    /// Meaning: not just percentages.
    pub fn can_parse_real_values(&self) -> bool {
        // TODO
        false
    }

    pub fn unit(&self) -> String {
        // TODO
        "".to_string()
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

fn format_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    let play_rate = PlayRate::from_normalized_value(NormalizedPlayRate::new(value.get()));
    format!("{:.2}", play_rate.playback_speed_factor().get())
}

fn format_as_bpm_without_unit(value: UnitValue) -> String {
    let tempo = Tempo::from_normalized_value(value.get());
    format!("{:.4}", tempo.bpm().get())
}

fn format_as_percent_without_unit(value: UnitValue) -> String {
    let percent = value.get() * 100.0;
    if (percent - percent.round()).abs() < 0.0000_0001 {
        // No fraction. Omit zeros after dot.
        format!("{:.0}", percent)
    } else {
        // Has fraction. We want to display these.
        format!("{:.8}", percent)
    }
}

fn format_as_db_without_unit(value: UnitValue) -> String {
    let db = Volume::from_normalized_value(value.get()).db();
    if db == Db::MINUS_INF {
        "-inf".to_string()
    } else {
        format!("{:.2}", db.get())
    }
}

fn format_as_pan_without_unit(value: UnitValue) -> String {
    Pan::from_normalized_value(value.get()).to_string()
}
