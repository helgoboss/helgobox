use crate::conversion_util::convert_duration_in_seconds_to_frames;
use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::fade_util::{
    apply_fade_in_starting_at_zero, apply_fade_out_ending_at, SECTION_FADE_LENGTH,
};
use crate::rt::supplier::midi_util::SilenceMidiBlockMode;
use crate::rt::supplier::{
    midi_util, AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiMaterialInfo, MidiSupplier,
    PositionTranslationSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyRequest,
    SupplyRequestInfo, SupplyResponse, SupplyResponseStatus, WithMaterialInfo,
};
use crate::ClipEngineResult;
use playtime_api::persistence::{MidiResetMessageRange, PositiveSecond};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, MidiFrameOffset};

#[derive(Debug)]
pub struct Section<S> {
    supplier: S,
    bounds: SectionBounds,
    midi_reset_msg_range: MidiResetMessageRange,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct SectionBounds {
    start_frame: usize,
    length: Option<usize>,
}

impl SectionBounds {
    pub fn new(start_frame: usize, length: Option<usize>) -> Self {
        Self {
            start_frame,
            length,
        }
    }

    pub fn is_default(&self) -> bool {
        self == &Default::default()
    }

    pub fn calculate_frame_count(&self, supplier_frame_count: usize) -> usize {
        if let Some(length) = self.length {
            length
        } else {
            supplier_frame_count.saturating_sub(self.start_frame)
        }
    }

    pub fn start_frame(&self) -> usize {
        self.start_frame
    }

    pub fn length(&self) -> Option<usize> {
        self.length
    }
}

impl<S> Section<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            bounds: Default::default(),
            midi_reset_msg_range: Default::default(),
        }
    }

    pub fn set_midi_reset_msg_range(&mut self, range: MidiResetMessageRange) {
        self.midi_reset_msg_range = range;
    }

    pub fn bounds(&self) -> SectionBounds {
        self.bounds
    }

    pub fn set_bounds(&mut self, start_frame: usize, length: Option<usize>) {
        self.bounds.start_frame = start_frame;
        self.bounds.length = length;
    }

    pub fn reset(&mut self) {
        self.bounds = Default::default();
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn set_bounds_in_seconds(
        &mut self,
        start: PositiveSecond,
        length: Option<PositiveSecond>,
        material_info: &MaterialInfo,
    ) -> ClipEngineResult<()>
    where
        S: WithMaterialInfo,
    {
        let source_frame_rate = material_info.frame_rate();
        let start_frame = convert_duration_in_seconds_to_frames(
            DurationInSeconds::new(start.get()),
            source_frame_rate,
        );
        let frame_count = length.map(|l| {
            convert_duration_in_seconds_to_frames(
                DurationInSeconds::new(l.get()),
                source_frame_rate,
            )
        });
        self.set_bounds(start_frame, frame_count);
        Ok(())
    }

    fn get_instruction(
        &mut self,
        request: &impl SupplyRequest,
        dest_frame_count: usize,
    ) -> Instruction {
        if self.bounds.is_default() {
            return Instruction::Bypass;
        }
        // Section is set (start and/or length).
        // This logic assumes that the destination frame rate is comparable to the
        // source frame rate. The resampler (which sits on top of this supplier)
        // takes care of that.
        let ideal_num_frames_to_be_consumed = dest_frame_count;
        let ideal_end_frame_in_section =
            request.start_frame() + ideal_num_frames_to_be_consumed as isize;
        if ideal_end_frame_in_section <= 0 {
            // Pure count-in phase. Pass through for now.
            return Instruction::Bypass;
        }
        // TODO-high-downbeat If the start frame is < 0 and the end frame is > 0, we currently play
        //  some material which is shortly before the section start. I think one effect of this is
        //  that the MIDI piano clip sometimes plays the F note when using this boundary:
        //             boundary: Boundary {
        //                 start_frame: 1_024_000,
        //                 length: Some(1_024_000),
        //             }
        // Determine source range
        let start_frame_in_source = self.bounds.start_frame as isize + request.start_frame();
        let (phase_two, num_frames_to_be_written) = match self.bounds.length {
            None => {
                // Section doesn't have right bound (easy).
                (PhaseTwo::Unbounded, dest_frame_count)
            }
            Some(length) => {
                // Section has right bound.
                if request.start_frame() >= length as isize {
                    // We exceeded the section boundary. Return silence.
                    return Instruction::Return(SupplyResponse::exceeded_end());
                }
                // We are still within the section.
                let right_bound_in_source = self.bounds.start_frame + length;
                let ideal_end_frame_in_source =
                    start_frame_in_source + ideal_num_frames_to_be_consumed as isize;
                let (reached_bound, effective_end_frame_in_source) =
                    if ideal_end_frame_in_source <= right_bound_in_source as isize {
                        // End of block is located before or on end of section end
                        (false, ideal_end_frame_in_source)
                    } else {
                        // End of block is located behind section end
                        (true, right_bound_in_source as isize)
                    };
                let bounded_num_frames_to_be_consumed =
                    (effective_end_frame_in_source - start_frame_in_source) as usize;
                let bounded_num_frames_to_be_written = bounded_num_frames_to_be_consumed;
                let phase_two = PhaseTwo::Bounded {
                    reached_bound,
                    bounded_num_frames_to_be_consumed,
                    bounded_num_frames_to_be_written,
                    ideal_num_frames_to_be_consumed,
                };
                (phase_two, bounded_num_frames_to_be_written)
            }
        };
        let data = SectionRequestData {
            phase_one: PhaseOne {
                start_frame: start_frame_in_source,
                info: SupplyRequestInfo {
                    audio_block_frame_offset: request.info().audio_block_frame_offset,
                    requester: "section-request",
                    note: "",
                    is_realtime: request.info().is_realtime,
                },
                num_frames_to_be_written,
            },
            phase_two,
        };
        Instruction::ApplySection(data)
    }

    fn generate_outer_response(
        &self,
        inner_response: SupplyResponse,
        phase_two: PhaseTwo,
    ) -> SupplyResponse {
        use PhaseTwo::*;
        match phase_two {
            Unbounded => {
                // Section has open end. In that case the inner response is valid.
                inner_response
            }
            Bounded {
                reached_bound,
                bounded_num_frames_to_be_consumed,
                bounded_num_frames_to_be_written,
                ideal_num_frames_to_be_consumed,
            } => {
                // Section has right bound.
                if reached_bound {
                    // Bound reached.
                    SupplyResponse::reached_end(
                        bounded_num_frames_to_be_consumed,
                        bounded_num_frames_to_be_written,
                    )
                } else {
                    // Bound not yet reached.
                    use SupplyResponseStatus::*;
                    match inner_response.status {
                        PleaseContinue => {
                            // Source has more material.
                            SupplyResponse::please_continue(bounded_num_frames_to_be_consumed)
                        }
                        ReachedEnd { .. } => {
                            // Source has reached its end (but the boundary is not reached yet).
                            SupplyResponse::please_continue(ideal_num_frames_to_be_consumed)
                        }
                    }
                }
            }
        }
    }
}

impl<S: AudioSupplier> AudioSupplier for Section<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = match self.get_instruction(request, dest_buffer.frame_count()) {
            Instruction::Bypass => {
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Instruction::Return(r) => return r,
            Instruction::ApplySection(d) => d,
        };
        let inner_request = SupplyAudioRequest {
            start_frame: data.phase_one.start_frame,
            dest_sample_rate: request.dest_sample_rate,
            info: data.phase_one.info,
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let mut inner_dest_buffer =
            dest_buffer.slice_mut(0..data.phase_one.num_frames_to_be_written);
        let inner_response = self
            .supplier
            .supply_audio(&inner_request, &mut inner_dest_buffer);
        if self.bounds.start_frame > 0 {
            apply_fade_in_starting_at_zero(dest_buffer, request.start_frame, SECTION_FADE_LENGTH);
        }
        if let Some(length) = self.bounds.length {
            apply_fade_out_ending_at(
                dest_buffer,
                request.start_frame,
                length,
                SECTION_FADE_LENGTH,
            );
        }
        self.generate_outer_response(inner_response, data.phase_two)
    }
}

impl<S: MidiSupplier> MidiSupplier for Section<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = match self.get_instruction(request, request.dest_frame_count) {
            Instruction::Bypass => {
                return self.supplier.supply_midi(request, event_list);
            }
            Instruction::Return(r) => return r,
            Instruction::ApplySection(d) => d,
        };
        let inner_request = SupplyMidiRequest {
            start_frame: data.phase_one.start_frame,
            dest_frame_count: data.phase_one.num_frames_to_be_written,
            info: data.phase_one.info.clone(),
            dest_sample_rate: request.dest_sample_rate,
            parent_request: request.parent_request,
            general_info: request.general_info,
        };
        let inner_response = self.supplier.supply_midi(&inner_request, event_list);
        // Reset MIDI at start if necessary
        if request.start_frame <= 0 {
            debug!("Silence MIDI at section start");
            midi_util::silence_midi(
                event_list,
                self.midi_reset_msg_range.left,
                SilenceMidiBlockMode::Prepend,
                &mut self.supplier,
            );
        }
        // Reset MIDI at end if necessary
        if let PhaseTwo::Bounded {
            reached_bound: true,
            ..
        } = &data.phase_two
        {
            debug!("Silence MIDI at section end");
            midi_util::silence_midi(
                event_list,
                self.midi_reset_msg_range.right,
                SilenceMidiBlockMode::Append,
                &mut self.supplier,
            );
        }
        self.generate_outer_response(inner_response, data.phase_two)
    }

    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    ) {
        self.supplier.release_notes(frame_offset, event_list);
    }
}

impl<S: WithMaterialInfo> WithMaterialInfo for Section<S> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        let inner_material_info = self.supplier.material_info()?;
        if self.bounds.is_default() {
            return Ok(inner_material_info);
        }
        let material_info = match inner_material_info {
            MaterialInfo::Audio(i) => {
                let i = AudioMaterialInfo {
                    frame_count: self.bounds.calculate_frame_count(i.frame_count),
                    ..i
                };
                MaterialInfo::Audio(i)
            }
            MaterialInfo::Midi(i) => {
                let i = MidiMaterialInfo {
                    frame_count: self.bounds.calculate_frame_count(i.frame_count),
                };
                MaterialInfo::Midi(i)
            }
        };
        Ok(material_info)
    }
}

enum Instruction {
    Bypass,
    ApplySection(SectionRequestData),
    Return(SupplyResponse),
}

struct SectionRequestData {
    phase_one: PhaseOne,
    phase_two: PhaseTwo,
}

struct PhaseOne {
    start_frame: isize,
    info: SupplyRequestInfo,
    num_frames_to_be_written: usize,
}

enum PhaseTwo {
    Unbounded,
    Bounded {
        reached_bound: bool,
        bounded_num_frames_to_be_consumed: usize,
        bounded_num_frames_to_be_written: usize,
        ideal_num_frames_to_be_consumed: usize,
    },
}

impl<S: PositionTranslationSkill> PositionTranslationSkill for Section<S> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        let effective_play_pos = self.bounds.start_frame as isize + play_pos;
        self.supplier
            .translate_play_pos_to_source_pos(effective_play_pos)
    }
}
