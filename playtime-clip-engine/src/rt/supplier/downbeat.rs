use crate::conversion_util::convert_duration_in_seconds_to_frames;
use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{
    AudioSupplier, MaterialInfo, MidiSupplier, PositionTranslationSkill, PreBufferFillRequest,
    PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyRequest, SupplyRequestInfo,
    SupplyResponse, WithMaterialInfo,
};
use crate::ClipEngineResult;
use playtime_api::persistence::PositiveBeat;
use reaper_medium::{BorrowedMidiEventList, Bpm, DurationInSeconds, MidiFrameOffset};

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

    pub fn downbeat_frame(&self) -> usize {
        self.downbeat_frame
    }

    pub fn set_downbeat_in_beats(&mut self, beat: PositiveBeat, tempo: Bpm) -> ClipEngineResult<()>
    where
        S: WithMaterialInfo,
    {
        let source_frame_frate = self.supplier.material_info()?.frame_rate();
        let bps = tempo.get() / 60.0;
        let second = beat.get() / bps;
        let frame = convert_duration_in_seconds_to_frames(
            DurationInSeconds::new(second),
            source_frame_frate,
        );
        self.set_downbeat_frame(frame);
        Ok(())
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

    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    ) {
        self.supplier.release_notes(frame_offset, event_list);
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for Downbeat<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        if !self.enabled || self.downbeat_frame == 0 {
            return self.supplier.pre_buffer(request);
        }
        let inner_request = PreBufferFillRequest {
            start_frame: request.start_frame + self.downbeat_frame as isize,
        };
        self.supplier.pre_buffer(inner_request);
    }
}

impl<S: PositionTranslationSkill> PositionTranslationSkill for Downbeat<S> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        let effective_play_pos = if self.enabled {
            play_pos + self.downbeat_frame as isize
        } else {
            play_pos
        };
        self.supplier
            .translate_play_pos_to_source_pos(effective_play_pos)
    }
}

impl<S: WithMaterialInfo> WithMaterialInfo for Downbeat<S> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.supplier.material_info()
    }
}

struct DownbeatRequestData {
    start_frame: isize,
    info: SupplyRequestInfo,
}
