use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{
    AudioSupplier, MidiSupplier, PreBufferFillRequest, PreBufferSourceSkill, SupplyAudioRequest,
    SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use helgoboss_midi::{
    RawShortMessage, ShortMessage, ShortMessageFactory, StructuredShortMessage, U7,
};
use reaper_high::Reaper;
use reaper_medium::{BorrowedMidiEventList, Db, Hz, VolumeSliderValue};
use std::cmp;

#[derive(Debug)]
pub struct Amplifier<S> {
    supplier: S,
    volume: Db,
    derived_volume_factor: f64,
}

impl<S> Amplifier<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            volume: Db::ZERO_DB,
            derived_volume_factor: 1.0,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.volume = volume;
        // TODO-medium Maybe improve the volume factor
        self.derived_volume_factor = Reaper::get().medium_reaper().db2slider(volume).get()
            / VolumeSliderValue::ZERO_DB.get();
    }
}

impl<S: AudioSupplier> AudioSupplier for Amplifier<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let response = self.supplier.supply_audio(request, dest_buffer);
        if self.volume != Db::ZERO_DB {
            // TODO-medium Maybe improve the volume factor
            dest_buffer.modify_frames(|sample| sample.value * self.derived_volume_factor);
        }
        response
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for Amplifier<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier> MidiSupplier for Amplifier<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let response = self.supplier.supply_midi(request, event_list);
        if self.volume != Db::ZERO_DB {
            for event in event_list.iter_mut() {
                if let StructuredShortMessage::NoteOn {
                    channel,
                    key_number,
                    velocity,
                } = event.message().to_structured()
                {
                    let adjusted_velocity =
                        (self.derived_volume_factor * velocity.get() as f64).round() as u8;
                    let amplified_msg = RawShortMessage::note_on(
                        channel,
                        key_number,
                        U7::new(cmp::min(127u8, adjusted_velocity)),
                    );
                    event.set_message(amplified_msg);
                }
            }
        }
        response
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for Amplifier<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.supplier.pre_buffer(request);
    }
}
