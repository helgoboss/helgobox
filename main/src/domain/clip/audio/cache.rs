use crate::domain::clip::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    SupplyRequest, SupplyResponse,
};
use crate::domain::clip::buffer::{AudioBufMut, OwnedAudioBuffer};
use core::cmp;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer};

pub struct AudioCache<S: AudioSupplier> {
    cached_data: Option<CachedData>,
    supplier: S,
}

struct CachedData {
    sample_rate: Hz,
    content: OwnedAudioBuffer,
}

impl<S: AudioSupplier> AudioCache<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            cached_data: None,
            supplier,
        }
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
        let mut content =
            OwnedAudioBuffer::new(self.supplier.channel_count(), self.supplier.frame_count());
        let original_sample_rate = self.supplier.sample_rate();
        let request = SupplyRequest {
            start_frame: 0,
            dest_sample_rate: original_sample_rate,
        };
        self.supplier.supply_audio(&request, content.to_buf_mut());
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

impl<S: AudioSupplier> AudioSupplier for AudioCache<S> {
    fn supply_audio(
        &self,
        request: &SupplyRequest,
        mut dest_buffer: AudioBufMut,
    ) -> SupplyResponse {
        if let Some(d) = &self.cached_data {
            // TODO-high Respect the requested sample rate (we need to resample manually).
            let buf = d.content.to_buf();
            let num_remaining_frames_in_source = buf.frame_count() - request.start_frame;
            let num_frames_written =
                cmp::min(num_remaining_frames_in_source, dest_buffer.frame_count());
            buf.slice(request.start_frame..)
                .copy_to(dest_buffer.slice_mut(0..num_frames_written));
            SupplyResponse { num_frames_written }
        } else {
            self.supplier.supply_audio(request, dest_buffer)
        }
    }

    fn channel_count(&self) -> usize {
        if let Some(d) = &self.cached_data {
            d.content.to_buf().channel_count()
        } else {
            self.supplier.channel_count()
        }
    }

    fn frame_count(&self) -> usize {
        if let Some(d) = &self.cached_data {
            d.content.to_buf().frame_count()
        } else {
            self.supplier.frame_count()
        }
    }

    fn sample_rate(&self) -> Hz {
        if let Some(d) = &self.cached_data {
            d.sample_rate
        } else {
            self.supplier.sample_rate()
        }
    }
}
