use crate::{
    convert_duration_in_frames_to_seconds, AudioBufMut, AudioSupplier, ExactDuration,
    ExactFrameCount, MidiSupplier, SupplyAudioRequest, SupplyMidiRequest, SupplyRequest,
    SupplyRequestGeneralInfo, SupplyRequestInfo, SupplyResponse, WithFrameRate,
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

impl<S> Section<S> {
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

    fn create_section_request_data(
        &mut self,
        request: &impl SupplyRequest,
        dest_frame_count: usize,
    ) -> SectionRequestData {
        let inner_start_frame = request.start_frame() + self.boundary.start_frame as isize;
        let inner_end_frame = inner_start_frame + dest_frame_count as isize;
        let boundary_end_frame = self.boundary.end_frame().map(|f| f as isize);
        let inner_end_frame = if let Some(end_frame) = boundary_end_frame {
            cmp::min(end_frame, inner_end_frame)
        } else {
            inner_end_frame
        };
        let inner_block_length = (inner_end_frame - inner_start_frame) as usize;
        SectionRequestData {
            start_frame: inner_start_frame,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info().audio_block_frame_offset,
                requester: "section-audio-request",
                note: "",
            },
            inner_block_length,
            boundary_end_frame,
        }
    }
}

impl<S: AudioSupplier> AudioSupplier for Section<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        if self.boundary.is_default() {
            return self.supplier.supply_audio(request, dest_buffer);
        }
        let data = self.create_section_request_data(request, dest_buffer.frame_count());
        let section_request = SupplyAudioRequest {
            start_frame: data.start_frame,
            dest_sample_rate: request.dest_sample_rate,
            info: data.info,
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let mut inner_dest_buffer = dest_buffer.slice_mut(0..data.inner_block_length);
        let section_response = self
            .supplier
            .supply_audio(&section_request, &mut inner_dest_buffer);
        generate_outer_response(section_response, data.boundary_end_frame)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier> MidiSupplier for Section<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        if self.boundary.is_default() {
            return self.supplier.supply_midi(request, event_list);
        }
        let data = self.create_section_request_data(request, request.dest_frame_count);
        let section_request = SupplyMidiRequest {
            start_frame: data.start_frame,
            dest_frame_count: data.inner_block_length,
            info: data.info.clone(),
            dest_sample_rate: request.dest_sample_rate,
            parent_request: request.parent_request,
            general_info: request.general_info,
        };
        let section_response = self.supplier.supply_midi(&section_request, event_list);
        generate_outer_response(section_response, data.boundary_end_frame)
    }
}

impl<S: WithFrameRate> WithFrameRate for Section<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: ExactFrameCount> ExactFrameCount for Section<S> {
    fn frame_count(&self) -> usize {
        let source_frame_count = self.supplier.frame_count();
        let remaining_frame_count = source_frame_count.saturating_sub(self.boundary.start_frame);
        if let Some(length) = self.boundary.length {
            cmp::min(length, remaining_frame_count)
        } else {
            remaining_frame_count
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

struct SectionRequestData {
    start_frame: isize,
    info: SupplyRequestInfo,
    inner_block_length: usize,
    boundary_end_frame: Option<isize>,
}

fn generate_outer_response(
    section_response: SupplyResponse,
    boundary_end_frame: Option<isize>,
) -> SupplyResponse {
    SupplyResponse {
        next_inner_frame: if let (Some(next_inner_frame), Some(boundary_end_frame)) =
            (section_response.next_inner_frame, boundary_end_frame)
        {
            if next_inner_frame < boundary_end_frame as isize {
                Some(next_inner_frame)
            } else {
                None
            }
        } else {
            section_response.next_inner_frame
        },
        ..section_response
    }
}
