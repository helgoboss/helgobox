use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{BorrowedPcmSource, Hz, PcmSourceTransfer, PositionInSeconds};

pub trait TimeStretcher {
    /// Attempts to deliver stretched audio material without blocking.
    ///
    /// For time stretching algorithms that don't work in real-time, this will only succeed if the
    /// requested material has been pre-stretched already. If not, it will take this as a request
    /// to start stretching the *next* few blocks asynchronously, using the given parameters
    /// (so that consecutive calls will hopefully return successfully).
    fn try_non_blocking_stretch(&self, request: TimeStretchRequest) -> Result<(), &'static str>;
}

pub struct TimeStretchRequest<'a> {
    /// The request needs the source to read the original samples on-the-fly into the time stretch
    /// buffer.
    ///
    /// We don't pass a source buffer slice because that would necessarily mean an extra copy.
    pub source: &'a BorrowedPcmSource,
    /// Where in the given source to start.
    pub start_time: PositionInSeconds,
    /// 1.0 means original tempo.
    pub tempo_factor: f64,
    /// The final time stretched samples should end up here.
    pub destination_buffer: *mut f64,
    pub destination_frame_count: u32,
    pub destination_channel_count: u32,
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
    fn try_non_blocking_stretch(&self, req: TimeStretchRequest) -> Result<(), &'static str> {
        // Set parameters that can always vary
        self.api.set_nch(req.destination_channel_count as _);
        self.api.set_tempo(req.tempo_factor);
        // Write original material into pitch shift buffer.
        let inner_block_length = (req.destination_frame_count as f64 * req.tempo_factor) as u32;
        let stretch_buffer = self.api.GetBuffer(inner_block_length as _);
        let mut transfer = PcmSourceTransfer::default();
        transfer.set_time_s(req.start_time);
        transfer.set_sample_rate(self.source_sample_rate);
        unsafe {
            transfer.set_nch(req.destination_channel_count as _);
            transfer.set_length(inner_block_length as _);
            transfer.set_samples(stretch_buffer);
            req.source.get_samples(&transfer);
        }
        self.api.BufferDone(transfer.samples_out());
        // Let time stretcher write the stretched material into the destination buffer.
        unsafe {
            self.api
                .GetSamples(req.destination_frame_count as _, req.destination_buffer);
        };

        // TODO-high Might have to zero the remaining frames
        Ok(())
    }
}
