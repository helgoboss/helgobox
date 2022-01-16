use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{BorrowedPcmSource, Hz, PcmSourceTransfer, PositionInSeconds};

pub trait TimeStretcher {
    /// Attempts to deliver stretched audio material.
    ///
    /// If the stretching is done asynchronously, this will only succeed if the requested material
    /// has been stretched already. If not, it will take this as a request to start stretching
    /// the *next* few blocks asynchronously, using the given parameters (so that consecutive calls
    /// will hopefully return successfully).
    fn stretch<S: TimeStretchSource>(
        &self,
        request: TimeStretchRequest<S>,
    ) -> Result<(), &'static str>;
}

pub trait TimeStretchSource {
    fn read(&self, destination_buffer: AudioBuffer) -> Result<u32, &'static str>;
}

pub struct PcmSourceSection<'a> {
    pub pcm_source: &'a BorrowedPcmSource,
    pub start_time: PositionInSeconds,
}

impl<'a> TimeStretchSource for PcmSourceSection<'a> {
    fn read(&self, dest_buffer: AudioBuffer) -> Result<u32, &'static str> {
        let mut transfer = PcmSourceTransfer::default();
        transfer.set_time_s(self.start_time);
        let sample_rate = self
            .pcm_source
            .get_sample_rate()
            .ok_or("source without sample rate")?;
        transfer.set_sample_rate(sample_rate);
        unsafe {
            transfer.set_nch(dest_buffer.channel_count as _);
            transfer.set_length(dest_buffer.frame_count as _);
            transfer.set_samples(dest_buffer.data.as_mut_ptr());
            self.pcm_source.get_samples(&transfer);
        }
        Ok(transfer.samples_out() as _)
    }
}

pub struct TimeStretchRequest<'a, S: TimeStretchSource> {
    /// Source material.
    pub source: S,
    /// 1.0 means original tempo.
    pub tempo_factor: f64,
    /// The final time stretched samples should end up here.
    pub dest_buffer: AudioBuffer<'a>,
}

#[derive(Debug)]
pub struct ReaperTimeStretcher {
    // TODO-high This is just temporary until we create an owned IReaperPitchShift struct in
    //  reaper-medium.
    api: &'static IReaperPitchShift,
    source_sample_rate: Hz,
}

impl ReaperTimeStretcher {
    /// Creates a time stretcher instance based on the constant properties of the given audio source
    /// (which is going to be time-stretched with this instance).
    pub fn new(source: &BorrowedPcmSource) -> Result<Self, &'static str> {
        let api = Reaper::get()
            .medium_reaper()
            .low()
            .ReaperGetPitchShiftAPI(REAPER_PITCHSHIFT_API_VER);
        let api = unsafe { &*api };
        let source_sample_rate = source
            .get_sample_rate()
            .ok_or("doesn't look like audio source")?;
        api.set_srate(source_sample_rate.get());
        let stretcher = Self {
            api,
            source_sample_rate,
        };
        Ok(stretcher)
    }
}

impl TimeStretcher for ReaperTimeStretcher {
    fn stretch<S: TimeStretchSource>(
        &self,
        req: TimeStretchRequest<S>,
    ) -> Result<(), &'static str> {
        // Set parameters that can always vary
        let dest_nch = req.dest_buffer.channel_count;
        self.api.set_nch(dest_nch as _);
        self.api.set_tempo(req.tempo_factor);
        // Write original material into pitch shift buffer.
        let inner_block_length = (req.dest_buffer.frame_count as f64 * req.tempo_factor) as u32;
        let raw_stretch_buffer = self.api.GetBuffer(inner_block_length as _);
        let mut stretch_buffer =
            unsafe { AudioBuffer::from_raw(raw_stretch_buffer, dest_nch, inner_block_length) };
        let read_sample_count = req.source.read(stretch_buffer)?;
        self.api.BufferDone(read_sample_count as _);
        // Let time stretcher write the stretched material into the destination buffer.
        unsafe {
            self.api.GetSamples(
                req.dest_buffer.frame_count as _,
                req.dest_buffer.data.as_mut_ptr(),
            );
        };
        // TODO-high Might have to zero the remaining frames
        Ok(())
    }
}

// TODO-medium Replace this with one of the audio buffer types in the Rust ecosystem
//  (dasp_slice, audio, fon, ...)
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
}
