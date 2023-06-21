use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::midi_util::SilenceMidiBlockMode;
use crate::rt::supplier::{
    midi_util, AudioSupplier, AutoDelegatingMidiSilencer, AutoDelegatingWithMaterialInfo, MidiSilencer, MidiSupplier, PositionTranslationSkill, SupplyAudioRequest,
    SupplyMidiRequest, SupplyRequest, SupplyRequestInfo, SupplyResponse, SupplyResponseStatus,
    WithMaterialInfo, WithSupplier,
};
use crate::ClipEngineResult;
use playtime_api::persistence::MidiResetMessageRange;
use reaper_medium::{BorrowedMidiEventList};

#[derive(Debug)]
pub struct Looper<S> {
    loop_behavior: LoopBehavior,
    enabled: bool,
    supplier: S,
    midi_reset_msg_range: MidiResetMessageRange,
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Repetition {
    Infinitely,
    Once,
}

impl Repetition {
    pub fn from_bool(repeated: bool) -> Self {
        if repeated {
            Repetition::Infinitely
        } else {
            Repetition::Once
        }
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

    /// Returns the index of the last cycle to be played.
    fn last_cycle_to_be_played(&self) -> Option<usize> {
        use LoopBehavior::*;
        match self {
            Infinitely => None,
            UntilEndOfCycle(n) => Some(*n),
        }
    }
}

impl<S> WithSupplier for Looper<S> {
    type Supplier = S;

    fn supplier(&self) -> &Self::Supplier {
        &self.supplier
    }

    fn supplier_mut(&mut self) -> &mut Self::Supplier {
        &mut self.supplier
    }
}

impl<S: WithMaterialInfo> Looper<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            loop_behavior: Default::default(),
            enabled: false,
            supplier,
            midi_reset_msg_range: Default::default(),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_midi_reset_msg_range(&mut self, range: MidiResetMessageRange) {
        self.midi_reset_msg_range = range;
    }

    pub fn set_loop_behavior(&mut self, loop_behavior: LoopBehavior) {
        self.loop_behavior = loop_behavior;
    }

    pub fn keep_playing_until_end_of_current_cycle(&mut self, pos: isize) -> ClipEngineResult<()> {
        let last_cycle = get_cycle_at_frame(pos, self.supplier.material_info()?.frame_count());
        self.loop_behavior = LoopBehavior::UntilEndOfCycle(last_cycle);
        Ok(())
    }

    fn check_relevance(&self, start_frame: isize) -> Option<RelevantData> {
        if !self.enabled {
            return None;
        }
        let frame_count = self.supplier.material_info().unwrap().frame_count();
        let current_cycle = get_cycle_at_frame(start_frame, frame_count);
        let cycle_in_scope = self
            .loop_behavior
            .last_cycle_to_be_played()
            .map(|last_cycle| current_cycle <= last_cycle)
            .unwrap_or(true);
        if !cycle_in_scope {
            return None;
        }
        let data = RelevantData {
            start_frame,
            current_cycle,
            frame_count,
        };
        Some(data)
    }

    fn is_last_cycle(&self, cycle: usize) -> bool {
        self.loop_behavior
            .last_cycle_to_be_played()
            .map(|last_cycle| cycle == last_cycle)
            .unwrap_or(false)
    }
}

struct RelevantData {
    start_frame: isize,
    current_cycle: usize,
    frame_count: usize,
}

impl RelevantData {
    /// Start from beginning if we encounter a start frame after the end (modulo).
    fn modulo_start_frame(&self) -> isize {
        if self.start_frame < 0 {
            self.start_frame
        } else {
            self.start_frame % self.frame_count as isize
        }
    }
}

impl<S: AudioSupplier + WithMaterialInfo> AudioSupplier for Looper<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = match self.check_relevance(request.start_frame) {
            None => {
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Some(d) => d,
        };
        let modulo_start_frame = data.modulo_start_frame();
        let modulo_request = SupplyAudioRequest {
            start_frame: modulo_start_frame,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "looper-audio-modulo-request",
                note: "",
                is_realtime: request.info().is_realtime,
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let modulo_response = self.supplier.supply_audio(&modulo_request, dest_buffer);
        match modulo_response.status {
            SupplyResponseStatus::PleaseContinue => modulo_response,
            SupplyResponseStatus::ReachedEnd { num_frames_written } => {
                if self.is_last_cycle(data.current_cycle) {
                    // Time to stop.
                    modulo_response
                } else if num_frames_written == dest_buffer.frame_count() {
                    // Perfect landing, source completely consumed. Start next cycle.
                    SupplyResponse::please_continue(modulo_response.num_frames_consumed)
                } else {
                    // Exceeded end of source.
                    // We need to fill the rest with material from the beginning of the source.
                    let start_request = SupplyAudioRequest {
                        start_frame: 0,
                        dest_sample_rate: request.dest_sample_rate,
                        info: SupplyRequestInfo {
                            audio_block_frame_offset: request.info.audio_block_frame_offset
                                + num_frames_written,
                            requester: "looper-audio-start-request",
                            note: "",
                            is_realtime: request.info().is_realtime,
                        },
                        parent_request: Some(request),
                        general_info: request.general_info,
                    };
                    let start_response = self.supplier.supply_audio(
                        &start_request,
                        &mut dest_buffer.slice_mut(num_frames_written..),
                    );
                    SupplyResponse::please_continue(
                        modulo_response.num_frames_consumed + start_response.num_frames_consumed,
                    )
                }
            }
        }
    }
}

impl<S: MidiSupplier + WithMaterialInfo + MidiSilencer> MidiSupplier for Looper<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = match self.check_relevance(request.start_frame) {
            None => {
                return self.supplier.supply_midi(request, event_list);
            }
            Some(d) => d,
        };
        let modulo_start_frame = data.modulo_start_frame();
        let modulo_request = SupplyMidiRequest {
            start_frame: modulo_start_frame,
            dest_frame_count: request.dest_frame_count,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "looper-midi-modulo-request",
                note: "",
                is_realtime: request.info().is_realtime,
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let modulo_response = self.supplier.supply_midi(&modulo_request, event_list);
        if data.start_frame <= 0 {
            let end_frame = data.start_frame + modulo_response.num_frames_consumed as isize;
            if end_frame > 0 {
                debug!("Silence MIDI at loop start");
                midi_util::silence_midi(
                    event_list,
                    self.midi_reset_msg_range.left,
                    SilenceMidiBlockMode::Prepend,
                    &mut self.supplier,
                );
            }
        }
        match modulo_response.status {
            SupplyResponseStatus::PleaseContinue => modulo_response,
            SupplyResponseStatus::ReachedEnd { num_frames_written } => {
                if self.is_last_cycle(data.current_cycle) {
                    // Time to stop.
                    debug!("Silence MIDI at loop end");
                    midi_util::silence_midi(
                        event_list,
                        self.midi_reset_msg_range.right,
                        SilenceMidiBlockMode::Append,
                        &mut self.supplier,
                    );
                    modulo_response
                } else if num_frames_written == request.dest_frame_count {
                    // Perfect landing, source completely consumed. Start next cycle.
                    SupplyResponse::please_continue(modulo_response.num_frames_consumed)
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
                                + num_frames_written,
                            requester: "looper-midi-start-request",
                            note: "",
                            is_realtime: request.info().is_realtime,
                        },
                        parent_request: Some(request),
                        general_info: request.general_info,
                    };
                    let start_response = self.supplier.supply_midi(&start_request, event_list);
                    // We don't add modulo_response.num_frames_consumed because that number of
                    // consumed frames is already contained in the number returned in the start
                    // response (because we started at a negative start position).
                    SupplyResponse::please_continue(start_response.num_frames_consumed)
                }
            }
        }
    }
}

pub fn get_cycle_at_frame(frame: isize, frame_count: usize) -> usize {
    if frame < 0 {
        return 0;
    }
    frame as usize / frame_count
}

impl<S: PositionTranslationSkill + WithMaterialInfo> PositionTranslationSkill for Looper<S> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        let effective_play_pos = match self.check_relevance(play_pos) {
            None => play_pos,
            Some(d) => d.modulo_start_frame(),
        };
        self.supplier
            .translate_play_pos_to_source_pos(effective_play_pos)
    }
}

impl<S> AutoDelegatingWithMaterialInfo for Looper<S> {}
impl<S> AutoDelegatingMidiSilencer for Looper<S> {}
