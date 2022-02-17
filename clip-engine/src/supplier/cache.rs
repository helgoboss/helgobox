use crate::buffer::{AudioBuf, AudioBufMut, OwnedAudioBuffer};
use crate::source_util::pcm_source_is_midi;
use crate::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    supply_source_material, transfer_samples_from_buffer, AudioSupplier, ExactFrameCount,
    MidiSupplier, SourceMaterialRequest, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
    WithFrameRate,
};
use crate::{get_source_frame_rate, ExactDuration, SupplyRequestInfo};
use core::cmp;
use crossbeam_channel::{Receiver, Sender};
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, OwnedPcmSource,
    PcmSourceTransfer,
};

#[derive(Debug)]
pub struct Cache {
    cached_data: Option<CachedData>,
    request_sender: Sender<CacheRequest>,
    response_channel: CacheResponseChannel,
    source: OwnedPcmSource,
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

impl Cache {
    pub fn new(
        source: OwnedPcmSource,
        request_sender: Sender<CacheRequest>,
        response_channel: CacheResponseChannel,
    ) -> Self {
        Self {
            cached_data: None,
            request_sender,
            response_channel,
            source,
        }
    }

    pub fn source(&self) -> &OwnedPcmSource {
        &self.source
    }

    pub fn source_mut(&mut self) -> &mut OwnedPcmSource {
        &mut self.source
    }

    /// If not cached already, triggers building the cache asynchronously, caching all supplied
    /// audio data in memory.
    ///
    /// Don't call in real-time thread. If this is necessary one day, no problem: Clone the source
    /// in advance.
    pub fn enable(&mut self) {
        if self.cached_data.is_some() || pcm_source_is_midi(&self.source) {
            return;
        }
        let request = CacheRequest::CacheSource {
            source: self.source.clone(),
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

impl AudioSupplier for Cache {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        self.process_worker_response();
        let d = match &self.cached_data {
            None => return self.source.supply_audio(request, dest_buffer),
            Some(d) => d,
        };
        let buf = d.content.to_buf();
        supply_source_material(request, dest_buffer, d.sample_rate, |input| {
            transfer_samples_from_buffer(buf, input)
        })
    }

    fn channel_count(&self) -> usize {
        if let Some(d) = &self.cached_data {
            d.content.to_buf().channel_count()
        } else {
            self.source.channel_count()
        }
    }
}

impl WithFrameRate for Cache {
    fn frame_rate(&self) -> Option<Hz> {
        if let Some(d) = &self.cached_data {
            Some(d.sample_rate)
        } else {
            self.source.frame_rate()
        }
    }
}

impl MidiSupplier for Cache {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        // MIDI doesn't need caching.
        self.source.supply_midi(request, event_list)
    }
}

impl ExactFrameCount for Cache {
    fn frame_count(&self) -> usize {
        if let Some(d) = &self.cached_data {
            d.content.to_buf().frame_count()
        } else {
            self.source.frame_count()
        }
    }
}
impl ExactDuration for Cache {
    fn duration(&self) -> DurationInSeconds {
        self.source.duration()
    }
}
