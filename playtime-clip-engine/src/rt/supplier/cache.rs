use crate::rt::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiSupplier, PreBufferFillRequest,
    PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyRequestInfo, SupplyResponse,
    WithMaterialInfo, WithSource,
};
use crate::ClipEngineResult;
use crossbeam_channel::{Receiver, Sender};
use reaper_medium::{BorrowedMidiEventList, OwnedPcmSource};
use std::fmt::Debug;

#[derive(Debug)]
pub struct Cache<S> {
    cached_data: Option<CachedData>,
    request_sender: Sender<CacheRequest>,
    response_channel: CacheResponseChannel,
    supplier: S,
}

#[derive(Debug)]
pub struct CacheResponseChannel {
    sender: Sender<CacheResponse>,
    receiver: Receiver<CacheResponse>,
}

impl Default for CacheResponseChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheResponseChannel {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::bounded(10);
        Self { sender, receiver }
    }
}

#[derive(Debug)]
pub enum CacheRequest {
    CacheSource {
        source: OwnedPcmSource,
        response_sender: Sender<CacheResponse>,
    },
    DiscardCachedData(CachedData),
}

#[derive(Debug)]
pub enum CacheResponse {
    CachedSource(CachedData),
}

#[derive(Debug)]
pub struct CachedData {
    material_info: AudioMaterialInfo,
    content: OwnedAudioBuffer,
}

impl<S: WithSource> Cache<S> {
    pub fn new(
        supplier: S,
        request_sender: Sender<CacheRequest>,
        response_channel: CacheResponseChannel,
    ) -> Self {
        Self {
            cached_data: None,
            request_sender,
            response_channel,
            supplier,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    /// If not cached already, triggers building the cache asynchronously, caching all supplied
    /// audio data in memory.
    ///
    /// Don't call in real-time thread. If this is necessary one day, no problem: Clone the source
    /// in advance.
    pub fn enable(&mut self) {
        if self.cached_data.is_some() || pcm_source_is_midi(self.supplier.source()) {
            return;
        }
        let request = CacheRequest::CacheSource {
            source: self.supplier.source().clone(),
            response_sender: self.response_channel.sender.clone(),
        };
        self.request_sender
            .try_send(request)
            .expect("couldn't send request to finish audio recording");
    }

    /// Disables the cache and clears it, releasing the consumed memory.
    pub fn disable(&mut self) {
        if let Some(cached_data) = self.cached_data.take() {
            let request = CacheRequest::DiscardCachedData(cached_data);
            self.request_sender.try_send(request).unwrap();
        }
    }

    fn process_worker_response(&mut self) {
        let response = match self.response_channel.receiver.try_recv() {
            Ok(r) => r,
            Err(_) => return,
        };
        match response {
            CacheResponse::CachedSource(cache_data) => {
                debug!("Cached audio material completely in memory");
                self.cached_data = Some(cache_data);
            }
        }
    }
}
pub fn keep_processing_cache_requests(receiver: Receiver<CacheRequest>) {
    while let Ok(request) = receiver.recv() {
        use CacheRequest::*;
        match request {
            CacheSource {
                mut source,
                response_sender,
            } => {
                let audio_material_info = match source.material_info() {
                    Ok(MaterialInfo::Audio(i)) => i,
                    _ => continue,
                };
                let mut content = OwnedAudioBuffer::new(
                    audio_material_info.channel_count,
                    audio_material_info.length,
                );
                let request = SupplyAudioRequest {
                    start_frame: 0,
                    dest_sample_rate: None,
                    info: SupplyRequestInfo {
                        audio_block_frame_offset: 0,
                        requester: "cache",
                        note: "",
                        is_realtime: false,
                    },
                    parent_request: None,
                    general_info: &Default::default(),
                };
                source.supply_audio(&request, &mut content.to_buf_mut());
                let cached_data = CachedData {
                    material_info: audio_material_info,
                    content,
                };
                // If the clip is not interested in the cached data anymore, so what.
                let _ = response_sender.try_send(CacheResponse::CachedSource(cached_data));
            }
            DiscardCachedData(_) => {}
        }
    }
}

impl<S: AudioSupplier + WithSource> AudioSupplier for Cache<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        self.process_worker_response();
        let d = match &self.cached_data {
            None => return self.supplier.supply_audio(request, dest_buffer),
            Some(d) => d,
        };
        let buf = d.content.to_buf();
        supply_audio_material(request, dest_buffer, d.material_info.sample_rate, |input| {
            transfer_samples_from_buffer(buf, input)
        })
    }
}

impl<S: MidiSupplier> MidiSupplier for Cache<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        // MIDI doesn't need caching.
        self.supplier.supply_midi(request, event_list)
    }
}

impl<S: WithMaterialInfo> WithMaterialInfo for Cache<S> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        if let Some(d) = &self.cached_data {
            Ok(MaterialInfo::Audio(d.material_info.clone()))
        } else {
            self.supplier.material_info()
        }
    }
}

impl<S: WithSource> WithSource for Cache<S> {
    fn source(&self) -> &OwnedPcmSource {
        self.supplier.source()
    }

    fn source_mut(&mut self) -> &mut OwnedPcmSource {
        self.supplier.source_mut()
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for Cache<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        if self.cached_data.is_some() {
            // No need to pre-buffer anything if we have everything cached in-memory anyway.
            return;
        }
        self.supplier.pre_buffer(request);
    }
}
