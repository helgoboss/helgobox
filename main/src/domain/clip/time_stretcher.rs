use crate::domain::clip::buffer::{AudioBufMut, CopyToAudioBuffer, OwnedAudioBuffer};
use crate::domain::clip::SourceInfo;
use crossbeam_channel::{Receiver, Sender};
use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{
    BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer, PositionInSeconds,
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::fmt::{Display, Formatter};

/// A request for stretching source material.
#[derive(Debug)]
pub struct StretchRequest<'a, S: CopyToAudioBuffer> {
    /// Source material.
    pub source: S,
    /// Position within source from which to start stretching.
    pub start_frame: usize,
    pub tempo_factor: f64,
    /// The final time stretched samples should end up here.
    pub dest_buffer: AudioBufMut<'a>,
}

pub enum StretchWorkerRequest {
    Stretch,
}

#[derive(Debug)]
pub struct AsyncStretcher {
    worker_sender: Sender<StretchWorkerRequest>,
    source_info: SourceInfo,
    api: &'static IReaperPitchShift,
    // // TODO-high Let producer and consumer implement AudioBuffer instead directly.
    // temp_buffer: OwnedAudioBuffer,
    // producer: Producer<f64>,
    // consumer: Consumer<f64>,
}

/// A function that keeps processing stretch worker requests until the channel of the given receiver
/// is dropped.
pub fn keep_stretching(requests: Receiver<StretchWorkerRequest>) {}

impl AsyncStretcher {
    pub fn new(worker_sender: Sender<StretchWorkerRequest>, source_info: SourceInfo) -> Self {
        let api = Reaper::get()
            .medium_reaper()
            .low()
            .ReaperGetPitchShiftAPI(REAPER_PITCHSHIFT_API_VER);
        let api = unsafe { &*api };
        api.set_srate(source_info.sample_rate().get());
        // TODO-high Should be able to hold at least twice as many frames as the current buffer size.
        // let (producer, consumer) = RingBuffer::new(512 * 2 * 2);
        Self {
            worker_sender,
            source_info,
            api,
            // TODO-high should be able to hold at least as many frames as the current buffer size.
            // temp_buffer: OwnedAudioBuffer::new(2, 512),
            // producer,
            // consumer,
        }
    }

    pub fn try_stretch(
        &mut self,
        mut req: StretchRequest<&BorrowedPcmSource>,
    ) -> Result<TryStretchSuccess, &'static str> {
        let mut total_num_frames_read = 0;
        let mut total_num_frames_written = 0;
        println!("stretch");
        loop {
            // Fill buffer with a minimum amount of source data (so that we never consume more than
            // necessary).
            let dest_nch = req.dest_buffer.channel_count();
            self.api.set_nch(dest_nch as _);
            self.api.set_tempo(req.tempo_factor);
            let buffer_frame_count = 128usize;
            let stretch_buffer = self.api.GetBuffer(buffer_frame_count as _);
            let mut stretch_buffer =
                unsafe { AudioBufMut::from_raw(stretch_buffer, dest_nch, buffer_frame_count) };
            let num_frames_read = req
                .source
                .copy_to_audio_buffer(req.start_frame + total_num_frames_read, stretch_buffer)?;
            total_num_frames_read += num_frames_read;
            self.api.BufferDone(num_frames_read as _);
            // Get samples
            let mut offset_buffer = req.dest_buffer.slice_mut(total_num_frames_written..);
            let num_frames_written = unsafe {
                self.api.GetSamples(
                    offset_buffer.frame_count() as _,
                    offset_buffer.data_as_mut_ptr(),
                )
            };
            total_num_frames_written += num_frames_written as usize;
            println!(
                "num_frames_read: {}, total_num_frames_read: {}, num_frames_written: {}, total_num_frames_written: {}",
                num_frames_read, total_num_frames_read, num_frames_written, total_num_frames_written
            );
            // let mut chunk = self
            //     .producer
            //     .write_chunk(req.dest_buffer.channel_count() * written_frames as _)
            //     .unwrap();
            // chunk.as_mut_slices();
            if total_num_frames_written >= req.dest_buffer.frame_count() {
                // We have enough stretched material.
                break;
            }
        }
        assert_eq!(
            total_num_frames_written,
            req.dest_buffer.frame_count(),
            "wrote more frames than requested"
        );
        let success = TryStretchSuccess {
            consumed_source_frames: total_num_frames_read,
        };
        Ok(success)
    }
}

pub struct TryStretchSuccess {
    pub consumed_source_frames: usize,
}
