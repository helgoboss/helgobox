use crate::{
    convert_duration_in_frames_to_other_frame_rate, convert_duration_in_frames_to_seconds,
    AudioBufMut, AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier, SupplyAudioRequest,
    SupplyMidiRequest, SupplyRequest, SupplyRequestGeneralInfo, SupplyRequestInfo, SupplyResponse,
    WithFrameRate,
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

impl<S: WithFrameRate> Section<S> {
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
        dest_frame_rate: Hz,
    ) -> Option<SectionRequestData> {
        if self.boundary.is_default() {
            return None;
        }
        let source_frame_rate = self
            .supplier
            .frame_rate()
            .expect("supplier doesn't have frame rate yet");
        let hypothetical_num_source_frames_to_be_consumed =
            convert_duration_in_frames_to_other_frame_rate(
                dest_frame_count,
                dest_frame_rate,
                source_frame_rate,
            );
        let hypothetical_end_frame =
            request.start_frame() + hypothetical_num_source_frames_to_be_consumed as isize;
        if hypothetical_end_frame < 0 {
            // TODO-high If the start frame is < 0 and the end frame is > 0, we currently play
            //  some material which is shortly before the section start. Let's deal with that as
            //  soon as we add downbeat support.
            return None;
        }
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
        let num_dest_frames_to_be_written = convert_duration_in_frames_to_other_frame_rate(
            num_source_frames_to_be_consumed,
            source_frame_rate,
            dest_frame_rate,
        );
        let data = SectionRequestData {
            start_frame: start_frame_in_source,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info().audio_block_frame_offset,
                requester: "section-audio-request",
                note: "",
            },
            inner_block_length: num_dest_frames_to_be_written,
            boundary_end_frame,
        };
        Some(data)
    }

    fn generate_outer_response(
        &self,
        section_response: SupplyResponse,
        boundary_end_frame: Option<isize>,
    ) -> SupplyResponse {
        let original_next_inner_frame = if let (Some(next_inner_frame), Some(boundary_end_frame)) =
            (section_response.next_inner_frame, boundary_end_frame)
        {
            if next_inner_frame < boundary_end_frame as isize {
                Some(next_inner_frame)
            } else {
                None
            }
        } else {
            section_response.next_inner_frame
        };
        let next_inner_frame =
            original_next_inner_frame.map(|f| f - self.boundary.start_frame as isize);
        SupplyResponse {
            next_inner_frame,
            ..section_response
        }
    }
}

impl<S: AudioSupplier + WithFrameRate> AudioSupplier for Section<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = self.create_section_request_data(
            request,
            dest_buffer.frame_count(),
            request.dest_sample_rate,
        );
        let data = match data {
            None => {
                return self.supplier.supply_audio(request, dest_buffer);
            }
            Some(d) => d,
        };
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
        self.generate_outer_response(section_response, data.boundary_end_frame)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier + WithFrameRate> MidiSupplier for Section<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = self.create_section_request_data(
            request,
            request.dest_frame_count,
            request.dest_sample_rate,
        );
        let data = match data {
            None => {
                return self.supplier.supply_midi(request, event_list);
            }
            Some(d) => d,
        };
        let section_request = SupplyMidiRequest {
            start_frame: data.start_frame,
            dest_frame_count: data.inner_block_length,
            info: data.info.clone(),
            dest_sample_rate: request.dest_sample_rate,
            parent_request: request.parent_request,
            general_info: request.general_info,
        };
        let section_response = self.supplier.supply_midi(&section_request, event_list);
        self.generate_outer_response(section_response, data.boundary_end_frame)
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
