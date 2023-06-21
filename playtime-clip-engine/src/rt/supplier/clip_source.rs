use crate::rt::supplier::{
    AudioSupplier, CacheableSource, MaterialInfo, MidiSupplier, ReaperClipSource,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithMaterialInfo,
};
use crate::rt::AudioBufMut;
use crate::ClipEngineResult;
use reaper_medium::BorrowedMidiEventList;
use std::path::Path;

#[derive(Clone, Debug)]
pub enum RtClipSource {
    Reaper(ReaperClipSource),
}

impl AudioSupplier for RtClipSource {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        match self {
            RtClipSource::Reaper(s) => s.supply_audio(request, dest_buffer),
        }
    }
}

impl MidiSupplier for RtClipSource {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        match self {
            RtClipSource::Reaper(s) => s.supply_midi(request, event_list),
        }
    }
}

impl WithMaterialInfo for RtClipSource {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        match self {
            RtClipSource::Reaper(s) => s.material_info(),
        }
    }
}
