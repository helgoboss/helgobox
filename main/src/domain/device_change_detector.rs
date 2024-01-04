use reaper_high::Reaper;
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::collections::HashSet;
use std::hash::Hash;

#[derive(Debug, Default)]
pub struct MidiDeviceChangeDetector {
    connected_midi_in_devs: HashSet<MidiInputDeviceId>,
    connected_midi_out_devs: HashSet<MidiOutputDeviceId>,
}

impl MidiDeviceChangeDetector {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn poll_for_midi_input_device_changes(&mut self) -> DeviceDiff<MidiInputDeviceId> {
        let connected_devs: HashSet<_> = Reaper::get()
            .midi_input_devices()
            .filter(|d| d.is_connected())
            .map(|d| d.id())
            .collect();
        let diff = DeviceDiff::new(&self.connected_midi_in_devs, &connected_devs);
        self.connected_midi_in_devs = connected_devs;
        diff
    }

    pub fn poll_for_midi_output_device_changes(&mut self) -> DeviceDiff<MidiOutputDeviceId> {
        let connected_devs: HashSet<_> = Reaper::get()
            .midi_output_devices()
            .filter(|d| d.is_connected())
            .map(|d| d.id())
            .collect();
        let diff = DeviceDiff::new(&self.connected_midi_out_devs, &connected_devs);
        self.connected_midi_out_devs = connected_devs;
        diff
    }
}

pub struct DeviceDiff<T> {
    pub added_devices: HashSet<T>,
    pub removed_devices: HashSet<T>,
}

impl<T: Eq + Hash + Copy> DeviceDiff<T> {
    fn new(old_devs: &HashSet<T>, new_devs: &HashSet<T>) -> Self {
        Self {
            added_devices: new_devs.difference(old_devs).copied().collect(),
            removed_devices: old_devs.difference(new_devs).copied().collect(),
        }
    }

    pub fn devices_changed(&self) -> bool {
        !self.added_devices.is_empty() || !self.removed_devices.is_empty()
    }
}
