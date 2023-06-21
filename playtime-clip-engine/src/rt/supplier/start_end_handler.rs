use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::fade_util::{
    apply_fade_in_starting_at_zero, apply_fade_out_ending_at, START_END_FADE_LENGTH,
};
use crate::rt::supplier::midi_util::SilenceMidiBlockMode;
use crate::rt::supplier::{
    midi_util, AudioSupplier, MaterialInfo, MidiSilencer, MidiSupplier, PositionTranslationSkill,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithMaterialInfo,
};
use crate::ClipEngineResult;
use playtime_api::persistence::MidiResetMessageRange;
use reaper_medium::{BorrowedMidiEventList, MidiFrameOffset};

#[derive(Debug)]
pub struct StartEndHandler<S> {
    supplier: S,
    audio_fades_enabled: bool,
    enabled_for_start: bool,
    enabled_for_end: bool,
    midi_reset_msg_range: MidiResetMessageRange,
}

impl<S> StartEndHandler<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            audio_fades_enabled: false,
            enabled_for_start: false,
            enabled_for_end: false,
            midi_reset_msg_range: Default::default(),
        }
    }

    pub fn set_audio_fades_enabled(&mut self, enabled: bool) {
        self.audio_fades_enabled = enabled;
    }

    pub fn set_midi_reset_msg_range(&mut self, range: MidiResetMessageRange) {
        self.midi_reset_msg_range = range;
    }

    pub fn set_enabled_for_start(&mut self, enabled: bool) {
        self.enabled_for_start = enabled;
    }

    pub fn set_enabled_for_end(&mut self, enabled: bool) {
        self.enabled_for_end = enabled;
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }
}

impl<S: AudioSupplier + WithMaterialInfo> AudioSupplier for StartEndHandler<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let response = self.supplier.supply_audio(request, dest_buffer);
        if !self.audio_fades_enabled {
            return response;
        }
        if self.enabled_for_start {
            apply_fade_in_starting_at_zero(dest_buffer, request.start_frame, START_END_FADE_LENGTH);
        }
        let frame_count = self.supplier.material_info().unwrap().frame_count();
        if self.enabled_for_end {
            apply_fade_out_ending_at(
                dest_buffer,
                request.start_frame,
                frame_count,
                START_END_FADE_LENGTH,
            );
        }
        response
    }
}

impl<S: MidiSupplier + MidiSilencer> MidiSupplier for StartEndHandler<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let response = self.supplier.supply_midi(request, event_list);
        if self.enabled_for_start && request.start_frame <= 0 {
            let end_frame = request.start_frame + response.num_frames_consumed as isize;
            if end_frame > 0 {
                debug!("Silence MIDI at source start");
                midi_util::silence_midi(
                    event_list,
                    self.midi_reset_msg_range.left,
                    SilenceMidiBlockMode::Prepend,
                    &mut self.supplier,
                );
            }
        }
        if self.enabled_for_end && response.status.reached_end() {
            // TODO-high-clip-engine This is sent repeatedly when the section exceeds the source length!
            debug!("Silence MIDI at source end");
            midi_util::silence_midi(
                event_list,
                self.midi_reset_msg_range.right,
                SilenceMidiBlockMode::Append,
                &mut self.supplier,
            );
        }
        response
    }
}

impl<S: WithMaterialInfo> WithMaterialInfo for StartEndHandler<S> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.supplier.material_info()
    }
}

impl<S: PositionTranslationSkill> PositionTranslationSkill for StartEndHandler<S> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        self.supplier.translate_play_pos_to_source_pos(play_pos)
    }
}

impl<S: MidiSilencer> MidiSilencer for StartEndHandler<S> {
    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    ) {
        self.supplier.release_notes(frame_offset, event_list)
    }
}
