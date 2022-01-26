use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::{
    AudioSupplier, ExactFrameCount, MidiSupplier, SupplyAudioRequest, SupplyMidiRequest,
    SupplyResponse, WithFrameRate,
};
use reaper_medium::{BorrowedMidiEventList, Hz};

pub struct FlexibleSource<S> {
    supplier: S,
}

impl<S> FlexibleSource<S> {
    pub fn new(supplier: S) -> Self {
        Self { supplier }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }
}

impl<S: AudioSupplier> AudioSupplier for FlexibleSource<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        self.supplier.supply_audio(request, dest_buffer)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier> MidiSupplier for FlexibleSource<S> {
    fn supply_midi(
        &self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        self.supplier.supply_midi(request, event_list)
    }
}

impl<S: ExactFrameCount> ExactFrameCount for FlexibleSource<S> {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for FlexibleSource<S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}
