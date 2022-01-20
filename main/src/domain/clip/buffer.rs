use core::cmp;
use reaper_medium::{BorrowedPcmSource, PcmSourceTransfer, PositionInSeconds};
use std::collections::Bound;
use std::ops::{Index, Range, RangeBounds, RangeFrom};

#[derive(Debug)]
pub struct OwnedAudioBuffer {
    data: Vec<f64>,
    channel_count: usize,
    frame_count: usize,
}

impl OwnedAudioBuffer {
    /// Creates an owned audio buffer with the given topology.
    pub fn new(channel_count: usize, frame_count: usize) -> Self {
        Self {
            data: vec![0.0; channel_count * frame_count],
            channel_count,
            frame_count,
        }
    }

    pub fn to_buf(&self) -> AudioBuf {
        AudioBuf {
            data: self.data.as_slice(),
            frame_count: 0,
            channel_count: 0,
        }
    }

    pub fn to_buf_mut(&mut self) -> AudioBufMut {
        AudioBufMut {
            data: self.data.as_mut_slice(),
            frame_count: 0,
            channel_count: 0,
        }
    }

    /// Attempts to create an owned audio buffer with the given topology by reusing the given vec.
    ///
    /// Returns an error if the given vec is not large enough.
    pub fn try_recycle(
        mut data: Vec<f64>,
        channel_count: usize,
        frame_count: usize,
    ) -> Result<Self, &'static str> {
        let min_capacity = channel_count * frame_count;
        if data.capacity() < min_capacity {
            return Err("given vector doesn't have enough capacity");
        }
        data.resize(min_capacity, 0.0);
        let buffer = Self {
            data,
            channel_count,
            frame_count,
        };
        Ok(buffer)
    }

    pub fn into_inner(self) -> Vec<f64> {
        self.data
    }
}

// TODO-medium Replace this with one of the audio buffer types in the Rust ecosystem
//  (dasp_slice, audio, fon, ...)
#[derive(Debug)]
pub struct AbstractAudioBuf<T: AsRef<[f64]>> {
    data: T,
    frame_count: usize,
    channel_count: usize,
}

pub type AudioBuf<'a> = AbstractAudioBuf<&'a [f64]>;
pub type AudioBufMut<'a> = AbstractAudioBuf<&'a mut [f64]>;

impl<'a> AudioBufMut<'a> {
    pub unsafe fn from_transfer(transfer: &PcmSourceTransfer) -> Self {
        Self::from_raw(
            transfer.samples(),
            transfer.nch() as _,
            transfer.length() as _,
        )
    }

    pub unsafe fn from_raw(data: *mut f64, channel_count: usize, frame_count: usize) -> Self {
        AudioBufMut {
            data: unsafe {
                std::slice::from_raw_parts_mut(data, (channel_count * frame_count) as _)
            },
            frame_count,
            channel_count,
        }
    }
}

impl<T: AsRef<[f64]>> AbstractAudioBuf<T> {
    pub fn interleaved_length(&self) -> usize {
        self.channel_count * self.channel_count
    }

    /// Destination buffer must have the same number of channels.
    pub fn copy_to(&self, mut dest: AudioBufMut) -> Result<(), &'static str> {
        let channel_count = self.channel_count();
        if channel_count != dest.channel_count() {
            return Err("different channel counts");
        }
        if dest.frame_count() > self.frame_count() {
            return Err("end of copied range out of source buffer bounds");
        }
        dest.data_as_mut_slice().copy_from_slice(self.data.as_ref());
        Ok(())
    }

    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn data_as_slice(&self) -> &[f64] {
        self.data.as_ref()
    }

    pub fn slice(&self, bounds: impl RangeBounds<usize>) -> AudioBuf {
        let desc = self.prepare_slice(bounds);
        AudioBuf {
            data: &self.data.as_ref()[desc.data_start_index..desc.data_end_index],
            frame_count: desc.new_frame_count,
            channel_count: desc.channel_count,
        }
    }

    fn prepare_slice(&self, bounds: impl RangeBounds<usize>) -> SliceDescriptor {
        use Bound::*;
        let start_frame = match bounds.start_bound() {
            Included(i) => *i,
            Excluded(i) => *i + 1,
            Unbounded => 0,
        };
        let end_frame = match bounds.end_bound() {
            Included(i) => *i - 1,
            Excluded(i) => *i,
            Unbounded => self.frame_count,
        };
        if start_frame >= self.frame_count || end_frame > self.frame_count {
            panic!("slice range out of bounds");
        }
        if start_frame > end_frame {
            panic!("slice start greater than end");
        }
        SliceDescriptor {
            new_frame_count: end_frame - start_frame,
            data_start_index: start_frame * self.channel_count,
            data_end_index: end_frame * self.channel_count,
            channel_count: self.channel_count,
        }
    }
}

struct SliceDescriptor {
    new_frame_count: usize,
    data_start_index: usize,
    data_end_index: usize,
    channel_count: usize,
}

impl<T: AsRef<[f64]> + AsMut<[f64]>> AbstractAudioBuf<T> {
    pub fn data_as_mut_slice(&mut self) -> &mut [f64] {
        self.data.as_mut()
    }

    pub fn data_as_mut_ptr(&mut self) -> *mut f64 {
        self.data.as_mut().as_mut_ptr()
    }

    pub fn slice_mut(&mut self, bounds: impl RangeBounds<usize>) -> AudioBufMut {
        let desc = self.prepare_slice(bounds);
        AudioBufMut {
            data: &mut self.data.as_mut()[desc.data_start_index..desc.data_end_index],
            frame_count: desc.new_frame_count,
            channel_count: desc.channel_count,
        }
    }
}

/// Material to be stretched.
pub trait CopyToAudioBuffer {
    fn copy_to_audio_buffer(
        &self,
        start_frame: usize,
        dest_buffer: AudioBufMut,
    ) -> Result<usize, &'static str>;
}

impl<'a> CopyToAudioBuffer for &'a BorrowedPcmSource {
    fn copy_to_audio_buffer(
        &self,
        start_frame: usize,
        mut dest_buffer: AudioBufMut,
    ) -> Result<usize, &'static str> {
        let mut transfer = PcmSourceTransfer::default();
        let sample_rate = self.get_sample_rate().ok_or("source without sample rate")?;
        let start_time =
            (start_frame as f64 / sample_rate.get()) % self.get_length().unwrap().get();
        let start_time = PositionInSeconds::new(start_time);
        transfer.set_time_s(start_time);
        transfer.set_sample_rate(sample_rate);
        // TODO-high Here we need to handle repeat/not-repeat
        unsafe {
            transfer.set_nch(dest_buffer.channel_count() as _);
            transfer.set_length(dest_buffer.frame_count() as _);
            transfer.set_samples(dest_buffer.data_as_mut_ptr());
            self.get_samples(&transfer);
        }
        Ok(dest_buffer.frame_count())
    }
}
