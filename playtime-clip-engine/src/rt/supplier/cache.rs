use std::fmt::Debug;
use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use playtime_api::persistence::AudioCacheBehavior;
use reaper_medium::{BorrowedMidiEventList, MidiFrameOffset};

use crate::rt::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiSupplier, PositionTranslationSkill,
    RtClipSource, SupplyAudioRequest, SupplyMidiRequest, SupplyRequestInfo, SupplyResponse,
    WithMaterialInfo, WithSource, WithSupplier,
};
use crate::ClipEngineResult;

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
        source: RtClipSource,
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
    file_path: PathBuf,
    content: OwnedAudioBuffer,
}

impl CachedData {
    fn is_still_valid(&self, source: &RtClipSource) -> bool {
        source.reaper_source().get_file_name(|path| {
            if let Some(path) = path {
                path == self.file_path
            } else {
                false
            }
        })
    }
}

impl<S> WithSupplier for Cache<S> {
    type Supplier = S;

    fn supplier(&self) -> &Self::Supplier {
        &self.supplier
    }

    fn supplier_mut(&mut self) -> &mut Self::Supplier {
        &mut self.supplier
    }
}

impl<S: WithSource> Cache<S> {
    pub fn new(supplier: S, request_sender: Sender<CacheRequest>) -> Self {
        Self {
            cached_data: None,
            request_sender,
            response_channel: CacheResponseChannel::new(),
            supplier,
        }
    }

    pub fn set_audio_cache_behavior(&mut self, cache_behavior: AudioCacheBehavior) {
        use AudioCacheBehavior::*;
        let cache_enabled = match cache_behavior {
            DirectFromDisk => false,
            CacheInMemory => true,
        };
        if cache_enabled {
            self.enable();
        } else {
            self.disable();
        }
    }

    /// If not cached already, triggers building the cache asynchronously, caching all supplied
    /// audio data in memory.
    ///
    /// Don't call in real-time thread. If this is necessary one day, no problem: Clone the source
    /// in advance.
    fn enable(&mut self) {
        let source = match self.supplier.source() {
            None => return,
            Some(s) => s,
        };
        if let Some(cached_data) = self.cached_data.take() {
            if cached_data.is_still_valid(source) {
                self.cached_data = Some(cached_data);
                return;
            }
            self.request_sender.discard_cached_data(cached_data);
        }
        if pcm_source_is_midi(source.reaper_source()) {
            return;
        }
        self.request_sender
            .cache_source(source.clone(), self.response_channel.sender.clone());
    }

    /// Disables the cache and clears it, releasing the consumed memory.
    fn disable(&mut self) {
        if let Some(cached_data) = self.cached_data.take() {
            self.request_sender.discard_cached_data(cached_data);
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
                let _ = cache_source(&mut source, response_sender);
            }
            DiscardCachedData(_) => {}
        }
    }
}

fn cache_source(
    source: &mut RtClipSource,
    response_sender: Sender<CacheResponse>,
) -> ClipEngineResult<()> {
    let audio_material_info = match source.material_info() {
        Ok(MaterialInfo::Audio(i)) => i,
        _ => return Err("no audio source"),
    };
    let file_path = source
        .reaper_source()
        .get_file_name(|path| path.map(|p| p.to_path_buf()))
        .ok_or("source without file name")?;
    let mut content = OwnedAudioBuffer::new(
        audio_material_info.channel_count,
        audio_material_info.frame_count,
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
        file_path,
        content,
    };
    response_sender
        .try_send(CacheResponse::CachedSource(cached_data))
        .map_err(|_| "clip not interested in cached data anymore")?;
    Ok(())
}

trait CacheRequestSender {
    fn cache_source(&self, source: RtClipSource, response_sender: Sender<CacheResponse>);

    fn discard_cached_data(&self, data: CachedData);

    fn send_request(&self, request: CacheRequest);
}

impl CacheRequestSender for Sender<CacheRequest> {
    fn cache_source(&self, source: RtClipSource, response_sender: Sender<CacheResponse>) {
        let request = CacheRequest::CacheSource {
            source,
            response_sender,
        };
        self.send_request(request);
    }

    fn discard_cached_data(&self, data: CachedData) {
        let request = CacheRequest::DiscardCachedData(data);
        self.send_request(request);
    }

    fn send_request(&self, request: CacheRequest) {
        self.try_send(request).unwrap();
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
        supply_audio_material(request, dest_buffer, d.material_info.frame_rate, |input| {
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

impl<S: PositionTranslationSkill> PositionTranslationSkill for Cache<S> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        self.supplier.translate_play_pos_to_source_pos(play_pos)
    }
}
