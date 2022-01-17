use reaper_medium::PcmSourceTransfer;

// TODO-medium Replace this with one of the audio buffer types in the Rust ecosystem
//  (dasp_slice, audio, fon, ...)
#[derive(Debug)]
pub struct AudioBuffer<'a> {
    pub data: &'a mut [f64],
    pub frame_count: u32,
    pub channel_count: u32,
}

impl<'a> AudioBuffer<'a> {
    pub unsafe fn from_transfer(transfer: &PcmSourceTransfer) -> Self {
        Self::from_raw(
            transfer.samples(),
            transfer.nch() as _,
            transfer.length() as _,
        )
    }

    pub unsafe fn from_raw(data: *mut f64, channel_count: u32, frame_count: u32) -> Self {
        AudioBuffer {
            data: unsafe {
                std::slice::from_raw_parts_mut(data, (channel_count * frame_count) as _)
            },
            frame_count,
            channel_count,
        }
    }

    pub fn interleaved_length(&self) -> usize {
        (self.channel_count * self.channel_count) as _
    }
}
