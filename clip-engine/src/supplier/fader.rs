use crate::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::midi_util;
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
    fade: Option<Fade>,
    supplier: S,
}

#[derive(Clone, Copy, Debug)]
struct Fade {
    direction: FadeDirection,
    start_frame: isize,
    end_frame: isize,
}

impl Fade {
    fn new(start_frame: isize, direction: FadeDirection) -> Self {
        Fade {
            direction,
            start_frame,
            end_frame: start_frame + FADE_LENGTH as isize,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum FadeDirection {
    FadeIn,
    FadeOut,
}

impl<S> Fader<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            fade: None,
            supplier,
        }
    }

    pub fn is_fading_in(&self) -> bool {
        self.fade
            .map(|f| f.direction == FadeDirection::FadeIn)
            .unwrap_or(false)
    }

    pub fn is_fading_out(&self) -> bool {
        self.fade
            .map(|f| f.direction == FadeDirection::FadeOut)
            .unwrap_or(false)
    }

    pub fn reset(&mut self) {
        self.fade = None;
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    /// When interacting with an already running fade-out, the assumption is that the given start
    /// frame is the current frame.
    pub fn start_fade_in(&mut self, start_frame: isize) {
        self.start_fade(FadeDirection::FadeIn, start_frame);
    }

    /// When interacting with an already running fade-in, the assumption is that the given start
    /// frame is the current frame.
    pub fn start_fade_out(&mut self, start_frame: isize) {
        self.start_fade(FadeDirection::FadeOut, start_frame);
    }

    fn start_fade(&mut self, direction: FadeDirection, start_frame: isize) {
        if let Some(start_frame) = self.calc_actual_start_frame(direction, start_frame) {
            self.fade = Some(Fade::new(start_frame, direction));
        }
    }

    fn calc_actual_start_frame(
        &self,
        direction: FadeDirection,
        requested_start_frame: isize,
    ) -> Option<isize> {
        match self.fade {
            None => Some(requested_start_frame),
            Some(f) => {
                if f.direction == direction {
                    // Already fading.
                    return None;
                }
                let current_pos_in_fade = requested_start_frame - f.start_frame;
                // If current_pos_in_fade is zero, I should skip the fade (move it completely to left).
                // If it's FADE_LENGTH, I should apply the complete fade
                let adjustment = current_pos_in_fade - FADE_LENGTH as isize;
                Some(requested_start_frame + adjustment)
            }
        }
    }
}

impl<S: AudioSupplier> AudioSupplier for Fader<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        use FadeDirection::*;
        let fade = match self.fade {
            // No fade request.
            None => return self.supplier.supply_audio(request, dest_buffer),
            Some(f) => f,
        };
        if request.start_frame < fade.start_frame && fade.direction == FadeOut {
            // Fade out not started yet. Shouldn't happen if used in normal ways (instant fade).
            return self.supplier.supply_audio(request, dest_buffer);
        }
        if request.start_frame >= fade.end_frame {
            // Nothing to fade anymore. Shouldn't happen if used in normal ways (stops requests
            // as soon as fade phase ended).
            match fade.direction {
                FadeIn => {
                    self.fade = None;
                    return self.supplier.supply_audio(request, dest_buffer);
                }
                FadeOut => {
                    return SupplyResponse::empty();
                }
            }
        }
        // In fade phase.
        let inner_response = self.supplier.supply_audio(request, dest_buffer);
        let counter = match fade.direction {
            FadeIn => (request.start_frame - fade.start_frame) as usize,
            FadeOut => (fade.end_frame - request.start_frame) as usize,
        };
        dest_buffer.modify_frames(|frame, sample| {
            let factor = (counter + frame) as f64 / FADE_LENGTH as f64;
            sample * factor
        });
        let fade_finished = inner_response
            .next_inner_frame
            .map(|f| f >= fade.end_frame)
            .unwrap_or(true);
        if fade_finished {
            self.fade = None;
            match fade.direction {
                FadeIn => inner_response,
                FadeOut => inner_response.with_end_reached(),
            }
        } else {
            inner_response
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for Fader<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier> MidiSupplier for Fader<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let fade = match self.fade {
            Some(
                f
                @
                Fade {
                    direction: FadeDirection::FadeOut,
                    ..
                },
            ) => f,
            // No fade out request.
            _ => return self.supplier.supply_midi(request, event_list),
        };
        if request.start_frame < fade.start_frame {
            // Fade not started yet. Shouldn't happen if used in normal ways (instant fade).
            return self.supplier.supply_midi(request, event_list);
        }
        // With MIDI it's simple. No fade necessary, just a plain "Shut up!".
        midi_util::silence_midi(event_list);
        SupplyResponse::empty()
    }
}

// 0.01s = 10ms at 48 kHz
const FADE_LENGTH: usize = 480;
