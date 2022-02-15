use crate::buffer::{AudioBuf, AudioBufMut, OwnedAudioBuffer};
use crate::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    supply_source_material, transfer_samples_from_buffer, AudioSupplier, ExactFrameCount,
    MidiSupplier, SourceMaterialRequest, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
    WithFrameRate,
};
use crate::SupplyRequestInfo;
use core::cmp;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer,
};

pub struct Cache<S> {
    cached_data: Option<CachedData>,
    supplier: S,
}

struct CachedData {
    sample_rate: Hz,
    content: OwnedAudioBuffer,
}

impl<S: AudioSupplier + ExactFrameCount + WithFrameRate> Cache<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            cached_data: None,
            supplier,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    /// Enables the cache and builds it, caching all supplied audio data in memory if it hasn't
    /// been cached already.
    ///
    /// Shouldn't be called in a real-time thread.
    pub fn enable(&mut self) {
        if self.cached_data.is_some() {
            // Already cached.
            return;
        }
        let original_sample_rate = match self.supplier.frame_rate() {
            None => {
                // Nothing to cache.
                return;
            }
            Some(r) => r,
        };
        let mut content =
            OwnedAudioBuffer::new(self.supplier.channel_count(), self.supplier.frame_count());
        let request = SupplyAudioRequest {
            start_frame: 0,
            dest_sample_rate: original_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "cache",
                note: "",
            },
            parent_request: None,
            general_info: &Default::default(),
        };
        self.supplier
            .supply_audio(&request, &mut content.to_buf_mut());
        let cached_data = CachedData {
            sample_rate: original_sample_rate,
            content,
        };
        self.cached_data = Some(cached_data);
    }

    /// Disables the cache and clears it, releasing the consumed memory.
    ///
    /// Shouldn't be called in a real-time thread.
    pub fn disable(&mut self) {
        self.cached_data = None;
    }
}

impl<S: AudioSupplier + ExactFrameCount> AudioSupplier for Cache<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let d = match &self.cached_data {
            None => return self.supplier.supply_audio(request, dest_buffer),
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
