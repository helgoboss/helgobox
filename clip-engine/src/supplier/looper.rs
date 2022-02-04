use crate::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    print_distance_from_beat_start_at, AudioSupplier, ExactFrameCount, MidiSupplier,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use crate::{clip_timeline, Repetition, SupplyRequestInfo};
use core::cmp;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer,
    PositionInSeconds,
};

#[derive(Debug)]
pub struct Looper<S> {
    loop_behavior: LoopBehavior,
    fades_enabled: bool,
    supplier: S,
}

#[derive(Debug)]
pub enum LoopBehavior {
    Infinitely,
    UntilEndOfCycle(usize),
}

impl Default for LoopBehavior {
    fn default() -> Self {
        Self::UntilEndOfCycle(0)
    }
}

impl LoopBehavior {
    pub fn from_repetition(repetition: Repetition) -> Self {
        use Repetition::*;
        match repetition {
            Infinitely => Self::Infinitely,
            Once => Self::UntilEndOfCycle(0),
        }
    }

    pub fn from_bool(repeated: bool) -> Self {
        if repeated {
            Self::Infinitely
        } else {
            Self::UntilEndOfCycle(0)
        }
    }

    fn last_cycle(&self) -> Option<usize> {
        use LoopBehavior::*;
        match self {
            Infinitely => None,
            UntilEndOfCycle(n) => Some(*n),
        }
    }
}

impl<S: ExactFrameCount> Looper<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            loop_behavior: Default::default(),
            fades_enabled: false,
            supplier,
        }
    }

    pub fn reset(&mut self) {
        if let LoopBehavior::UntilEndOfCycle(n) = self.loop_behavior {
            if n > 0 {
                self.loop_behavior = LoopBehavior::Infinitely;
            }
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

    pub fn keep_playing_until_end_of_current_cycle(&mut self, pos: isize) {
        // TODO-high Scheduling for stop after 2nd cycle plays a bit
        //  too far. Check MIDI clip, plays the downbeat!
        let last_cycle = if pos < 0 {
            0
        } else {
            self.get_cycle_at_frame(pos as usize)
        };
        self.loop_behavior = LoopBehavior::UntilEndOfCycle(last_cycle);
    }

    pub fn set_fades_enabled(&mut self, fades_enabled: bool) {
        self.fades_enabled = fades_enabled;
    }

    pub fn get_cycle_at_frame(&self, frame: usize) -> usize {
        frame / self.supplier.frame_count()
    }

    fn check_relevance(&self, start_frame: isize) -> Option<RelevantData> {
        if start_frame < 0 {
            return None;
        }
        let start_frame = start_frame as usize;
        let current_cycle = self.get_cycle_at_frame(start_frame);
        let cycle_in_scope = self
            .loop_behavior
            .last_cycle()
            .map(|last_cycle| current_cycle <= last_cycle)
            .unwrap_or(true);
        if !cycle_in_scope {
            return None;
        }
        let data = RelevantData {
            start_frame,
            current_cycle,
        };
        Some(data)
    }

    fn is_last_cycle(&self, cycle: usize) -> bool {
        self.loop_behavior
            .last_cycle()
            .map(|last_cycle| cycle == last_cycle)
            .unwrap_or(false)
    }
}

struct RelevantData {
    start_frame: usize,
    current_cycle: usize,
}

impl<S: AudioSupplier + ExactFrameCount> AudioSupplier for Looper<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = match self.check_relevance(request.start_frame) {
            None => {
                return self.supplier.supply_audio(&request, dest_buffer);
            }
            Some(d) => d,
        };
        let start_frame = data.start_frame;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyAudioRequest {
            start_frame: modulo_start_frame as isize,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "looper-audio-modulo-request",
                note: "",
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let modulo_response = self.supplier.supply_audio(&modulo_request, dest_buffer);
        let final_response = if modulo_response.num_frames_written == dest_buffer.frame_count() {
            // Didn't cross the end yet. But maybe reached the end.
            if modulo_response.next_inner_frame.is_none() && self.is_last_cycle(data.current_cycle)
            {
                // Reached the end of last cycle.
                modulo_response
            } else {
                SupplyResponse {
                    num_frames_written: modulo_response.num_frames_written,
                    num_frames_consumed: modulo_response.num_frames_consumed,
                    next_inner_frame: unmodulo_next_inner_frame(
                        modulo_response.next_inner_frame,
                        start_frame,
                        supplier_frame_count,
                    ),
                }
            }
        } else {
            // Crossed the end.
            if self.is_last_cycle(data.current_cycle) {
                modulo_response
            } else {
                // We need to fill the rest with material from the beginning of the source.
                let start_request = SupplyAudioRequest {
                    start_frame: 0,
                    dest_sample_rate: request.dest_sample_rate,
                    info: SupplyRequestInfo {
                        audio_block_frame_offset: request.info.audio_block_frame_offset
                            + modulo_response.num_frames_written,
                        requester: "looper-audio-start-request",
                        note: "",
                    },
                    parent_request: Some(request),
                    general_info: request.general_info,
                };
                let start_response = self.supplier.supply_audio(
                    &start_request,
                    &mut dest_buffer.slice_mut(modulo_response.num_frames_written..),
                );
                SupplyResponse {
                    num_frames_written: dest_buffer.frame_count(),
                    num_frames_consumed: modulo_response.num_frames_consumed
                        + start_response.num_frames_consumed,
                    next_inner_frame: unmodulo_next_inner_frame(
                        start_response.next_inner_frame,
                        start_frame,
                        supplier_frame_count,
                    ),
                }
            }
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
}

impl<S: WithFrameRate> WithFrameRate for Looper<S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier + ExactFrameCount> MidiSupplier for Looper<S> {
    fn supply_midi(
        &self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = match self.check_relevance(request.start_frame) {
            None => {
                return self.supplier.supply_midi(&request, event_list);
            }
            Some(d) => d,
        };
        let start_frame = data.start_frame;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyMidiRequest {
            start_frame: modulo_start_frame as isize,
            dest_frame_count: request.dest_frame_count,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "looper-midi-modulo-request",
                note: "",
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let modulo_response = self.supplier.supply_midi(&modulo_request, event_list);
        if modulo_response.num_frames_written == request.dest_frame_count {
            // Didn't cross the end yet. But maybe reached the end.
            if modulo_response.next_inner_frame.is_none() && self.is_last_cycle(data.current_cycle)
            {
                // Reached the end of last cycle.
                modulo_response
            } else {
                SupplyResponse {
                    num_frames_written: modulo_response.num_frames_written,
                    num_frames_consumed: modulo_response.num_frames_consumed,
                    next_inner_frame: unmodulo_next_inner_frame(
                        modulo_response.next_inner_frame,
                        start_frame,
                        supplier_frame_count,
                    ),
                }
            }
        } else {
            // Crossed the end.
            if self.is_last_cycle(data.current_cycle) {
                modulo_response
            } else {
                // We need to fill the rest with material from the beginning of the source.
                // Repeat. Fill rest of buffer with beginning of source.
                // We need to start from negative position so the frame
                // offset of the *added* MIDI events is correctly written.
                // The negative position should be as long as the duration of
                // samples already written.
                let start_request = SupplyMidiRequest {
                    start_frame: -(modulo_response.num_frames_consumed as isize),
                    dest_sample_rate: request.dest_sample_rate,
                    dest_frame_count: request.dest_frame_count,
                    info: SupplyRequestInfo {
                        audio_block_frame_offset: request.info.audio_block_frame_offset
                            + modulo_response.num_frames_written,
                        requester: "looper-midi-start-request",
                        note: "",
                    },
                    parent_request: Some(request),
                    general_info: request.general_info,
                };
                let start_response = self.supplier.supply_midi(&start_request, event_list);
                SupplyResponse {
                    num_frames_written: request.dest_frame_count,
                    num_frames_consumed: modulo_response.num_frames_consumed
                        + start_response.num_frames_consumed,
                    next_inner_frame: unmodulo_next_inner_frame(
                        start_response.next_inner_frame,
                        start_frame,
                        supplier_frame_count,
                    ),
                }
            }
        }
    }
}

fn unmodulo_next_inner_frame(
    next_inner_frame: Option<isize>,
    previous_start_frame: usize,
    frame_count: usize,
) -> Option<isize> {
    let next_inner_frame = next_inner_frame.unwrap_or(0);
    assert!(next_inner_frame >= 0);
    let next_inner_frame = next_inner_frame as usize;
    assert!(next_inner_frame < frame_count);
    let previous_cycle = previous_start_frame / frame_count;
    let previous_modulo_start_frame = previous_start_frame % frame_count;
    let next_cycle = if previous_modulo_start_frame <= next_inner_frame {
        // We are still in the same cycle.
        previous_cycle
    } else {
        previous_cycle + 1
    };
    Some((next_cycle * frame_count + next_inner_frame) as isize)
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
