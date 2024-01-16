use reaper_high::Reaper;
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct MidiInDevsConfig {
    midiins: u128,
    midiins_nowarn: u128,
}

impl MidiInDevsConfig {
    pub const ALL_ENABLED: Self = Self {
        midiins: u128::MAX,
        midiins_nowarn: u128::MAX,
    };

    pub fn from_reaper() -> Self {
        Self {
            midiins: get_midi_dev_var("midiins"),
            midiins_nowarn: get_midi_dev_var("midiins_nowarn"),
        }
    }

    pub fn apply_to_reaper(&self) {
        set_midi_dev_var("midiins", self.midiins);
        set_midi_dev_var("midiins_nowarn", self.midiins_nowarn);
    }

    pub fn with_dev_enabled(&self, dev_id: MidiInputDeviceId) -> Self {
        let index = dev_id.get();
        Self {
            midiins: self.midiins | (1 << index),
            midiins_nowarn: self.midiins_nowarn | (1 << index),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct MidiOutDevsConfig {
    midiouts: u128,
    midiouts_nowarn: u128,
    midiouts_noreset: u128,
}

impl MidiOutDevsConfig {
    pub fn from_reaper() -> Self {
        Self {
            midiouts: get_midi_dev_var("midiouts"),
            midiouts_nowarn: get_midi_dev_var("midiouts_nowarn"),
            midiouts_noreset: get_midi_dev_var("midiouts_noreset"),
        }
    }

    pub fn apply_to_reaper(&self) {
        set_midi_dev_var("midiouts", self.midiouts);
        set_midi_dev_var("midiouts_nowarn", self.midiouts_nowarn);
        set_midi_dev_var("midiouts_noreset", self.midiouts_noreset);
    }

    pub fn with_dev_enabled(&self, dev_id: MidiOutputDeviceId) -> Self {
        let index = dev_id.get();
        Self {
            midiouts: self.midiouts | (1 << index),
            midiouts_nowarn: self.midiouts_nowarn | (1 << index),
            midiouts_noreset: self.midiouts_noreset | (1 << index),
        }
    }
}

fn get_midi_dev_var(name: &str) -> u128 {
    let reaper = Reaper::get();
    match reaper.get_preference_ref::<u128>(name) {
        Ok(v) => *v,
        Err(_) => {
            // Older REAPER versions supported only 64 MIDI devices
            match reaper.get_preference_ref::<u64>(name) {
                Ok(v) => *v as u128,
                Err(_) => 0,
            }
        }
    }
}

fn set_midi_dev_var(name: &str, value: u128) {
    let reaper = Reaper::get();
    match reaper.get_preference_ref::<u128>(name) {
        Ok(v) => *v = value,
        Err(_) => {
            // Older REAPER versions supported only 64 MIDI devices
            match reaper.get_preference_ref::<u64>(name) {
                Ok(v) => *v = value as u64,
                Err(_) => {}
            }
        }
    }
}
