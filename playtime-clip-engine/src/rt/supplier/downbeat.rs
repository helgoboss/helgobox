use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{
    AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier, PreBufferFillRequest,
    PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyRequest, SupplyRequestInfo,
    SupplyResponse, WithFrameRate,
};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz};

#[derive(Debug)]
pub struct Downbeat<S> {
    supplier: S,
    enabled: bool,
    downbeat_frame: usize,
}

impl<S> Downbeat<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            enabled: false,
            downbeat_frame: 0,
            // downbeat_frame: 1_024_000 / 2,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_downbeat_frame(&mut self, frame: usize) {
        self.downbeat_frame = frame;
    }

    fn get_data(&self, request: &impl SupplyRequest) -> Option<DownbeatRequestData> {
        if !self.enabled || self.downbeat_frame == 0 {
            return None;
        }
        let data = DownbeatRequestData {
            start_frame: request.start_frame() + self.downbeat_frame as isize,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info().audio_block_frame_offset,
                requester: "downbeat-request",
                note: "",
                is_realtime: request.info().is_realtime,
            },
        };
        Some(data)
    }
}

impl<S: AudioSupplier> AudioSupplier for Downbeat<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = match self.get_data(request) {
            None => {
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Some(d) => d,
        };
        let inner_request = SupplyAudioRequest {
            start_frame: data.start_frame,
            info: data.info,
            dest_sample_rate: request.dest_sample_rate,
            parent_request: Some(request),
            general_info: request.general_info,
        };
        self.supplier.supply_audio(&inner_request, dest_buffer)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier> MidiSupplier for Downbeat<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = match self.get_data(request) {
            None => {
                return self.supplier.supply_midi(request, event_list);
            }
            Some(d) => d,
        };
        let inner_request = SupplyMidiRequest {
            start_frame: data.start_frame,
            info: data.info,
            dest_frame_count: request.dest_frame_count,
            dest_sample_rate: request.dest_sample_rate,
            parent_request: Some(request),
            general_info: request.general_info,
        };
        self.supplier.supply_midi(&inner_request, event_list)
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for Downbeat<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        if !self.enabled || self.downbeat_frame == 0 {
            return self.supplier.pre_buffer(request);
        }
        let inner_request = PreBufferFillRequest {
            start_frame: request.start_frame + self.downbeat_frame as isize,
            ..request
        };
        self.supplier.pre_buffer(inner_request);
    }
}

impl<S: WithFrameRate> WithFrameRate for Downbeat<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: ExactFrameCount> ExactFrameCount for Downbeat<S> {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}

impl<S: ExactDuration> ExactDuration for Downbeat<S> {
    fn duration(&self) -> DurationInSeconds {
        self.supplier.duration()
    }
}

struct DownbeatRequestData {
    start_frame: isize,
    info: SupplyRequestInfo,
}
