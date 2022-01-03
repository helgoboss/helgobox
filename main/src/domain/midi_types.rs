use reaper_medium::{Hz, MidiFrameOffset};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Event<T> {
    offset: SampleOffset,
    payload: T,
}

impl<T: Copy> Event<T> {
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

    pub fn from_frame_offset(frame_offset: MidiFrameOffset, sample_rate: Hz) -> Self {
        let offset_in_secs = frame_offset.get() as f64 / 1024000.0;
        let offset_in_samples = (offset_in_secs * sample_rate.get()).round() as u64;
        SampleOffset(offset_in_samples)
    }

    pub fn new(value: u64) -> Self {
        SampleOffset(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}
