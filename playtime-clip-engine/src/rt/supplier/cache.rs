use crate::rt::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::audio_util::{supply_audio_material, transfer_samples_from_buffer};
use crate::rt::supplier::{
    get_source_frame_rate, AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier,
    PreBufferFillRequest, PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest,
    SupplyRequestInfo, SupplyResponse, WithFrameRate, WithSource,
};
use crossbeam_channel::{Receiver, Sender};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz, OwnedPcmSource};
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
    sample_rate: Hz,
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
                let original_sample_rate = get_source_frame_rate(&source);
                let mut content =
                    OwnedAudioBuffer::new(source.channel_count(), source.frame_count());
                let request = SupplyAudioRequest {
                    start_frame: 0,
                    dest_sample_rate: original_sample_rate,
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
                    sample_rate: original_sample_rate,
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
        supply_audio_material(request, dest_buffer, d.sample_rate, |input| {
            transfer_samples_from_buffer(buf, input)
        })
    }

    fn channel_count(&self) -> usize {
        if let Some(d) = &self.cached_data {
            d.content.to_buf().channel_count()
        } else {
            self.supplier.channel_count()
        }
    }
}

impl<S: WithFrameRate> WithFrameRate for Cache<S> {
    fn frame_rate(&self) -> Option<Hz> {
        if let Some(d) = &self.cached_data {
            Some(d.sample_rate)
        } else {
            self.supplier.frame_rate()
        }
    }
}

impl<S: MidiSupplier> MidiSupplier for Cache<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        // MIDI doesn't need caching.
        self.supplier.supply_midi(request, event_list)
    }
}

impl<S: ExactFrameCount> ExactFrameCount for Cache<S> {
    fn frame_count(&self) -> usize {
        if let Some(d) = &self.cached_data {
            d.content.to_buf().frame_count()
        } else {
            self.supplier.frame_count()
        }
    }
}
impl<S: ExactDuration> ExactDuration for Cache<S> {
    fn duration(&self) -> DurationInSeconds {
        self.supplier.duration()
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
