use crate::ClipEngineResult;
use derivative::Derivative;
use std::collections::Bound;
use std::fmt::Debug;
use std::ops::RangeBounds;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct OwnedAudioBuffer {
    #[derivative(Debug = "ignore")]
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
            frame_count: self.frame_count,
            channel_count: self.channel_count,
        }
    }

    pub fn to_buf_mut(&mut self) -> AudioBufMut {
        AudioBufMut {
            data: self.data.as_mut_slice(),
            frame_count: self.frame_count,
            channel_count: self.channel_count,
        }
    }

    /// Attempts to create an owned audio buffer with the given topology by reusing the given vec.
    ///
    /// Returns an error if the given vec is not large enough.
    pub fn try_recycle(
        mut data: Vec<f64>,
        channel_count: usize,
        frame_count: usize,
    ) -> ClipEngineResult<Self> {
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
#[derive(Copy, Clone, Debug)]
pub struct AbstractAudioBuf<T: AsRef<[f64]>> {
    data: T,
    frame_count: usize,
    channel_count: usize,
}

pub type AudioBuf<'a> = AbstractAudioBuf<&'a [f64]>;
pub type AudioBufMut<'a> = AbstractAudioBuf<&'a mut [f64]>;

impl<'a> AudioBuf<'a> {
    /// # Safety
    ///
    /// REAPER can crash if you pass an invalid pointer.
    pub unsafe fn from_raw(data: *mut f64, channel_count: usize, frame_count: usize) -> Self {
        AudioBuf {
            data: std::slice::from_raw_parts(data, (channel_count * frame_count) as _),
            frame_count,
            channel_count,
        }
    }
}

impl<'a> AudioBufMut<'a> {
    /// # Panics
    ///
    /// Panics if requested frame count is zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the size of the given data chunk isn't large enough.
    pub fn from_slice(
        chunk: &'a mut [f64],
        channel_count: usize,
        frame_count: usize,
    ) -> Result<Self, &'static str> {
        if frame_count == 0 {
            panic!("attempt to create buffer from sliced data with a frame count of zero");
        }
        let required_slice_length = channel_count * frame_count;
        if chunk.len() < required_slice_length {
            return Err("given slice not large enough");
        }
        let buf = AudioBufMut {
            data: &mut chunk[0..required_slice_length],
            frame_count,
            channel_count,
        };
        Ok(buf)
    }

    /// # Panics
    ///
    /// Panics if requested frame count is zero.
    ///
    /// # Safety
    ///
    /// REAPER can crash if you pass an invalid pointer.
    pub unsafe fn from_raw(data: *mut f64, channel_count: usize, frame_count: usize) -> Self {
        if frame_count == 0 {
            panic!("attempt to create buffer from raw data with a frame count of zero");
        }
        AudioBufMut {
            data: std::slice::from_raw_parts_mut(data, (channel_count * frame_count) as _),
            frame_count,
            channel_count,
        }
    }
}

impl<T: AsRef<[f64]>> AbstractAudioBuf<T> {
    /// Destination buffer must have the same number of channels and frames.
    pub fn copy_to(&self, dest: &mut AudioBufMut) {
        if dest.channel_count() != self.channel_count() {
            panic!("different channel counts");
        }
        if dest.frame_count() != self.frame_count() {
            panic!("different frame counts");
        }
        dest.data_as_mut_slice().copy_from_slice(self.data.as_ref());
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

    pub fn data_as_mut_ptr(&self) -> *mut f64 {
        self.data.as_ref().as_ptr() as *mut _
    }

    pub fn sample_value_at(&self, index: SampleIndex) -> Option<f64> {
        self.data
            .as_ref()
            .get(index.frame * self.channel_count + index.channel)
            .copied()
    }

    pub fn slice(&self, bounds: impl RangeBounds<usize>) -> AudioBuf {
        let desc = self.prepare_slice(bounds);
        if desc.new_frame_count == 0 {
            panic!("slicing results in buffer with a frame count of zero");
        }
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

    pub fn slice_mut(&mut self, bounds: impl RangeBounds<usize>) -> AudioBufMut {
        let desc = self.prepare_slice(bounds);
        AudioBufMut {
            data: &mut self.data.as_mut()[desc.data_start_index..desc.data_end_index],
            frame_count: desc.new_frame_count,
            channel_count: desc.channel_count,
        }
    }

    pub fn modify_frames(&mut self, mut f: impl FnMut(SampleDescriptor) -> f64) {
        for frame_index in 0..self.frame_count {
            for ch in 0..self.channel_count {
                // TODO-high-performance For performance we might want to skip the bound checks. This is
                //  very hot code.
                let sample_value = &mut self.data.as_mut()[frame_index * self.channel_count + ch];
                let descriptor = SampleDescriptor {
                    index: SampleIndex {
                        frame: frame_index,
                        channel: ch,
                    },
                    value: *sample_value,
                };
                *sample_value = f(descriptor);
            }
        }
    }

    /// Fills the buffer with zero samples.
    ///
    /// This is not always necessary, it depends on the situation. The preview register pre-zeroes
    /// buffers but the time stretcher and resampler doesn't, which results in beeps if we don't
    /// clear it.
    pub fn clear(&mut self) {
        self.data.as_mut().fill(0.0);
    }
}

pub struct SampleDescriptor {
    pub index: SampleIndex,
    pub value: f64,
}

#[derive(Copy, Clone)]
pub struct SampleIndex {
    pub channel: usize,
    pub frame: usize,
}

impl SampleIndex {
    pub fn new(channel: usize, frame: usize) -> Self {
        Self { channel, frame }
    }
}
