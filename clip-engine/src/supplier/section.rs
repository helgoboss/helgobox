use crate::supplier::{SupplyResponse, SupplyResponseStatus};
use crate::{
    convert_duration_in_frames_to_other_frame_rate, convert_duration_in_frames_to_seconds,
    AudioBufMut, AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier, SupplyAudioRequest,
    SupplyMidiRequest, SupplyRequest, SupplyRequestGeneralInfo, SupplyRequestInfo, WithFrameRate,
};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz};
use std::cmp;

#[derive(Debug)]
pub struct Section<S> {
    supplier: S,
    boundary: Boundary,
}

#[derive(PartialEq, Debug, Default)]
struct Boundary {
    start_frame: usize,
    length: Option<usize>,
}

impl Boundary {
    fn end_frame(&self) -> Option<usize> {
        self.length.map(|l| self.start_frame + l)
    }

    fn is_default(&self) -> bool {
        self == &Default::default()
    }
}

impl<S: WithFrameRate + ExactFrameCount> Section<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            boundary: Default::default(),
        }
    }

    pub fn set_start_frame(&mut self, start_frame: usize) {
        self.boundary.start_frame = start_frame;
    }

    pub fn set_length(&mut self, length: Option<usize>) {
        self.boundary.length = length;
    }

    pub fn reset(&mut self) {
        self.boundary = Default::default();
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    fn get_instruction(
        &mut self,
        request: &impl SupplyRequest,
        dest_frame_count: usize,
        dest_frame_rate: Hz,
        is_midi: bool,
    ) -> Instruction {
        if self.boundary.is_default() {
            return Instruction::PassThrough;
        }
        let source_frame_rate = self
            .supplier
            .frame_rate()
            .expect("supplier doesn't have frame rate yet");
        if !is_midi {
            assert_eq!(source_frame_rate, dest_frame_rate);
        }
        // For audio, the source and destination frame rate are always equal in our chain setup.
        let hypothetical_num_source_frames_to_be_consumed = if is_midi {
            convert_duration_in_frames_to_other_frame_rate(
                dest_frame_count,
                dest_frame_rate,
                source_frame_rate,
            )
        } else {
            dest_frame_count
        };
        let hypothetical_end_frame_in_section =
            request.start_frame() + hypothetical_num_source_frames_to_be_consumed as isize;
        if hypothetical_end_frame_in_section < 0 {
            // Count-in phase
            return Instruction::PassThrough;
        }
        // TODO-high If the start frame is < 0 and the end frame is > 0, we currently play
        //  some material which is shortly before the section start. I think one effect of this is
        //  that the MIDI piano clip sometimes plays the F note when using this boundary:
        //             boundary: Boundary {
        //                 start_frame: 1_024_000,
        //                 length: Some(1_024_000),
        //             }
        //  Let's deal with that as soon as we add support for customizable downbeats.
        let boundary_end_frame = self.boundary.end_frame().map(|f| f as isize);
        let start_frame_in_source = request.start_frame() + self.boundary.start_frame as isize;
        let hypothetical_end_frame_in_source =
            start_frame_in_source + hypothetical_num_source_frames_to_be_consumed as isize;
        let end_frame_in_source = if let Some(f) = boundary_end_frame {
            cmp::min(f, hypothetical_end_frame_in_source)
        } else {
            hypothetical_end_frame_in_source
        };
        let num_source_frames_to_be_consumed =
            (end_frame_in_source - start_frame_in_source) as usize;
        // For audio, the source and destination frame rate are always equal in our chain setup.
        let num_dest_frames_to_be_written = if is_midi {
            convert_duration_in_frames_to_other_frame_rate(
                num_source_frames_to_be_consumed,
                source_frame_rate,
                dest_frame_rate,
            )
        } else {
            num_source_frames_to_be_consumed
        };
        let phase_two = PhaseTwo {
            boundary_end_frame,
            hypothetical_end_frame_in_section,
            num_source_frames_to_be_consumed,
            num_dest_frames_to_be_written,
        };
        let source_frame_count = self.supplier.frame_count();
        if let Some(boundary_end_frame) = boundary_end_frame {
            if start_frame_in_source > source_frame_count as isize {
                // We are behind the end of the source but still before the boundary end.
                // Return silence.
                // TODO-medium We could also let lower suppliers handle this (make sure that silence
                //  is returned if out of material bounds).
                let response = phase_two.generate_bounded_response(boundary_end_frame);
                return Instruction::Return(response);
            }
        }
        let data = SectionRequestData {
            phase_one: PhaseOne {
                start_frame: start_frame_in_source,
                info: SupplyRequestInfo {
                    audio_block_frame_offset: request.info().audio_block_frame_offset,
                    requester: "section-request",
                    note: "",
                },
                inner_block_length: num_dest_frames_to_be_written,
            },
            phase_two,
        };
        Instruction::QueryInner(data)
    }

    fn generate_outer_response(
        &self,
        inner_response: SupplyResponse,
        phase_two: PhaseTwo,
    ) -> SupplyResponse {
        match phase_two.boundary_end_frame {
            None => {
                // Section has open end.
                inner_response
            }
            Some(boundary_end_frame) => phase_two.generate_bounded_response(boundary_end_frame),
        }
    }
}

impl<S: AudioSupplier + WithFrameRate + ExactFrameCount> AudioSupplier for Section<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = match self.get_instruction(
            request,
            dest_buffer.frame_count(),
            request.dest_sample_rate,
            false,
        ) {
            Instruction::PassThrough => {
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Instruction::Return(r) => return r,
            Instruction::QueryInner(d) => d,
        };
        let inner_request = SupplyAudioRequest {
            start_frame: data.phase_one.start_frame,
            dest_sample_rate: request.dest_sample_rate,
            info: data.phase_one.info,
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let mut inner_dest_buffer = dest_buffer.slice_mut(0..data.phase_one.inner_block_length);
        let section_response = self
            .supplier
            .supply_audio(&inner_request, &mut inner_dest_buffer);
        self.generate_outer_response(section_response, data.phase_two)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier + WithFrameRate + ExactFrameCount> MidiSupplier for Section<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = match self.get_instruction(
            request,
            request.dest_frame_count,
            request.dest_sample_rate,
            true,
        ) {
            Instruction::PassThrough => {
                return self.supplier.supply_midi(request, event_list);
            }
            Instruction::Return(r) => return r,
            Instruction::QueryInner(d) => d,
        };
        let section_request = SupplyMidiRequest {
            start_frame: data.phase_one.start_frame,
            dest_frame_count: data.phase_one.inner_block_length,
            info: data.phase_one.info.clone(),
            dest_sample_rate: request.dest_sample_rate,
            parent_request: request.parent_request,
            general_info: request.general_info,
        };
        let section_response = self.supplier.supply_midi(&section_request, event_list);
        self.generate_outer_response(section_response, data.phase_two)
    }
}

impl<S: WithFrameRate> WithFrameRate for Section<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: ExactFrameCount> ExactFrameCount for Section<S> {
    fn frame_count(&self) -> usize {
        if self.boundary.is_default() {
            return self.supplier.frame_count();
        }
        if let Some(length) = self.boundary.length {
            length
        } else {
            let source_frame_count = self.supplier.frame_count();
            source_frame_count.saturating_sub(self.boundary.start_frame)
        }
    }
}

impl<S: ExactDuration + WithFrameRate + ExactFrameCount> ExactDuration for Section<S> {
    fn duration(&self) -> DurationInSeconds {
        if self.boundary == Default::default() {
            return self.supplier.duration();
        };
        let frame_rate = match self.frame_rate() {
            None => return DurationInSeconds::MIN,
            Some(r) => r,
        };
        convert_duration_in_frames_to_seconds(self.frame_count(), frame_rate)
    }
}

enum Instruction {
    PassThrough,
    QueryInner(SectionRequestData),
    Return(SupplyResponse),
}

struct SectionRequestData {
    phase_one: PhaseOne,
    phase_two: PhaseTwo,
}

struct PhaseOne {
    start_frame: isize,
    info: SupplyRequestInfo,
    inner_block_length: usize,
}

struct PhaseTwo {
    boundary_end_frame: Option<isize>,
    hypothetical_end_frame_in_section: isize,
    num_source_frames_to_be_consumed: usize,
    num_dest_frames_to_be_written: usize,
}

impl PhaseTwo {
    fn generate_bounded_response(&self, boundary_end_frame: isize) -> SupplyResponse {
        SupplyResponse {
            num_frames_consumed: self.num_source_frames_to_be_consumed,
            status: if self.hypothetical_end_frame_in_section < boundary_end_frame {
                SupplyResponseStatus::PleaseContinue
            } else {
                SupplyResponseStatus::ReachedEnd {
                    num_frames_written: self.num_dest_frames_to_be_written,
                }
            },
        }
    }
}
