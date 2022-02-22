use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::fade_util::{apply_fade_in, apply_fade_out};
use crate::rt::supplier::{
    midi_util, AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier, PreBufferFillRequest,
    PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz};

#[derive(Debug)]
pub struct StartEndFader<S> {
    supplier: S,
    fade_in_enabled: bool,
    fade_out_enabled: bool,
}

impl<S> StartEndFader<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            fade_in_enabled: false,
            fade_out_enabled: false,
        }
    }

    pub fn set_fade_in_enabled(&mut self, enabled: bool) {
        self.fade_in_enabled = enabled;
    }

    pub fn set_fade_out_enabled(&mut self, enabled: bool) {
        self.fade_out_enabled = enabled;
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }
}

impl<S: AudioSupplier + ExactFrameCount> AudioSupplier for StartEndFader<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let response = self.supplier.supply_audio(request, dest_buffer);
        if self.fade_in_enabled {
            apply_fade_in(dest_buffer, request.start_frame);
        }
        if self.fade_out_enabled {
            apply_fade_out(
                dest_buffer,
                request.start_frame,
                self.supplier.frame_count(),
            );
        }
        response
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier + ExactFrameCount> MidiSupplier for StartEndFader<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let response = self.supplier.supply_midi(request, event_list);
        if self.fade_out_enabled {
            if response.status.reached_end() {
                debug!("Silence MIDI at source end");
                midi_util::silence_midi(event_list);
            }
        }
        response
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for StartEndFader<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.supplier.pre_buffer(request);
    }
}

impl<S: WithFrameRate> WithFrameRate for StartEndFader<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: ExactFrameCount> ExactFrameCount for StartEndFader<S> {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}

impl<S: ExactDuration + WithFrameRate + ExactFrameCount> ExactDuration for StartEndFader<S> {
    fn duration(&self) -> DurationInSeconds {
        self.supplier.duration()
    }
}
