use std::fmt::Debug;
use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};
use playtime_api::persistence::AudioCacheBehavior;

use crate::rt::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::rt::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, AutoDelegatingMidiSupplier,
    AutoDelegatingPositionTranslationSkill, CacheableSource, MaterialInfo, SupplyAudioRequest,
    SupplyRequestInfo, SupplyResponse, WithCacheableSource, WithMaterialInfo, WithSupplier,
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
    CacheSource(CacheSourceRequest),
    DiscardCachedData(CachedData),
}

#[derive(Debug)]
pub struct CacheSourceRequest {
    audio_material_info: AudioMaterialInfo,
    source_file: PathBuf,
    source: Box<dyn CacheableSource>,
    response_sender: Sender<CacheResponse>,
}

#[derive(Debug)]
pub enum CacheResponse {
    CachedSource(CachedData),
}

#[derive(Debug)]
pub struct CachedData {
    audio_material_info: AudioMaterialInfo,
    source_file: PathBuf,
    content: OwnedAudioBuffer,
}

impl CachedData {
    fn is_still_valid(&self, source_file: &Path) -> bool {
        self.source_file == source_file
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

impl<S: WithCacheableSource + WithMaterialInfo> Cache<S> {
    pub fn new(supplier: S, request_sender: Sender<CacheRequest>) -> Self {
        Self {
            cached_data: None,
            request_sender,
            response_channel: CacheResponseChannel::new(),
            supplier,
        }
    }

    pub fn set_audio_cache_behavior(
        &mut self,
        cache_behavior: AudioCacheBehavior,
    ) -> ClipEngineResult<()> {
        use AudioCacheBehavior::*;
        let cache_enabled = match cache_behavior {
            DirectFromDisk => false,
            CacheInMemory => true,
        };
        if cache_enabled {
            self.enable()?;
        } else {
            self.disable();
        }
        Ok(())
    }

    /// If not cached already, triggers building the cache asynchronously, caching all supplied
    /// audio data in memory.
    ///
    /// Don't call in real-time thread. If this is necessary one day, no problem: Clone the source
    /// in advance.
    fn enable(&mut self) -> ClipEngineResult<()> {
        // A cacheable source might not be available at all times.
        let source = self
            .supplier
            .cacheable_source()
            .ok_or("no cacheable source available")?;
        let material_info = source.material_info()?;
        // Reject MIDI
        let MaterialInfo::Audio(audio_material_info) = material_info else {
            return Err("source can't be cached because it's MIDI");
        };
        // If source has no file name, we can't check if the cache is still valid.
        let source_file = source
            .file_name()
            .ok_or("source doesn't have any file name")?;
        // Look at currently cached data
        if let Some(cached_data) = self.cached_data.take() {
            if cached_data.is_still_valid(source_file) {
                self.cached_data = Some(cached_data);
                return Ok(());
            }
            self.request_sender.discard_cached_data(cached_data);
        }
        // Cache
        let req = CacheSourceRequest {
            audio_material_info,
            source_file: source_file.to_path_buf(),
            source: source.duplicate(),
            response_sender: self.response_channel.sender.clone(),
        };
        self.request_sender.cache_source(req);
        Ok(())
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
            CacheSource(req) => {
                let _ = cache_source(req);
            }
            DiscardCachedData(_) => {}
        }
    }
}

fn cache_source(mut req: CacheSourceRequest) -> ClipEngineResult<()> {
    let mut content = OwnedAudioBuffer::new(
        req.audio_material_info.channel_count,
        req.audio_material_info.frame_count,
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
    req.source.supply_audio(&request, &mut content.to_buf_mut());
    let cached_data = CachedData {
        audio_material_info: req.audio_material_info,
        source_file: req.source_file,
        content,
    };
    req.response_sender
        .try_send(CacheResponse::CachedSource(cached_data))
        .map_err(|_| "clip not interested in cached data anymore")?;
    Ok(())
}

trait CacheRequestSender {
    fn cache_source(&self, req: CacheSourceRequest);

    fn discard_cached_data(&self, data: CachedData);

    fn send_request(&self, request: CacheRequest);
}

impl CacheRequestSender for Sender<CacheRequest> {
    fn cache_source(&self, req: CacheSourceRequest) {
        let request = CacheRequest::CacheSource(req);
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

impl<S: AudioSupplier + WithCacheableSource> AudioSupplier for Cache<S> {
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
        supply_audio_material(
            request,
            dest_buffer,
            d.audio_material_info.frame_rate,
            |input| transfer_samples_from_buffer(buf, input),
        )
    }
}

impl<S: WithMaterialInfo> WithMaterialInfo for Cache<S> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        if let Some(d) = &self.cached_data {
            Ok(MaterialInfo::Audio(d.audio_material_info.clone()))
        } else {
            self.supplier.material_info()
        }
    }
}

impl<S> AutoDelegatingMidiSupplier for Cache<S> {}
impl<S> AutoDelegatingPositionTranslationSkill for Cache<S> {}
