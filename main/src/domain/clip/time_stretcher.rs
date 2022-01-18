use crate::domain::clip::buffer::{AudioBuffer, BorrowedAudioBuffer, OwnedAudioBuffer};
use crossbeam_channel::{Receiver, Sender};
use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{
    BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer, PositionInSeconds,
};
use std::fmt::{Display, Formatter};

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
        // TODO-high Here we need to handle repeat/not-repeat
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
    /// Position within source from which to start stretching.
    pub start_time: PositionInSeconds,
    /// 1.0 means original tempo.
    pub tempo_factor: f64,
    /// The final time stretched samples should end up here.
    pub dest_buffer: B,
}

impl<S: CopyToAudioBuffer, B: AudioBuffer> StretchRequest<S, B> {
    pub fn stretch_info(&self, source_info: &SourceInfo) -> StretchInfo {
        let input = StretchInfoInput {
            start_frame: {
                // TODO-medium What if we use frames everywhere and only convert to times when REAPER needs it?
                (self.start_time.get() * source_info.sample_rate.get()) as usize
            },
            tempo_factor: self.tempo_factor,
            stretched_frame_count: self.dest_buffer.frame_count(),
            source_frame_count: source_info.frame_count(),
        };
        StretchInfo::new(input)
    }

    fn modulo_end_frame(&self, source_info: &SourceInfo) -> usize {
        self.stretch_info(source_info).modulo_end_frame
    }
}

#[derive(Debug)]
pub struct StretchInfoInput {
    /// Position within source from which to start stretching.
    start_frame: usize,
    /// 1.0 means original tempo.
    tempo_factor: f64,
    /// Frame count of destination buffer (which contains the stretched material).
    stretched_frame_count: usize,
    /// Total length of source.
    source_frame_count: usize,
}

#[derive(Debug)]
pub struct StretchInfo {
    /// The tempo factor.
    pub tempo_factor: f64,
    /// Position within source from which to start stretching.
    pub start_frame: usize,
    /// Non-inclusive non-modulo end position in source (or right to source).
    pub hypothetical_end_frame: usize,
    /// Non-inclusive modulo end position in source.
    pub modulo_end_frame: usize,
    /// Length of the stretched material.
    ///
    /// Corresponds to the length of the buffer containing the stretched material.
    pub stretched_frame_count: usize,
    /// Length of the original material.
    pub unstretched_frame_count: usize,
}

impl StretchInfo {
    pub fn new(input: StretchInfoInput) -> Self {
        // The higher the tempo (factor) the longer the material that we need to stretch.
        let unstretched_frame_count =
            (input.stretched_frame_count as f64 * input.tempo_factor) as usize;
        let hypothetical_end_frame = input.start_frame + unstretched_frame_count;
        let modulo_end_frame = hypothetical_end_frame % input.source_frame_count;
        StretchInfo {
            tempo_factor: input.tempo_factor,
            start_frame: input.start_frame,
            stretched_frame_count: input.stretched_frame_count,
            unstretched_frame_count,
            hypothetical_end_frame,
            modulo_end_frame,
        }
    }
}

#[derive(Debug)]
pub struct ReaperStretcher {
    state: Option<ReaperStretcherState>,
}

impl ReaperStretcher {
    pub fn new(source_sample_rate: Hz) -> Self {
        let empty = EmptyReaperStretcher::new(source_sample_rate);
        Self {
            state: Some(ReaperStretcherState::Empty(empty)),
        }
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
}

unsafe impl Send for EmptyReaperStretcher {}

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
}

unsafe impl Send for FilledReaperStretcher {}

impl EmptyReaperStretcher {
    /// Creates an empty time stretcher instance based on the constant properties of the given audio
    /// source (which is going to be time-stretched).
    pub fn new(source_sample_rate: Hz) -> Self {
        let api = Reaper::get()
            .medium_reaper()
            .low()
            .ReaperGetPitchShiftAPI(REAPER_PITCHSHIFT_API_VER);
        let api = unsafe { &*api };
        api.set_srate(source_sample_rate.get());
        Self { api }
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
        let unstretched_frame_count = (req.dest_frame_count as f64 * req.tempo_factor) as usize;
        let raw_stretch_buffer = self.api.GetBuffer(unstretched_frame_count as _);
        let mut stretch_buffer = unsafe {
            BorrowedAudioBuffer::from_raw(raw_stretch_buffer, dest_nch, unstretched_frame_count)
        };
        let read_sample_count = req
            .source
            .copy_to_audio_buffer(req.start_time, stretch_buffer)?;
        self.api.BufferDone(read_sample_count as _);
        let filled = FilledReaperStretcher { api: self.api };
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
        EmptyReaperStretcher { api: self.api }
    }

    /// Discards the currently filled material.
    fn discard(self) -> EmptyReaperStretcher {
        EmptyReaperStretcher { api: self.api }
    }
}

#[derive(Debug)]
pub struct AsyncStretcher {
    lookahead_factor: usize,
    worker_sender: Sender<StretchWorkerRequest>,
    // TODO-high We should use a one-shot channel.
    response_sender: Sender<AsyncStretchResponse>,
    response_receiver: Receiver<AsyncStretchResponse>,
    state: Option<AsyncStretcherState>,
    source_info: SourceInfo,
}

#[derive(Debug)]
pub struct SourceInfo {
    sample_rate: Hz,
    length: DurationInSeconds,
}

impl SourceInfo {
    pub fn from_source(source: &BorrowedPcmSource) -> Result<Self, &'static str> {
        let info = Self {
            sample_rate: source
                .get_sample_rate()
                .ok_or("source without sample rate")?,
            length: {
                let length = source.get_length().map_err(|_| "source without length")?;
                if length == DurationInSeconds::ZERO {
                    return Err("source is empty");
                }
                length
            },
        };
        Ok(info)
    }

    pub fn sample_rate(&self) -> Hz {
        self.sample_rate
    }

    pub fn length(&self) -> DurationInSeconds {
        self.length
    }

    pub fn frame_count(&self) -> usize {
        (self.length.get() * self.sample_rate.get()) as usize
    }
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
    /// Position within the source that was the origin of stretching.
    start_frame: usize,
    /// Tempo factor.
    tempo_factor: f64,
    /// Audio buffer that contains the stretched material.
    buffer: OwnedAudioBuffer,
}

impl StretchedMaterial {
    /// Checks if this material contains the requested material and if yes, copies it to the buffer.
    ///
    /// If not, it returns an error.
    fn apply(
        &self,
        req: &mut StretchRequest<&BorrowedPcmSource, impl AudioBuffer>,
        source_info: &SourceInfo,
    ) -> Result<MaterialStatus, &'static str> {
        if req.dest_buffer.channel_count() != self.buffer.channel_count() {
            return Err("channel count mismatch");
        }
        if req.tempo_factor != self.tempo_factor {
            return Err("tempo factor mismatch");
        }
        let req_info = req.stretch_info(source_info);
        let material_info = self.stretch_info(source_info);
        if req_info.start_frame < material_info.start_frame {
            return Err("requested portion starts before material portion");
        }
        if req_info.hypothetical_end_frame > material_info.hypothetical_end_frame {
            dbg!(source_info, material_info);
            return Err("requested portion ends after material portion");
        }
        let material_frame_offset = req_info.start_frame - material_info.start_frame;
        let dest_frame_count = req.dest_buffer.frame_count();
        self.buffer.copy_to(
            &mut req.dest_buffer,
            material_frame_offset,
            0,
            dest_frame_count,
        )?;
        let status = if req_info.hypothetical_end_frame == material_info.hypothetical_end_frame {
            MaterialStatus::IsNowObsolete
        } else {
            MaterialStatus::IsStillHot
        };
        Ok(status)
    }

    pub fn stretch_info(&self, source_info: &SourceInfo) -> StretchInfo {
        let input = StretchInfoInput {
            start_frame: self.start_frame,
            tempo_factor: self.tempo_factor,
            stretched_frame_count: self.buffer.frame_count(),
            source_frame_count: source_info.frame_count(),
        };
        StretchInfo::new(input)
    }

    /// Returns the end frame within the source (modulo).
    pub fn modulo_end_frame(&self, source_info: &SourceInfo) -> usize {
        self.stretch_info(source_info).modulo_end_frame
    }
}

enum MaterialStatus {
    IsStillHot,
    IsNowObsolete,
}

impl AsyncStretcher {
    pub fn new(
        stretcher: EmptyReaperStretcher,
        lookahead_factor: usize,
        worker_sender: Sender<StretchWorkerRequest>,
        source_info: SourceInfo,
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
            source_info,
        }
    }

    /// Attempts to deliver stretched audio material.
    ///
    /// If the stretching is done asynchronously, this will only succeed if the requested material
    /// has been stretched already. If not, it will take this as a request to start stretching
    /// the *next* few blocks asynchronously, using the given parameters (so that consecutive calls
    /// will hopefully return successfully).
    ///
    /// Returns success/failure messages for debugging purposes.
    pub fn try_stretch(
        &mut self,
        mut req: StretchRequest<&BorrowedPcmSource, impl AudioBuffer>,
    ) -> Result<&'static str, TryStretchError> {
        use AsyncStretcherState::*;
        let outcome = match self.state.take().unwrap() {
            Empty { equipment } => {
                // We can't fulfill the incoming request but we can do our best to predict the
                // next few requests and make sure they will succeed.
                // Fill stretcher with input buffer covering material for the next few requests.
                let next_start_frame = req.modulo_end_frame(&self.source_info);
                self.request_more_material(&req, equipment, next_start_frame, None, None)?;
                Outcome {
                    next_state: PreparingCurrentMaterial,
                    result: Err(e("first request, cache miss", "")),
                }
            }
            PreparingCurrentMaterial => {
                if let Some(response) = self.poll_stretch_response() {
                    // We've got material!
                    match response.material.apply(&mut req, &self.source_info) {
                        Ok(s) => {
                            // And it's the right one!
                            let next_start_frame =
                                response.material.modulo_end_frame(&self.source_info);
                            use MaterialStatus::*;
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
                                next_start_frame,
                                obsolete_material,
                                None,
                            )?;
                            Outcome {
                                next_state,
                                result: Ok("current material just arrived and it works"),
                            }
                        }
                        Err(msg) => {
                            // It's not the right one :(
                            let next_start_frame = req.modulo_end_frame(&self.source_info);
                            self.request_more_material(
                                &req,
                                response.equipment,
                                next_start_frame,
                                Some(response.material),
                                None,
                            )?;
                            Outcome {
                                next_state: PreparingCurrentMaterial,
                                result: Err(e("we have material but not the right one", msg)),
                            }
                        }
                    }
                } else {
                    Outcome {
                        next_state: PreparingCurrentMaterial,
                        result: Err(e("second request, still waiting for material", "")),
                    }
                }
            }
            PreparingNextMaterial { current_material } => {
                let response = self.poll_stretch_response();
                match current_material.apply(&mut req, &self.source_info) {
                    Ok(s) => {
                        // The current material fulfilled the request.
                        use MaterialStatus::*;
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
                                    let next_start_frame =
                                        response.material.modulo_end_frame(&self.source_info);
                                    self.request_more_material(
                                        &req,
                                        response.equipment,
                                        next_start_frame,
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
                            result: Ok("current material works while waiting for next one"),
                        }
                    }
                    Err(msg1) => {
                        // The current material didn't fulfill the request.
                        if let Some(response) = response {
                            // But new material arrived just now. Let's see if this one works.
                            match response.material.apply(&mut req, &self.source_info) {
                                Ok(_) => {
                                    // It does! And we assume it will work for the next few blocks,
                                    // too!
                                    // TODO-low We could check if the new material is now obsolete,
                                    //  but I think this shouldn't happen often. If it does, the
                                    //  next request will miss and request new material.
                                    let next_start_time =
                                        response.material.modulo_end_frame(&self.source_info);
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
                                        result: Ok("current material doesn't work but next one just arrived and works"),
                                    }
                                }
                                Err(msg2) => {
                                    // It doesn't work :(
                                    let next_start_frame = req.modulo_end_frame(&self.source_info);
                                    self.request_more_material(
                                        &req,
                                        response.equipment,
                                        next_start_frame,
                                        // Yes, all existing material became obsolete.
                                        Some(current_material),
                                        Some(response.material),
                                    )?;
                                    Outcome {
                                        next_state: PreparingCurrentMaterial,
                                        result: Err(e(
                                            "neither current nor new material fits while preparing next material",
                                            msg1
                                        )),
                                    }
                                }
                            }
                        } else {
                            // And we also don't have new material yet :(
                            // There's nothing we can do at the moment except to wait for the new
                            // material.
                            Outcome {
                                next_state: PreparingNextMaterial { current_material },
                                result: Err(e(
                                    "current material exhausted, waiting for new one",
                                    msg1,
                                )),
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
                match current_material.apply(&mut req, &self.source_info) {
                    Ok(s) => {
                        // The current material fulfilled the request.
                        use MaterialStatus::*;
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
                                let next_start_frame =
                                    next_material.modulo_end_frame(&self.source_info);
                                self.request_more_material(
                                    &req,
                                    equipment,
                                    next_start_frame,
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
                            result: Ok("current material works while full"),
                        }
                    }
                    Err(msg1) => {
                        // The current material didn't fulfill the request.
                        // Let's check whether the next material works.
                        match next_material.apply(&mut req, &self.source_info) {
                            Ok(_) => {
                                // It does! And we assume it will work for the next few blocks, too!
                                // TODO-low We could check if the new material is now obsolete,
                                //  but I think this shouldn't happen often. If it does, the
                                //  next request will miss and request new material.
                                let next_start_frame =
                                    next_material.modulo_end_frame(&self.source_info);
                                self.request_more_material(
                                    &req,
                                    equipment,
                                    next_start_frame,
                                    Some(current_material),
                                    None,
                                )?;
                                Outcome {
                                    next_state: PreparingNextMaterial {
                                        current_material: next_material,
                                    },
                                    result: Ok("next material works while full"),
                                }
                            }
                            Err(msg2) => {
                                // It doesn't work :(
                                let next_start_frame = req.modulo_end_frame(&self.source_info);
                                self.request_more_material(
                                    &req,
                                    equipment,
                                    next_start_frame,
                                    // Yes, all existing material became obsolete.
                                    Some(current_material),
                                    Some(next_material),
                                )?;
                                Outcome {
                                    next_state: PreparingCurrentMaterial,
                                    result: Err(e(
                                        "neither current nor new material fits while full",
                                        msg1,
                                    )),
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
        start_frame: usize,
        obsolete_material_1: Option<StretchedMaterial>,
        obsolete_material_2: Option<StretchedMaterial>,
    ) -> Result<(), TryStretchError> {
        let dest_channel_count = req.dest_buffer.channel_count();
        let dest_frame_count = req.dest_buffer.frame_count() * self.lookahead_factor;
        let start_time =
            PositionInSeconds::new(start_frame as f64 / self.source_info.sample_rate.get());
        let fill_request = FillRequest {
            source: req.source,
            start_time,
            tempo_factor: req.tempo_factor,
            dest_channel_count,
            dest_frame_count,
        };
        // Let another thread to the actual stretching work.
        let async_stretch_request = AsyncStretchRequest {
            stretcher: equipment
                .stretcher
                .fill(fill_request)
                .map_err(|msg| e(msg, ""))?,
            start_frame,
            tempo_factor: req.tempo_factor,
            dest_channel_count,
            dest_frame_count,
            spare_buffer_1: obsolete_material_1.map(|m| m.buffer.into_inner()),
            spare_buffer_2: obsolete_material_2.map(|m| m.buffer.into_inner()),
        };
        let worker_request = StretchWorkerRequest::Stretch {
            request: async_stretch_request,
            response_sender: self.response_sender.clone(),
        };
        self.worker_sender
            .try_send(worker_request)
            .map_err(|_| "couldn't contact worker")
            .map_err(|msg| e(msg, ""))?;
        Ok(())
    }

    fn poll_stretch_response(&mut self) -> Option<AsyncStretchResponse> {
        let response = self.response_receiver.try_iter().last();
        if let Some(r) = &response {
            let info = r.material.stretch_info(&self.source_info);
            println!("Received material: {:?}", info);
        }
        response
    }
}

pub enum StretchWorkerRequest {
    Stretch {
        request: AsyncStretchRequest,
        response_sender: Sender<AsyncStretchResponse>,
    },
}

#[derive(Debug)]
pub struct AsyncStretchRequest {
    stretcher: FilledReaperStretcher,
    start_frame: usize,
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

/// A function that keeps processing stretch worker requests until the channel of the given receiver
/// is dropped.
pub fn keep_stretching(requests: Receiver<StretchWorkerRequest>) {
    while let Ok(req) = requests.recv() {
        use StretchWorkerRequest::*;
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
            start_frame: req.start_frame,
            tempo_factor: req.tempo_factor,
            buffer: dest_buffer,
        },
    }
}

struct Outcome {
    next_state: AsyncStretcherState,
    result: Result<&'static str, TryStretchError>,
}

pub struct TryStretchError {
    pub primary_msg: &'static str,
    pub secondary_msg: &'static str,
}

impl Display for TryStretchError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(self.primary_msg)?;
        if !self.secondary_msg.is_empty() {
            write!(f, " ({})", self.secondary_msg)?;
        }
        Ok(())
    }
}

fn e(primary_msg: &'static str, secondary_msg: &'static str) -> TryStretchError {
    TryStretchError {
        primary_msg,
        secondary_msg,
    }
}
