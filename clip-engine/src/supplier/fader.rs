use crate::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::supplier::midi_util::silence_midi;
use crate::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    ExactFrameCount, MidiSupplier, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
    WithFrameRate,
};
use core::cmp;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer,
};

#[derive(Debug)]
pub struct Fader<S> {
    start_frame: Option<isize>,
    supplier: S,
}
//
// struct Fade {
//     direction: FadeDirection,
//     start_frame: isize
// }

enum FadeDirection {
    FadeIn,
    FadeOut,
}

impl<S> Fader<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            start_frame: None,
            supplier,
        }
    }

    pub fn is_fading_out(&self) -> bool {
        self.start_frame.is_some()
    }

    pub fn reset(&mut self) {
        self.start_frame = None;
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn start_fade_out(&mut self, start_frame: isize) {
        self.start_frame = Some(start_frame);
    }
}

impl<S: AudioSupplier> AudioSupplier for Fader<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let fade_start_frame = match self.start_frame {
            // No fade request.
            None => return self.supplier.supply_audio(request, dest_buffer),
            Some(f) => f,
        };
        if request.start_frame < fade_start_frame {
            // Fade not started yet. Shouldn't happen if used in normal ways (instant fade).
            return self.supplier.supply_audio(request, dest_buffer);
        }
        let fade_end_frame = fade_start_frame + FADE_LENGTH as isize;
        if request.start_frame >= fade_end_frame {
            // Nothing to fade anymore. Shouldn't happen if used in normal ways (stops requests
            // as soon as fade phase ended).
            return SupplyResponse {
                num_frames_written: 0,
                num_frames_consumed: 0,
                next_inner_frame: None,
            };
        }
        // In fade phase.
        let inner_response = self.supplier.supply_audio(request, dest_buffer);
        let remaining_frames_until_fade_finished = (fade_end_frame - request.start_frame) as usize;
        dest_buffer.modify_frames(|frame, sample| {
            let factor = (remaining_frames_until_fade_finished + frame) as f64 / FADE_LENGTH as f64;
            sample * factor
        });
        let request_end_frame = request.start_frame + dest_buffer.frame_count() as isize;
        SupplyResponse {
            num_frames_written: inner_response.num_frames_written,
            num_frames_consumed: inner_response.num_frames_consumed,
            next_inner_frame: if request_end_frame < fade_end_frame {
                inner_response.next_inner_frame
            } else {
                // Fade finished.
                self.start_frame = None;
                None
            },
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for Fader<S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier> MidiSupplier for Fader<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let fade_start_frame = match self.start_frame {
            // No fade request.
            None => return self.supplier.supply_midi(request, event_list),
            Some(f) => f,
        };
        if request.start_frame < fade_start_frame {
            // Fade not started yet. Shouldn't happen if used in normal ways (instant fade).
            return self.supplier.supply_midi(request, event_list);
        }
        // With MIDI it's simple. No fade necessary, just a plain "Shut up!".
        silence_midi(event_list);
        SupplyResponse {
            num_frames_written: 0,
            num_frames_consumed: 0,
            next_inner_frame: None,
        }
    }
}

// 0.01s = 10ms at 48 kHz
const FADE_LENGTH: usize = 480;
