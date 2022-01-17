use crate::domain::clip::buffer::{AudioBuffer, BorrowedAudioBuffer};
use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{BorrowedPcmSource, Hz, PcmSourceTransfer, PositionInSeconds};

/// Easy-to-use facade for sync or async time stretching.
pub trait Stretch {
    /// Attempts to deliver stretched audio material.
    ///
    /// If the stretching is done asynchronously, this will only succeed if the requested material
    /// has been stretched already. If not, it will take this as a request to start stretching
    /// the *next* few blocks asynchronously, using the given parameters (so that consecutive calls
    /// will hopefully return successfully).
    fn stretch(
        &mut self,
        request: StretchRequest<impl StretchSource, impl AudioBuffer>,
    ) -> Result<(), &'static str>;
}

/// Material to be stretched.
pub trait StretchSource {
    fn read(
        &self,
        start_time: PositionInSeconds,
        dest_buffer: impl AudioBuffer,
    ) -> Result<u32, &'static str>;
}

impl<'a> StretchSource for &'a BorrowedPcmSource {
    fn read(
        &self,
        start_time: PositionInSeconds,
        mut dest_buffer: impl AudioBuffer,
    ) -> Result<u32, &'static str> {
        let mut transfer = PcmSourceTransfer::default();
        transfer.set_time_s(start_time);
        let sample_rate = self.get_sample_rate().ok_or("source without sample rate")?;
        transfer.set_sample_rate(sample_rate);
        unsafe {
            transfer.set_nch(dest_buffer.channel_count() as _);
            transfer.set_length(dest_buffer.frame_count() as _);
            transfer.set_samples(dest_buffer.data_as_mut_ptr());
            self.get_samples(&transfer);
        }
        Ok(transfer.samples_out() as _)
    }
}

/// A request for stretching source material.
#[derive(Debug)]
pub struct StretchRequest<S: StretchSource, B: AudioBuffer> {
    /// Source material.
    pub source: S,
    /// Position within source where to start stretching.
    pub start_time: PositionInSeconds,
    /// 1.0 means original tempo.
    pub tempo_factor: f64,
    /// The final time stretched samples should end up here.
    pub dest_buffer: B,
}

#[derive(Debug)]
pub struct ReaperStretcher<B: AudioBuffer> {
    state: Option<ReaperStretcherState<B>>,
}

impl<B: AudioBuffer> ReaperStretcher<B> {
    pub fn new(source: &BorrowedPcmSource) -> Result<Self, &'static str> {
        let stretcher = Self {
            state: Some(ReaperStretcherState::Empty(EmptyReaperStretcher::new(
                source,
            )?)),
        };
        Ok(stretcher)
    }
}

impl<B: AudioBuffer> Stretch for ReaperStretcher<B> {
    fn stretch(
        &mut self,
        req: StretchRequest<impl StretchSource, impl AudioBuffer>,
    ) -> Result<(), &'static str> {
        use ReaperStretcherState::*;
        let empty = match self.state.take().unwrap() {
            Empty(s) => s,
            Filled(s) => s.discard(),
        };
        let filled = empty.fill(req)?;
        self.state = Some(ReaperStretcherState::Empty(filled.stretch()));
        Ok(())
    }
}

#[derive(Debug)]
enum ReaperStretcherState<B: AudioBuffer> {
    Empty(EmptyReaperStretcher),
    Filled(FilledReaperStretcher<B>),
}

#[derive(Debug)]
pub struct EmptyReaperStretcher {
    // TODO-high This is just temporary until we create an owned IReaperPitchShift struct in
    //  reaper-medium.
    api: &'static IReaperPitchShift,
    source_sample_rate: Hz,
}

#[derive(Debug)]
pub struct FilledReaperStretcher<B: AudioBuffer> {
    // TODO-high This is just temporary until we create an owned IReaperPitchShift struct in
    //  reaper-medium.
    api: &'static IReaperPitchShift,
    dest_buffer: B,
    source_sample_rate: Hz,
}

impl EmptyReaperStretcher {
    /// Creates an empty time stretcher instance based on the constant properties of the given audio
    /// source (which is going to be time-stretched).
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

    /// Fills the time stretcher with audio material, returning a filled time stretcher.
    pub fn fill<B: AudioBuffer>(
        self,
        req: StretchRequest<impl StretchSource, B>,
    ) -> Result<FilledReaperStretcher<B>, &'static str> {
        // Set parameters that can always vary
        let dest_nch = req.dest_buffer.channel_count();
        self.api.set_nch(dest_nch as _);
        self.api.set_tempo(req.tempo_factor);
        // Write original material into pitch shift buffer.
        let inner_block_length = (req.dest_buffer.frame_count() as f64 * req.tempo_factor) as usize;
        let raw_stretch_buffer = self.api.GetBuffer(inner_block_length as _);
        let mut stretch_buffer = unsafe {
            BorrowedAudioBuffer::from_raw(raw_stretch_buffer, dest_nch, inner_block_length)
        };
        let read_sample_count = req.source.read(req.start_time, stretch_buffer)?;
        self.api.BufferDone(read_sample_count as _);
        let filled = FilledReaperStretcher {
            api: self.api,
            source_sample_rate: self.source_sample_rate,
            dest_buffer: req.dest_buffer,
        };
        Ok(filled)
    }
}

impl<B: AudioBuffer> FilledReaperStretcher<B> {
    /// Does the actual stretching of the contained audio material.
    pub fn stretch(mut self) -> EmptyReaperStretcher {
        // Let time stretcher write the stretched material into the destination buffer.
        unsafe {
            self.api.GetSamples(
                self.dest_buffer.frame_count() as _,
                self.dest_buffer.data_as_mut_ptr(),
            );
        };
        // TODO-high Might have to zero the remaining frames
        EmptyReaperStretcher {
            api: self.api,
            source_sample_rate: self.source_sample_rate,
        }
    }

    /// Discards the currently filled material.
    fn discard(self) -> EmptyReaperStretcher {
        EmptyReaperStretcher {
            api: self.api,
            source_sample_rate: self.source_sample_rate,
        }
    }
}

#[derive(Debug)]
pub struct AsyncStretcher {
    worker: crossbeam_channel::Sender<WorkerRequest>,
    response: crossbeam_channel::Receiver<WorkerResponse>,
    state: Option<AsyncStretcherState>,
}

#[derive(Debug)]
pub enum AsyncStretcherState {
    Empty {
        stretcher: EmptyReaperStretcher,
    },
    PreparingCurrentBuffer,
    PreparingNextBuffer {
        current_buffer: StretchedBuffer,
    },
    Full {
        stretcher: EmptyReaperStretcher,
        current_buffer: StretchedBuffer,
        next_buffer: StretchedBuffer,
    },
}

#[derive(Debug)]
pub struct StretchedBuffer {
    start_time: PositionInSeconds,
    tempo_factor: f64,
    channel_count: u32,
    buffer: Vec<f64>,
}

impl StretchedBuffer {
    fn process(
        &self,
        req: &StretchRequest<impl StretchSource, impl AudioBuffer>,
    ) -> StretchBufferProcessingResult {
        todo!()
    }
}

enum StretchBufferProcessingResult {
    Successful,
    RequestedPortionIsInPast,
    RequestedPortionIsInFuture,
}

impl AsyncStretcher {
    pub fn new(stretcher: EmptyReaperStretcher, buffer_size: usize) -> Self {
        let (worker, worker_receiver) = crossbeam_channel::bounded::<WorkerRequest>(10);
        let (response_sender, response) = crossbeam_channel::bounded::<WorkerResponse>(10);
        Self {
            worker,
            response,
            state: Some(AsyncStretcherState::Empty { stretcher }),
        }
    }
}

impl Stretch for AsyncStretcher {
    fn stretch(
        &mut self,
        request: StretchRequest<impl StretchSource, impl AudioBuffer>,
    ) -> Result<(), &'static str> {
        use AsyncStretcherState::*;
        match self.state.take().unwrap() {
            Empty { stretcher } => {
                todo!();
                Err("first request, cache miss")
            }
            PreparingCurrentBuffer => todo!(),
            PreparingNextBuffer { .. } => todo!(),
            Full { .. } => todo!(),
        }
    }
}

enum WorkerRequest {
    Stretch { inner: EmptyReaperStretcher },
}

enum WorkerResponse {}
