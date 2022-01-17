use crate::domain::clip::buffer::{AudioBuffer, BorrowedAudioBuffer, OwnedAudioBuffer};
use crossbeam_channel::{Receiver, Sender};
use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{
    BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer, PositionInSeconds,
};

/// Material to be stretched.
pub trait CopyToAudioBuffer {
    fn copy_to_audio_buffer(
        &self,
        start_time: PositionInSeconds,
        dest_buffer: impl AudioBuffer,
    ) -> Result<u32, &'static str>;
}

impl<'a> CopyToAudioBuffer for &'a BorrowedPcmSource {
    fn copy_to_audio_buffer(
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
pub struct StretchRequest<S: CopyToAudioBuffer, B: AudioBuffer> {
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
pub struct ReaperStretcher {
    state: Option<ReaperStretcherState>,
}

impl ReaperStretcher {
    pub fn new(source: &BorrowedPcmSource) -> Result<Self, &'static str> {
        let stretcher = Self {
            state: Some(ReaperStretcherState::Empty(EmptyReaperStretcher::new(
                source,
            )?)),
        };
        Ok(stretcher)
    }

    pub fn stretch(
        &mut self,
        req: StretchRequest<impl CopyToAudioBuffer, impl AudioBuffer>,
    ) -> Result<(), &'static str> {
        use ReaperStretcherState::*;
        let empty = match self.state.take().unwrap() {
            Empty(s) => s,
            Filled(s) => s.discard(),
        };
        let fill_req = FillRequest {
            source: req.source,
            start_time: req.start_time,
            tempo_factor: req.tempo_factor,
            dest_channel_count: req.dest_buffer.channel_count(),
            dest_frame_count: req.dest_buffer.frame_count(),
        };
        let filled = empty.fill(fill_req)?;
        self.state = Some(ReaperStretcherState::Empty(filled.stretch(req.dest_buffer)));
        Ok(())
    }
}

#[derive(Debug)]
enum ReaperStretcherState {
    Empty(EmptyReaperStretcher),
    Filled(FilledReaperStretcher),
}

#[derive(Debug)]
pub struct EmptyReaperStretcher {
    // TODO-high This is just temporary until we create an owned IReaperPitchShift struct in
    //  reaper-medium.
    api: &'static IReaperPitchShift,
    source_sample_rate: Hz,
}

#[derive(Debug)]
pub struct FillRequest<S: CopyToAudioBuffer> {
    /// Source material.
    pub source: S,
    /// Position within source where to start stretching.
    pub start_time: PositionInSeconds,
    /// 1.0 means original tempo.
    pub tempo_factor: f64,
    /// The number of channels in the destination buffer.
    pub dest_channel_count: usize,
    /// The number of frames to be filled.
    pub dest_frame_count: usize,
}

#[derive(Debug)]
pub struct FilledReaperStretcher {
    // TODO-high This is just temporary until we create an owned IReaperPitchShift struct in
    //  reaper-medium.
    api: &'static IReaperPitchShift,
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
    pub fn fill(
        self,
        req: FillRequest<impl CopyToAudioBuffer>,
    ) -> Result<FilledReaperStretcher, &'static str> {
        // Set parameters that can always vary
        let dest_nch = req.dest_channel_count;
        self.api.set_nch(dest_nch as _);
        self.api.set_tempo(req.tempo_factor);
        // Write original material into pitch shift buffer.
        let inner_block_length = (req.dest_frame_count as f64 * req.tempo_factor) as usize;
        let raw_stretch_buffer = self.api.GetBuffer(inner_block_length as _);
        let mut stretch_buffer = unsafe {
            BorrowedAudioBuffer::from_raw(raw_stretch_buffer, dest_nch, inner_block_length)
        };
        let read_sample_count = req
            .source
            .copy_to_audio_buffer(req.start_time, stretch_buffer)?;
        self.api.BufferDone(read_sample_count as _);
        let filled = FilledReaperStretcher {
            api: self.api,
            source_sample_rate: self.source_sample_rate,
        };
        Ok(filled)
    }
}

impl FilledReaperStretcher {
    /// Does the actual stretching of the contained audio material.
    pub fn stretch(mut self, mut dest_buffer: impl AudioBuffer) -> EmptyReaperStretcher {
        // Let time stretcher write the stretched material into the destination buffer.
        unsafe {
            self.api.GetSamples(
                dest_buffer.frame_count() as _,
                dest_buffer.data_as_mut_ptr(),
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
    lookahead_factor: usize,
    worker_sender: Sender<WorkerRequest>,
    // TODO-high We should use a one-shot channel.
    response_sender: Sender<AsyncStretchResponse>,
    response_receiver: Receiver<AsyncStretchResponse>,
    state: Option<AsyncStretcherState>,
}

#[derive(Debug)]
pub struct StretchEquipment {
    stretcher: EmptyReaperStretcher,
}

#[derive(Debug)]
pub enum AsyncStretcherState {
    Empty {
        equipment: StretchEquipment,
    },
    PreparingCurrentMaterial,
    PreparingNextMaterial {
        current_material: StretchedMaterial,
    },
    Full {
        equipment: StretchEquipment,
        current_material: StretchedMaterial,
        next_material: StretchedMaterial,
    },
}

#[derive(Debug)]
pub struct StretchedMaterial {
    start_time: PositionInSeconds,
    tempo_factor: f64,
    buffer: OwnedAudioBuffer,
}

impl StretchedMaterial {
    fn apply(
        &self,
        req: &StretchRequest<impl CopyToAudioBuffer, impl AudioBuffer>,
    ) -> Result<StretchedMaterialSuccess, StretchedMaterialError> {
        todo!()
    }

    fn end_time(&self) -> PositionInSeconds {
        todo!()
    }
}

enum StretchedMaterialSuccess {
    IsStillHot,
    IsNowObsolete,
}

enum StretchedMaterialError {
    RequestedPortionIsInPast,
    RequestedPortionIsInFuture,
}

impl AsyncStretcher {
    pub fn new(
        stretcher: EmptyReaperStretcher,
        lookahead_factor: usize,
        worker_sender: Sender<WorkerRequest>,
    ) -> Self {
        let (response_sender, response_receiver) =
            crossbeam_channel::bounded::<AsyncStretchResponse>(10);
        Self {
            lookahead_factor,
            worker_sender,
            response_sender,
            response_receiver,
            state: {
                let equipment = StretchEquipment { stretcher };
                Some(AsyncStretcherState::Empty { equipment })
            },
        }
    }

    /// Attempts to deliver stretched audio material.
    ///
    /// If the stretching is done asynchronously, this will only succeed if the requested material
    /// has been stretched already. If not, it will take this as a request to start stretching
    /// the *next* few blocks asynchronously, using the given parameters (so that consecutive calls
    /// will hopefully return successfully).
    pub fn try_stretch(
        &mut self,
        req: StretchRequest<&BorrowedPcmSource, impl AudioBuffer>,
    ) -> Result<(), &'static str> {
        use AsyncStretcherState::*;
        let outcome = match self.state.take().unwrap() {
            Empty { equipment } => {
                // We can't fulfill the incoming request but we can do our best to predict the
                // next few requests and make sure they will succeed.
                // Fill stretcher with input buffer covering material for the next few requests.
                let next_start_time = req.start_time_of_next_block_within_source()?;
                self.request_more_material(&req, equipment, next_start_time, None, None)?;
                Outcome {
                    next_state: PreparingCurrentMaterial,
                    result: Err("first request, cache miss"),
                }
            }
            PreparingCurrentMaterial => {
                if let Some(response) = self.poll_stretch_response() {
                    // We've got material!
                    match response.material.apply(&req) {
                        Ok(s) => {
                            // And it's the right one!
                            let next_start_time = response.material.end_time();
                            use StretchedMaterialSuccess::*;
                            let (next_state, obsolete_material) = match s {
                                // It's not exhausted yet.
                                IsStillHot => (
                                    PreparingNextMaterial {
                                        current_material: response.material,
                                    },
                                    None,
                                ),
                                // It's exhausted.
                                IsNowObsolete => {
                                    (PreparingCurrentMaterial, Some(response.material))
                                }
                            };
                            self.request_more_material(
                                &req,
                                response.equipment,
                                next_start_time,
                                obsolete_material,
                                None,
                            )?;
                            Outcome {
                                next_state,
                                result: Ok(()),
                            }
                        }
                        Err(_) => {
                            // It's not the right one :(
                            let next_start_time = req.start_time_of_next_block_within_source()?;
                            self.request_more_material(
                                &req,
                                response.equipment,
                                next_start_time,
                                Some(response.material),
                                None,
                            )?;
                            Outcome {
                                next_state: PreparingCurrentMaterial,
                                result: Err("we have material but not the right one"),
                            }
                        }
                    }
                } else {
                    Outcome {
                        next_state: PreparingCurrentMaterial,
                        result: Err("second request, still waiting for material"),
                    }
                }
            }
            PreparingNextMaterial { current_material } => {
                let response = self.poll_stretch_response();
                match current_material.apply(&req) {
                    Ok(s) => {
                        // The current material fulfilled the request.
                        use StretchedMaterialSuccess::*;
                        let next_state = match s {
                            IsStillHot => {
                                // And it's not exhausted yet.
                                if let Some(response) = response {
                                    // We've also got more material already. Enough material for
                                    // now!
                                    Full {
                                        equipment: response.equipment,
                                        current_material,
                                        next_material: response.material,
                                    }
                                } else {
                                    // Still waiting for more material.
                                    PreparingNextMaterial { current_material }
                                }
                            }
                            IsNowObsolete => {
                                // The current material is exhausted.
                                if let Some(response) = response {
                                    // And we've got new material already.
                                    // Let the new material become the current material and
                                    // request more material!
                                    let next_start_time = response.material.end_time();
                                    self.request_more_material(
                                        &req,
                                        response.equipment,
                                        next_start_time,
                                        Some(current_material),
                                        None,
                                    )?;
                                    PreparingNextMaterial {
                                        current_material: response.material,
                                    }
                                } else {
                                    // And we don't have new material yet.
                                    PreparingNextMaterial { current_material }
                                }
                            }
                        };
                        Outcome {
                            next_state,
                            result: Ok(()),
                        }
                    }
                    Err(_) => {
                        // The current material didn't fulfill the request.
                        if let Some(response) = response {
                            // But new material arrived just now. Let's see if this one works.
                            match response.material.apply(&req) {
                                Ok(_) => {
                                    // It does! And we assume it will work for the next few blocks,
                                    // too!
                                    // TODO-low We could check if the new material is now obsolete,
                                    //  but I think this shouldn't happen often. If it does, the
                                    //  next request will miss and request new material.
                                    let next_start_time = response.material.end_time();
                                    self.request_more_material(
                                        &req,
                                        response.equipment,
                                        next_start_time,
                                        Some(current_material),
                                        None,
                                    )?;
                                    Outcome {
                                        next_state: PreparingNextMaterial {
                                            current_material: response.material,
                                        },
                                        result: Ok(()),
                                    }
                                }
                                Err(_) => {
                                    // It doesn't work :(
                                    let next_start_time =
                                        req.start_time_of_next_block_within_source()?;
                                    self.request_more_material(
                                        &req,
                                        response.equipment,
                                        next_start_time,
                                        // Yes, all existing material became obsolete.
                                        Some(current_material),
                                        Some(response.material),
                                    )?;
                                    Outcome {
                                        next_state: PreparingCurrentMaterial,
                                        result: Err(
                                            "neither current nor new material fits while preparing next material",
                                        ),
                                    }
                                }
                            }
                        } else {
                            // And we also don't have new material yet :(
                            // There's nothing we can do at the moment except to wait for the new
                            // material.
                            Outcome {
                                next_state: PreparingNextMaterial { current_material },
                                result: Err("current material exhausted, waiting for new one"),
                            }
                        }
                    }
                }
            }
            Full {
                equipment,
                current_material,
                next_material,
            } => {
                // We have enough material, what a nice situation. We don't need to poll
                // the channel because we are not waiting for any material.
                match current_material.apply(&req) {
                    Ok(s) => {
                        // The current material fulfilled the request.
                        use StretchedMaterialSuccess::*;
                        let next_state = match s {
                            IsStillHot => {
                                // And it's not exhausted yet.
                                Full {
                                    equipment,
                                    current_material,
                                    next_material,
                                }
                            }
                            IsNowObsolete => {
                                // The current material is exhausted.
                                // Let the next material become the current one and request more!
                                let next_start_time = next_material.end_time();
                                self.request_more_material(
                                    &req,
                                    equipment,
                                    next_start_time,
                                    Some(current_material),
                                    None,
                                )?;
                                PreparingNextMaterial {
                                    current_material: next_material,
                                }
                            }
                        };
                        Outcome {
                            next_state,
                            result: Ok(()),
                        }
                    }
                    Err(s) => {
                        // The current material didn't fulfill the request.
                        // Let's check whether the next material works.
                        match next_material.apply(&req) {
                            Ok(_) => {
                                // It does! And we assume it will work for the next few blocks, too!
                                // TODO-low We could check if the new material is now obsolete,
                                //  but I think this shouldn't happen often. If it does, the
                                //  next request will miss and request new material.
                                let next_start_time = next_material.end_time();
                                self.request_more_material(
                                    &req,
                                    equipment,
                                    next_start_time,
                                    Some(current_material),
                                    None,
                                )?;
                                Outcome {
                                    next_state: PreparingNextMaterial {
                                        current_material: next_material,
                                    },
                                    result: Ok(()),
                                }
                            }
                            Err(_) => {
                                // It doesn't work :(
                                let next_start_time =
                                    req.start_time_of_next_block_within_source()?;
                                self.request_more_material(
                                    &req,
                                    equipment,
                                    next_start_time,
                                    // Yes, all existing material became obsolete.
                                    Some(current_material),
                                    Some(next_material),
                                )?;
                                Outcome {
                                    next_state: PreparingCurrentMaterial,
                                    result: Err("neither current nor new material fits while full"),
                                }
                            }
                        }
                    }
                }
            }
        };
        self.state = Some(outcome.next_state);
        outcome.result
    }

    fn request_more_material(
        &self,
        req: &StretchRequest<&BorrowedPcmSource, impl AudioBuffer>,
        equipment: StretchEquipment,
        start_time: PositionInSeconds,
        obsolete_material_1: Option<StretchedMaterial>,
        obsolete_material_2: Option<StretchedMaterial>,
    ) -> Result<(), &'static str> {
        let dest_channel_count = req.dest_buffer.channel_count();
        let dest_frame_count = req.dest_buffer.frame_count() * self.lookahead_factor;
        let fill_request = FillRequest {
            source: req.source,
            start_time,
            tempo_factor: req.tempo_factor,
            dest_channel_count,
            dest_frame_count,
        };
        // Let another thread to the actual stretching work.
        let async_stretch_request = AsyncStretchRequest {
            stretcher: equipment.stretcher.fill(fill_request)?,
            start_time,
            tempo_factor: req.tempo_factor,
            dest_channel_count,
            dest_frame_count,
            spare_buffer_1: obsolete_material_1.map(|m| m.buffer.into_inner()),
            spare_buffer_2: obsolete_material_2.map(|m| m.buffer.into_inner()),
        };
        let worker_request = WorkerRequest::Stretch {
            request: async_stretch_request,
            response_sender: self.response_sender.clone(),
        };
        self.worker_sender
            .try_send(worker_request)
            .map_err(|_| "couldn't contact worker")?;
        Ok(())
    }

    fn poll_stretch_response(&mut self) -> Option<AsyncStretchResponse> {
        self.response_receiver.try_iter().last()
    }
}

pub enum WorkerRequest {
    Stretch {
        request: AsyncStretchRequest,
        response_sender: Sender<AsyncStretchResponse>,
    },
}

#[derive(Debug)]
pub struct AsyncStretchRequest {
    stretcher: FilledReaperStretcher,
    start_time: PositionInSeconds,
    tempo_factor: f64,
    dest_channel_count: usize,
    dest_frame_count: usize,
    spare_buffer_1: Option<Vec<f64>>,
    spare_buffer_2: Option<Vec<f64>>,
}

#[derive(Debug)]
pub struct AsyncStretchResponse {
    equipment: StretchEquipment,
    material: StretchedMaterial,
}

fn keep_stretching(requests: Receiver<WorkerRequest>) {
    while let Ok(req) = requests.recv() {
        use WorkerRequest::*;
        match req {
            Stretch {
                request,
                response_sender,
            } => {
                let response = process_async_stretch_req(request);
                let _ = response_sender.try_send(response);
            }
        }
    }
}

fn process_async_stretch_req(mut req: AsyncStretchRequest) -> AsyncStretchResponse {
    let spare_buffers = [req.spare_buffer_1.take(), req.spare_buffer_2.take()];
    let mut dest_buffer = IntoIterator::into_iter(spare_buffers)
        .flatten()
        .find_map(|b| {
            OwnedAudioBuffer::try_recycle(b, req.dest_channel_count, req.dest_frame_count).ok()
        })
        .unwrap_or_else(|| OwnedAudioBuffer::new(req.dest_channel_count, req.dest_frame_count));
    let empty = req.stretcher.stretch(&mut dest_buffer);
    AsyncStretchResponse {
        equipment: StretchEquipment { stretcher: empty },
        material: StretchedMaterial {
            start_time: req.start_time,
            tempo_factor: req.tempo_factor,
            buffer: dest_buffer,
        },
    }
}

impl<B: AudioBuffer> StretchRequest<&BorrowedPcmSource, B> {
    fn start_time_of_next_block_within_source(&self) -> Result<PositionInSeconds, &'static str> {
        let source_block_duration = {
            let sample_rate = self
                .source
                .get_sample_rate()
                .ok_or("source without sample rate")?;
            let outer_duration = self.dest_buffer.frame_count() as f64 / sample_rate.get();
            DurationInSeconds::new(self.tempo_factor * outer_duration)
        };
        let source_length = self
            .source
            .get_length()
            .map_err(|_| "source without length")?;
        let start_time_of_next_block = (self.start_time + source_block_duration);
        (start_time_of_next_block % source_length).ok_or("source duration is zero")
    }
}

struct Outcome {
    next_state: AsyncStretcherState,
    result: Result<(), &'static str>,
}
