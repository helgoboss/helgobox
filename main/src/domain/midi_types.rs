use reaper_common_types::DurationInSeconds;
use reaper_medium::{Hz, MIDI_INPUT_FRAME_RATE};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MidiEvent<T> {
    offset: SampleOffset,
    payload: T,
}

impl<T: Copy> MidiEvent<T> {
    pub fn without_offset(msg: T) -> Self {
        Self::new(SampleOffset::ZERO, msg)
    }

    pub fn new(offset: SampleOffset, payload: T) -> Self {
        Self { offset, payload }
    }

    pub fn offset(&self) -> SampleOffset {
        self.offset
    }

    pub fn payload(&self) -> T {
        self.payload
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SampleOffset(u64);

impl SampleOffset {
    pub const ZERO: SampleOffset = SampleOffset(0);

    pub fn from_midi_input_frame_offset(frame_offset: u32, sample_rate: Hz) -> Self {
        let offset_in_secs = frame_offset as f64 / MIDI_INPUT_FRAME_RATE.get();
        let offset_in_samples = (offset_in_secs * sample_rate.get()).round() as u64;
        SampleOffset(offset_in_samples)
    }

    pub fn to_midi_input_frame_offset(self, sample_rate: Hz) -> u32 {
        let offset_in_secs = self.0 as f64 / sample_rate.get();
        (offset_in_secs * MIDI_INPUT_FRAME_RATE.get()).round() as u32
    }

    pub fn new(value: u64) -> Self {
        SampleOffset(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn to_seconds(&self, sample_rate: Hz) -> DurationInSeconds {
        DurationInSeconds::new_panic(self.0 as f64 / sample_rate.get())
    }
}
