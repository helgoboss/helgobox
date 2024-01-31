use crate::domain::{MidiInDevsConfig, MidiOutDevsConfig};
use base::hash_util::NonCryptoHashSet;
use reaper_high::Reaper;
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::hash::Hash;

#[derive(Debug, Default)]
pub struct MidiDeviceChangeDetector {
    old_connected_in_devs: NonCryptoHashSet<MidiInputDeviceId>,
    old_in_config: MidiInDevsConfig,
    old_connected_out_devs: NonCryptoHashSet<MidiOutputDeviceId>,
    old_out_config: MidiOutDevsConfig,
}

impl MidiDeviceChangeDetector {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn poll_for_midi_input_device_changes(&mut self) -> DeviceDiff<MidiInputDeviceId> {
        let new_connected_devs: NonCryptoHashSet<_> = Reaper::get()
            .midi_input_devices()
            .filter(|d| d.is_connected())
            .map(|d| d.id())
            .collect();
        let new_in_config = MidiInDevsConfig::from_reaper();
        let diff = DeviceDiff::new(
            &self.old_connected_in_devs,
            &new_connected_devs,
            new_in_config != self.old_in_config,
        );
        self.old_connected_in_devs = new_connected_devs;
        self.old_in_config = new_in_config;
        diff
    }

    pub fn poll_for_midi_output_device_changes(&mut self) -> DeviceDiff<MidiOutputDeviceId> {
        let new_connected_devs: NonCryptoHashSet<_> = Reaper::get()
            .midi_output_devices()
            .filter(|d| d.is_connected())
            .map(|d| d.id())
            .collect();
        let new_out_config = MidiOutDevsConfig::from_reaper();
        let diff = DeviceDiff::new(
            &self.old_connected_out_devs,
            &new_connected_devs,
            new_out_config != self.old_out_config,
        );
        self.old_connected_out_devs = new_connected_devs;
        self.old_out_config = new_out_config;
        diff
    }
}

#[derive(Clone, Debug)]
pub struct DeviceDiff<T> {
    pub added_devices: NonCryptoHashSet<T>,
    pub removed_devices: NonCryptoHashSet<T>,
    pub device_config_changed: bool,
}

impl<T: Eq + Hash + Copy> DeviceDiff<T> {
    fn new(
        old_devs: &NonCryptoHashSet<T>,
        new_devs: &NonCryptoHashSet<T>,
        device_config_changed: bool,
    ) -> Self {
        Self {
            added_devices: new_devs.difference(old_devs).copied().collect(),
            removed_devices: old_devs.difference(new_devs).copied().collect(),
            device_config_changed,
        }
    }

    pub fn devices_changed(&self) -> bool {
        !self.added_devices.is_empty() || !self.removed_devices.is_empty()
    }
}
