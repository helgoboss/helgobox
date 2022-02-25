use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::midi_util::SilenceMidiBlockMode;
use crate::rt::supplier::{
    midi_util, AudioSupplier, MidiSupplier, PreBufferFillRequest, PreBufferSourceSkill,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, SupplyResponseStatus, WithFrameRate,
};
use playtime_api::{MidiResetMessageRange, MidiResetMessages};
use reaper_medium::{BorrowedMidiEventList, Hz};

#[derive(Debug)]
pub struct AdHocFader<S> {
    supplier: S,
    fade: Option<Fade>,
    midi_reset_msg_range: MidiResetMessageRange,
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

impl<S> AdHocFader<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            fade: None,
            supplier,
            midi_reset_msg_range: Default::default(),
        }
    }

    pub fn set_midi_reset_msg_range(&mut self, range: MidiResetMessageRange) {
        self.midi_reset_msg_range = range;
    }

    pub fn has_fade_in(&self) -> bool {
        self.fade
            .map(|f| f.direction == FadeDirection::FadeIn)
            .unwrap_or(false)
    }

    pub fn has_fade_out(&self) -> bool {
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

    /// Doesn't do anything special if a fade-in is running already.
    pub fn schedule_fade_out_ending_at(&mut self, end_frame: isize) {
        let start_frame = end_frame - FADE_LENGTH as isize;
        self.fade = Some(Fade::new(start_frame, FadeDirection::FadeOut))
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

    fn get_instruction(&mut self, start_frame: isize) -> Instruction {
        use FadeDirection::*;
        let fade = match self.fade {
            // No fade request.
            None => return Instruction::Bypass,
            Some(f) => f,
        };
        if start_frame < fade.start_frame && fade.direction == FadeOut {
            // Fade out not started yet. Shouldn't happen if used in normal ways (instant fade).
            return Instruction::Bypass;
        }
        if start_frame >= fade.end_frame {
            // Nothing to fade anymore. Shouldn't happen if used in normal ways (stops requesting
            // as soon as fade phase ended).
            match fade.direction {
                FadeIn => {
                    self.fade = None;
                    return Instruction::Bypass;
                }
                FadeOut => {
                    return Instruction::Return(SupplyResponse::exceeded_end());
                }
            }
        }
        Instruction::ApplyFade(fade)
    }
}

impl<S: AudioSupplier> AudioSupplier for AdHocFader<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        use Instruction::*;
        let fade = match self.get_instruction(request.start_frame) {
            Bypass => {
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Return(r) => {
                return r;
            }
            ApplyFade(f) => f,
        };
        // In fade phase.
        let inner_response = self.supplier.supply_audio(request, dest_buffer);
        use FadeDirection::*;
        let counter = match fade.direction {
            FadeIn => (request.start_frame - fade.start_frame) as usize,
            FadeOut => (fade.end_frame - request.start_frame) as usize,
        };
        dest_buffer.modify_frames(|frame, sample| {
            let factor = (counter + frame) as f64 / FADE_LENGTH as f64;
            sample * factor
        });
        let fade_finished = match inner_response.status {
            SupplyResponseStatus::PleaseContinue => {
                let end_frame = request.start_frame + inner_response.num_frames_consumed as isize;
                end_frame >= fade.end_frame
            }
            SupplyResponseStatus::ReachedEnd { .. } => true,
        };
        if fade_finished {
            self.fade = None;
            match fade.direction {
                FadeIn => inner_response,
                FadeOut => SupplyResponse::reached_end(
                    inner_response.num_frames_consumed,
                    dest_buffer.frame_count(),
                ),
            }
        } else {
            inner_response
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for AdHocFader<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier> MidiSupplier for AdHocFader<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        use Instruction::*;
        let fade = match self.get_instruction(request.start_frame) {
            Bypass => {
                return self.supplier.supply_midi(request, event_list);
            }
            Return(r) => {
                return r;
            }
            ApplyFade(f) => f,
        };
        // With MIDI it's simple. No fade necessary, just a plain "Shut up!".
        use FadeDirection::*;
        let (reset_messages, block_mode) = match fade.direction {
            FadeIn => {
                debug!("Silence MIDI at start interaction");
                (
                    self.midi_reset_msg_range.left,
                    SilenceMidiBlockMode::Prepend,
                )
            }
            FadeOut => {
                debug!("Silence MIDI at stop interaction");
                (
                    self.midi_reset_msg_range.right,
                    SilenceMidiBlockMode::Append,
                )
            }
        };
        midi_util::silence_midi(event_list, reset_messages, block_mode);
        SupplyResponse::exceeded_end()
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for AdHocFader<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.supplier.pre_buffer(request);
    }
}

// 0.01s = 10ms at 48 kHz
const FADE_LENGTH: usize = 480;

enum Instruction {
    Bypass,
    Return(SupplyResponse),
    ApplyFade(Fade),
}
