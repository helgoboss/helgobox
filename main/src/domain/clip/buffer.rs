use reaper_medium::PcmSourceTransfer;

pub trait AudioBuffer {
    fn frame_count(&self) -> usize;

    fn channel_count(&self) -> usize;

    fn data_as_mut_ptr(&mut self) -> *mut f64;

    fn interleaved_length(&self) -> usize {
        self.channel_count() * self.channel_count()
    }
}

// TODO-medium Replace this with one of the audio buffer types in the Rust ecosystem
//  (dasp_slice, audio, fon, ...)
#[derive(Debug)]
pub struct BorrowedAudioBuffer<'a> {
    data: &'a mut [f64],
    frame_count: usize,
    channel_count: usize,
}

impl<'a> BorrowedAudioBuffer<'a> {
    pub unsafe fn from_transfer(transfer: &PcmSourceTransfer) -> Self {
        Self::from_raw(
            transfer.samples(),
            transfer.nch() as _,
            transfer.length() as _,
        )
    }

    pub unsafe fn from_raw(data: *mut f64, channel_count: usize, frame_count: usize) -> Self {
        BorrowedAudioBuffer {
            data: unsafe {
                std::slice::from_raw_parts_mut(data, (channel_count * frame_count) as _)
            },
            frame_count,
            channel_count,
        }
    }
}

impl<'a> AudioBuffer for BorrowedAudioBuffer<'a> {
    fn frame_count(&self) -> usize {
        self.frame_count
    }

    fn channel_count(&self) -> usize {
        self.channel_count
    }

    fn data_as_mut_ptr(&mut self) -> *mut f64 {
        self.data.as_mut_ptr()
    }
}
