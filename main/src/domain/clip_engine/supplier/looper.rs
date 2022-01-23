use crate::domain::clip_engine::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::domain::clip_engine::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    ExactFrameCount, MidiSupplier, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
};
use crate::domain::clip_engine::Repetition;
use core::cmp;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer,
};

pub struct Looper<S> {
    loop_behavior: LoopBehavior,
    fades_enabled: bool,
    supplier: S,
}

pub enum LoopBehavior {
    Infinitely,
    UntilEndOfCycle(usize),
}

impl LoopBehavior {
    pub fn from_repetition(repetition: Repetition) -> Self {
        use Repetition::*;
        match repetition {
            Infinitely => Self::Infinitely,
            Once => Self::UntilEndOfCycle(0),
        }
    }
}

impl<S: ExactFrameCount> Looper<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            loop_behavior: LoopBehavior::UntilEndOfCycle(0),
            fades_enabled: false,
            supplier,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn set_loop_behavior(&mut self, loop_behavior: LoopBehavior) {
        self.loop_behavior = loop_behavior;
    }

    pub fn set_fades_enabled(&mut self, fades_enabled: bool) {
        self.fades_enabled = fades_enabled;
    }

    pub fn get_cycle_at_frame(&self, frame: usize) -> usize {
        frame / self.supplier.frame_count()
    }

    fn is_relevant(&self, start_frame: isize) -> bool {
        if start_frame < 0 {
            return false;
        }
        let start_frame = start_frame as usize;
        use LoopBehavior::*;
        match self.loop_behavior {
            Infinitely => true,
            UntilEndOfCycle(n) => {
                if n == 0 {
                    false
                } else {
                    self.get_cycle_at_frame(start_frame) <= n
                }
            }
        }
    }
}

impl<S: AudioSupplier + ExactFrameCount> AudioSupplier for Looper<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        if !self.is_relevant(request.start_frame) {
            return self.supplier.supply_audio(&request, dest_buffer);
        }
        let start_frame = request.start_frame as usize;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyAudioRequest {
            start_frame: modulo_start_frame as isize,
            ..*request
        };
        let modulo_response = self.supplier.supply_audio(&modulo_request, dest_buffer);
        let final_response = if modulo_response.num_frames_written == dest_buffer.frame_count() {
            // Didn't cross the end yet. Nothing else to do.
            build_response(
                request.start_frame,
                modulo_start_frame,
                modulo_response.num_frames_written,
                modulo_response.next_inner_frame,
            )
        } else {
            // Crossed the end. We need to fill the rest with material from the beginning of the source.
            let start_request = SupplyAudioRequest {
                start_frame: 0,
                ..*request
            };
            let start_response = self.supplier.supply_audio(
                &start_request,
                &mut dest_buffer.slice_mut(modulo_response.num_frames_written..),
            );
            build_response(
                request.start_frame,
                modulo_start_frame,
                dest_buffer.frame_count(),
                start_response.next_inner_frame,
            )
        };
        if self.fades_enabled {
            dest_buffer.modify_frames(|frame, sample| {
                let factor = calc_volume_factor_at(start_frame + frame, supplier_frame_count);
                sample * factor
            });
        }
        final_response
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }

    fn sample_rate(&self) -> Hz {
        self.supplier.sample_rate()
    }
}

fn build_response(
    original_start_frame: isize,
    modulo_start_frame: usize,
    num_frames_written: usize,
    inner_response_next_inner_frame: Option<isize>,
) -> SupplyResponse {
    SupplyResponse {
        num_frames_written,
        next_inner_frame: inner_response_next_inner_frame.map(|f| {
            let num_consumed_frames = f - modulo_start_frame as isize;
            original_start_frame + num_consumed_frames
        }),
    }
}

impl<S: MidiSupplier + ExactFrameCount> MidiSupplier for Looper<S> {
    fn supply_midi(
        &self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        if !self.is_relevant(request.start_frame) {
            return self.supplier.supply_midi(&request, event_list);
        }
        let start_frame = request.start_frame as usize;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyMidiRequest {
            start_frame: modulo_start_frame as isize,
            ..*request
        };
        let modulo_response = self.supplier.supply_midi(&modulo_request, event_list);
        if modulo_response.num_frames_written == request.dest_frame_count {
            // Didn't cross the end yet. Nothing else to do.
            build_response(
                request.start_frame,
                modulo_start_frame,
                modulo_response.num_frames_written,
                modulo_response.next_inner_frame,
            )
        } else {
            // Crossed the end. We need to fill the rest with material from the beginning of the source.
            dbg!("MIDI repeat");
            // Repeat. Fill rest of buffer with beginning of source.
            // We need to start from negative position so the frame
            // offset of the *added* MIDI events is correctly written.
            // The negative position should be as long as the duration of
            // samples already written.
            let start_request = SupplyMidiRequest {
                // TODO-high Probably wrong because start_frame expects source frames, not
                //  dest frames
                start_frame: -(modulo_response.num_frames_written as isize),
                ..*request
            };
            let start_response = self.supplier.supply_midi(&start_request, event_list);
            build_response(
                request.start_frame,
                modulo_start_frame,
                request.dest_frame_count,
                start_response.next_inner_frame,
            )
        }
    }
}

fn calc_volume_factor_at(frame: usize, frame_count: usize) -> f64 {
    let modulo_frame = frame % frame_count;
    let distance_to_end = frame_count - modulo_frame;
    if distance_to_end < FADE_LENGTH {
        // Approaching loop end: Fade out
        return distance_to_end as f64 / FADE_LENGTH as f64;
    }
    if frame >= frame_count && modulo_frame < FADE_LENGTH {
        // Continuing at loop start: Fade in
        return modulo_frame as f64 / FADE_LENGTH as f64;
    }
    return 1.0;
}

// 0.01s = 10ms at 48 kHz
const FADE_LENGTH: usize = 480;
