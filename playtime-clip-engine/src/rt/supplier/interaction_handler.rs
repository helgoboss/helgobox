use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::fade_util::{
    apply_fade_in_starting_at_zero, apply_fade_out_starting_at_zero, INTERACTION_FADE_LENGTH,
};
use crate::rt::supplier::midi_util::SilenceMidiBlockMode;
use crate::rt::supplier::{
    midi_util, AudioSupplier, MidiSupplier, PreBufferFillRequest, PreBufferSourceSkill,
    SupplyAudioRequest, SupplyMidiRequest, SupplyRequestInfo, SupplyResponse, SupplyResponseStatus,
    WithFrameRate,
};
use playtime_api::MidiResetMessageRange;
use reaper_medium::{BorrowedMidiEventList, Hz};
use std::cmp;

#[derive(Debug)]
pub struct InteractionHandler<S> {
    supplier: S,
    interaction: Option<Interaction>,
    midi_reset_msg_range: MidiResetMessageRange,
}

#[derive(Clone, Copy, Debug)]
struct Interaction {
    kind: InteractionKind,
    /// Reference frame.
    ///
    /// This is the frame on which the material should start (start interaction)
    /// or stop (stop interaction).
    ///
    /// For audio material, fades are inserted. For a start interaction, this frame marks the fade
    /// beginning. For a stop interaction, it marks the fade end.
    frame: isize,
}

impl Interaction {
    pub fn new(kind: InteractionKind, frame: isize) -> Self {
        Interaction { kind, frame }
    }

    pub fn immediate(kind: InteractionKind, current_frame: isize, is_midi: bool) -> Self {
        if is_midi {
            Self::new(kind, current_frame)
        } else {
            use InteractionKind::*;
            match kind {
                Start => Self::new(kind, current_frame),
                Stop => Self::new(kind, current_frame + INTERACTION_FADE_LENGTH as isize),
            }
        }
    }

    pub fn fade_begin_frame(&self) -> isize {
        use InteractionKind::*;
        match self.kind {
            Start => self.frame,
            Stop => self.frame - INTERACTION_FADE_LENGTH as isize,
        }
    }

    pub fn fade_end_frame(&self) -> isize {
        use InteractionKind::*;
        match self.kind {
            Start => self.frame + INTERACTION_FADE_LENGTH as isize,
            Stop => self.frame,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum InteractionKind {
    Start,
    Stop,
}

impl<S> InteractionHandler<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            interaction: None,
            supplier,
            midi_reset_msg_range: Default::default(),
        }
    }

    pub fn set_midi_reset_msg_range(&mut self, range: MidiResetMessageRange) {
        self.midi_reset_msg_range = range;
    }

    pub fn has_stop_interaction(&self) -> bool {
        self.interaction
            .map(|f| f.kind == InteractionKind::Stop)
            .unwrap_or(false)
    }

    pub fn reset(&mut self) {
        self.interaction = None;
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    /// Invokes a start interaction.
    ///
    /// Audio:
    /// - Installs a fade-in starting at the given frame.
    /// - Handles an already happening fade-out correctly.
    ///
    /// MIDI:
    /// - Installs some stop interaction reset messages.
    pub fn start_immediately(&mut self, current_frame: isize, is_midi: bool) {
        self.install_immediate_interaction(InteractionKind::Start, current_frame, is_midi);
    }

    /// Invokes a stop interaction.
    ///
    /// Audio:
    /// - Installs a fade-out starting at the given frame.
    /// - Handles an already happening fade-in correctly.
    ///
    /// MIDI:
    /// - Installs some stop interaction reset messages.
    pub fn stop_immediately(&mut self, current_frame: isize, is_midi: bool) {
        self.install_immediate_interaction(InteractionKind::Stop, current_frame, is_midi);
    }

    /// Schedules a stop interaction at the given position.
    ///
    /// Audio:
    /// - Installs a fade-out ending at the given frame (and starting some frames before that).
    /// - Doesn't do anything to handle an already happening fade-in correctly!
    ///
    /// MIDI:
    /// - Installs some stop interaction reset messages at the given frame.
    pub fn schedule_stop_at(&mut self, end_frame: isize) {
        self.interaction = Some(Interaction::new(InteractionKind::Stop, end_frame))
    }

    fn install_immediate_interaction(
        &mut self,
        kind: InteractionKind,
        current_frame: isize,
        is_midi: bool,
    ) {
        let new_interaction = Interaction::immediate(kind, current_frame, is_midi);
        let new_interaction = if is_midi {
            Some(new_interaction)
        } else {
            self.fix_new_interaction_respecting_overlapping_fades(new_interaction)
        };
        if let Some(i) = new_interaction {
            self.interaction = Some(i)
        }
    }

    /// Shifts the frame of the given interaction in case there's a fade happening already in order
    /// to ensure continuity of the volume envelope (e.g. a fade-in when it's already fading out).
    ///
    /// Returns `None` if no interaction change is necessary, in particular if there's already
    /// an ongoing fade in the right direction.
    ///
    /// Attention: This logic assumes that the given frame is the current timeline frame! It
    /// can't be used with scheduled interactions.
    fn fix_new_interaction_respecting_overlapping_fades(
        &self,
        new_interaction: Interaction,
    ) -> Option<Interaction> {
        let ongoing_interaction = match self.interaction {
            // No fade at the moment, no fix necessary.
            None => return Some(new_interaction),
            Some(i) => i,
        };
        if ongoing_interaction.kind == new_interaction.kind {
            // Already fading into same direction. No need to substitute interaction.
            return None;
        }
        let begin_frame_of_new_fade = new_interaction.fade_begin_frame();
        let begin_frame_of_ongoing_fade = ongoing_interaction.fade_begin_frame();
        let current_pos_in_fade = begin_frame_of_new_fade - begin_frame_of_ongoing_fade;
        // If current_pos_in_fade is zero, we should skip the fade (move it completely to left).
        // If it's FADE_LENGTH, we should apply the complete fade.
        let adjustment = current_pos_in_fade - INTERACTION_FADE_LENGTH as isize;
        let fixed_interaction =
            Interaction::new(new_interaction.kind, new_interaction.frame + adjustment);
        Some(fixed_interaction)
    }
}

impl<S: AudioSupplier> AudioSupplier for InteractionHandler<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let interaction = match self.interaction {
            None => {
                // No interaction installed.
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Some(i) => i,
        };
        use InteractionKind::*;
        let distance_from_fade_begin = request.start_frame - interaction.fade_begin_frame();
        match interaction.kind {
            Start => {
                if distance_from_fade_begin < 0 {
                    unreachable!("there shouldn't be any scheduled start interactions");
                }
                let inner_response = self.supplier.supply_audio(request, dest_buffer);
                // The following function returns early if fade not yet started.
                apply_fade_in_starting_at_zero(
                    dest_buffer,
                    distance_from_fade_begin,
                    INTERACTION_FADE_LENGTH,
                );
                let end_frame = request.start_frame + inner_response.num_frames_consumed as isize;
                if end_frame >= interaction.fade_end_frame() || inner_response.status.reached_end()
                {
                    // Fade-in over or end-of-material reached. We can uninstall the interaction.
                    self.interaction = None;
                }
                inner_response
            }
            Stop => {
                let distance_to_fade_end = interaction.fade_end_frame() - request.start_frame;
                if distance_to_fade_end <= 0 {
                    // Exceeded end. Shouldn't usually happen because playback is continuous, but
                    // let's handle this gracefully.
                    self.interaction = None;
                    return SupplyResponse::exceeded_end();
                }
                let num_frames_to_write =
                    cmp::min(dest_buffer.frame_count(), distance_to_fade_end as usize);
                let mut sliced_dest_buffer = dest_buffer.slice_mut(0..num_frames_to_write);
                let inner_response = self.supplier.supply_audio(request, &mut sliced_dest_buffer);
                match inner_response.status {
                    SupplyResponseStatus::PleaseContinue => {
                        // The following function returns early if fade not yet started.
                        apply_fade_out_starting_at_zero(
                            dest_buffer,
                            distance_from_fade_begin,
                            INTERACTION_FADE_LENGTH,
                        );
                        let end_frame =
                            request.start_frame + inner_response.num_frames_consumed as isize;
                        if end_frame < interaction.fade_end_frame() {
                            // Fade-out end not reached yet.
                            inner_response
                        } else {
                            // Fade-out over. We can uninstall the interaction.
                            self.interaction = None;
                            SupplyResponse::reached_end(
                                inner_response.num_frames_consumed,
                                num_frames_to_write,
                            )
                        }
                    }
                    SupplyResponseStatus::ReachedEnd { .. } => {
                        // If no more material, it's not our responsibility to apply a fade.
                        // Also, there's no need to continue the fade.
                        self.interaction = None;
                        inner_response
                    }
                }
            }
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for InteractionHandler<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier> MidiSupplier for InteractionHandler<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        // With MIDI it's simple. No fade necessary, just a plain "Shut up!".
        let interaction = match self.interaction {
            None => {
                // No interaction installed.
                return self.supplier.supply_midi(request, event_list);
            }
            Some(i) => i,
        };
        use InteractionKind::*;
        match interaction.kind {
            Start => {
                // We know that start interactions are always immediate and that they are cleared
                // immediately as well (MIDI only).
                assert_eq!(request.start_frame, interaction.frame);
                let inner_response = self.supplier.supply_midi(request, event_list);
                debug!("Silence MIDI at start interaction");
                midi_util::silence_midi(
                    event_list,
                    self.midi_reset_msg_range.left,
                    SilenceMidiBlockMode::Prepend,
                );
                self.interaction = None;
                inner_response
            }
            Stop => {
                let distance_to_stop = interaction.frame - request.start_frame;
                if distance_to_stop <= 0 {
                    // Exceeded end. Shouldn't usually happen because playback is continuous, but
                    // let's handle this gracefully.
                    self.interaction = None;
                    return SupplyResponse::exceeded_end();
                }
                let num_frames_to_write =
                    cmp::min(request.dest_frame_count, distance_to_stop as usize);
                let inner_request = SupplyMidiRequest {
                    dest_frame_count: num_frames_to_write,
                    info: SupplyRequestInfo {
                        audio_block_frame_offset: request.info.audio_block_frame_offset,
                        requester: "interaction-handler-midi-stop",
                        note: "",
                        is_realtime: true,
                    },
                    parent_request: Some(request),
                    ..request.clone()
                };
                let inner_response = self.supplier.supply_midi(&inner_request, event_list);
                match inner_response.status {
                    SupplyResponseStatus::PleaseContinue => {
                        let end_frame =
                            request.start_frame + inner_response.num_frames_consumed as isize;
                        if end_frame < interaction.frame {
                            // Not yet time to reset.
                            inner_response
                        } else {
                            // Time to reset. Also, we can uninstall the interaction.
                            debug!("Silence MIDI at stop interaction");
                            midi_util::silence_midi(
                                event_list,
                                self.midi_reset_msg_range.right,
                                SilenceMidiBlockMode::Append,
                            );
                            self.interaction = None;
                            SupplyResponse::reached_end(
                                inner_response.num_frames_consumed,
                                num_frames_to_write,
                            )
                        }
                    }
                    SupplyResponseStatus::ReachedEnd { .. } => {
                        // If no more material, it's not our responsibility to apply a fade.
                        // Also, there's no need to continue the fade.
                        self.interaction = None;
                        inner_response
                    }
                }
            }
        }
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for InteractionHandler<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.supplier.pre_buffer(request);
    }
}
