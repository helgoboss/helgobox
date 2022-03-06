use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::fade_util::{
    apply_fade_in_starting_at_zero, apply_fade_out_ending_at, START_END_FADE_LENGTH,
};
use crate::rt::supplier::midi_util::SilenceMidiBlockMode;
use crate::rt::supplier::{
    midi_util, AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier, PreBufferFillRequest,
    PreBufferSourceSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use playtime_api::MidiResetMessageRange;
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz};

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

impl<S: AudioSupplier + ExactFrameCount> AudioSupplier for StartEndHandler<S> {
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
        if self.enabled_for_end {
            apply_fade_out_ending_at(
                dest_buffer,
                request.start_frame,
                self.supplier.frame_count(),
                START_END_FADE_LENGTH,
            );
        }
        response
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier + ExactFrameCount> MidiSupplier for StartEndHandler<S> {
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
                );
            }
        }
        if self.enabled_for_end && response.status.reached_end() {
            debug!("Silence MIDI at source end");
            midi_util::silence_midi(
                event_list,
                self.midi_reset_msg_range.right,
                SilenceMidiBlockMode::Append,
            );
        }
        response
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for StartEndHandler<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.supplier.pre_buffer(request);
    }
}

impl<S: WithFrameRate> WithFrameRate for StartEndHandler<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: ExactFrameCount> ExactFrameCount for StartEndHandler<S> {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}

impl<S: ExactDuration + WithFrameRate + ExactFrameCount> ExactDuration for StartEndHandler<S> {
    fn duration(&self) -> DurationInSeconds {
        self.supplier.duration()
    }
}