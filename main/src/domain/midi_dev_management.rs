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

    pub fn to_ini_entries(self) -> impl Iterator<Item = (String, u32)> {
        to_ini_entries("midiins", self.midiins)
            .chain(to_ini_entries("midiins_nowarn", self.midiins_nowarn))
    }
}

fn to_ini_entries(name: &str, devs: u128) -> impl Iterator<Item = (String, u32)> {
    [
        (name.to_string(), (devs & 0x0000_0000_0000_FFFF) as u32),
        (
            format!("{name}_h"),
            ((devs & 0x0000_0000_FFFF_0000) >> 4) as u32,
        ),
        (
            format!("{name}_x"),
            ((devs & 0x0000_FFFF_0000_0000) >> 8) as u32,
        ),
        (
            format!("{name}_x_h"),
            ((devs & 0xFFFF_0000_0000_0000) >> 12) as u32,
        ),
    ]
    .into_iter()
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

    pub fn to_ini_entries(self) -> impl Iterator<Item = (String, u32)> {
        to_ini_entries("midiouts", self.midiouts)
            .chain(to_ini_entries("midiouts_nowarn", self.midiouts_nowarn))
            .chain(to_ini_entries("midiouts_noreset", self.midiouts_noreset))
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
