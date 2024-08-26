use helgoboss_learn::RawMidiEvent;
use reaper_medium::MidiInputDeviceId;

#[derive(Debug)]
pub struct MidiTransformationContainer {
    /// Emptied right after reading the input buffer of a device.
    same_device_events: Vec<RawMidiEvent>,
    /// Emptied later, after reading the input buffers of **all** devices (should have a larger capacity).
    other_device_events: Vec<DevQualifiedRawMidiEvent>,
}

#[derive(Debug)]
pub struct DevQualifiedRawMidiEvent {
    pub input_device_id: MidiInputDeviceId,
    pub event: RawMidiEvent,
}

impl DevQualifiedRawMidiEvent {
    fn new(input_device_id: MidiInputDeviceId, event: RawMidiEvent) -> Self {
        Self {
            input_device_id,
            event,
        }
    }
}

impl Default for MidiTransformationContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiTransformationContainer {
    pub fn new() -> Self {
        Self {
            same_device_events: Vec::with_capacity(100),
            other_device_events: Vec::with_capacity(900),
        }
    }

    pub fn push(&mut self, device: Option<MidiInputDeviceId>, event: RawMidiEvent) {
        if let Some(dev) = device {
            self.other_device_events
                .push(DevQualifiedRawMidiEvent::new(dev, event));
        } else {
            self.same_device_events.push(event);
        }
    }

    pub fn drain_same_device_events(&mut self) -> impl Iterator<Item = RawMidiEvent> + '_ {
        self.same_device_events.drain(..)
    }

    pub fn drain_other_device_events(
        &mut self,
    ) -> impl Iterator<Item = DevQualifiedRawMidiEvent> + '_ {
        self.other_device_events.drain(..)
    }
}
