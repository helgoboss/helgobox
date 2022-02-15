use crate::buffer::AudioBufMut;
use crate::source_util::pcm_source_is_midi;
use crate::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    convert_position_in_frames_to_seconds, convert_position_in_seconds_to_frames,
    print_distance_from_beat_start_at, supply_source_material, AudioSupplier, ExactDuration,
    ExactFrameCount, MidiSupplier, SourceMaterialRequest, SupplyAudioRequest, SupplyMidiRequest,
    SupplyResponse, WithFrameRate,
};
use crate::{adjust_proportionally, adjust_proportionally_positive, WithTempo};
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, Bpm, DurationInSeconds, Hz, OwnedPcmSource,
    PcmSourceTransfer, PositionInSeconds,
};

impl AudioSupplier for OwnedPcmSource {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        supply_source_material(request, dest_buffer, get_frame_rate(self), |input| {
            transfer_audio(self, input)
        })
    }

    fn channel_count(&self) -> usize {
        self.get_num_channels()
            .expect("source doesn't report channel count") as usize
    }
}

impl WithFrameRate for OwnedPcmSource {
    fn frame_rate(&self) -> Option<Hz> {
        Some(get_frame_rate(self))
    }
}

impl ExactDuration for OwnedPcmSource {
    fn duration(&self) -> DurationInSeconds {
        if pcm_source_is_midi(self) {
            // For MIDI, get_length() takes the current project tempo in account ... which is not
            // what we want because we want to do all the tempo calculations ourselves and treat
            // MIDI/audio the same wherever possible.
            let beats = self
                .get_length_beats()
                .expect("MIDI source must have length in beats");
            let beats_per_minute = MIDI_BASE_BPM;
            let beats_per_second = beats_per_minute / 60.0;
            DurationInSeconds::new(beats.get() / beats_per_second)
        } else {
            self.get_length().unwrap_or(DurationInSeconds::ZERO)
        }
    }
}

impl ExactFrameCount for OwnedPcmSource {
    fn frame_count(&self) -> usize {
        convert_duration_in_seconds_to_frames(self.duration(), get_frame_rate(self))
    }
}

fn get_frame_rate(source: &BorrowedPcmSource) -> Hz {
    if pcm_source_is_midi(source) {
        Hz::new(MIDI_FRAME_RATE)
    } else {
        source
            .get_sample_rate()
            .expect("audio source should expose frame rate")
    }
}

impl MidiSupplier for OwnedPcmSource {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let midi_frame_rate = Hz::new(MIDI_FRAME_RATE);
        // As with audio, the ratio between output frame count and output sample rate determines
        // the playback tempo.
        let input_ratio = request.dest_frame_count as f64 / request.dest_sample_rate.get();
        let num_midi_frames_requested =
            adjust_proportionally_positive(midi_frame_rate.get(), input_ratio);
        if request.start_frame == 0 {
            print_distance_from_beat_start_at(request, 0, "(MIDI, start_frame = 0)");
        } else if request.start_frame < 0
            && (request.start_frame + num_midi_frames_requested as isize) >= 0
        {
            let distance_to_zero_in_midi_frames = (-request.start_frame) as usize;
            let ratio = request.dest_frame_count as f64 / num_midi_frames_requested as f64;
            let distance_to_zero_in_dest_frames =
                adjust_proportionally_positive(distance_to_zero_in_midi_frames as f64, ratio);
            print_distance_from_beat_start_at(
                request,
                distance_to_zero_in_dest_frames,
                "(MIDI, start_frame < 0)",
            );
        }
        // For MIDI it seems to be okay to start at a negative position. The source
        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
        // offset.
        let time_s = convert_position_in_frames_to_seconds(request.start_frame, midi_frame_rate);
        let num_midi_frames_consumed = unsafe {
            let mut transfer = PcmSourceTransfer::default();
            transfer.set_sample_rate(midi_frame_rate);
            transfer.set_length(num_midi_frames_requested as i32);
            transfer.set_time_s(time_s);
            // Force MIDI tempo, then *we* can deal with on-the-fly tempo changes that occur while
            // playing instead of REAPER letting use its generic mechanism that leads to duplicate
            // notes, probably through internal position changes. Setting the absolute time to
            // zero prevents repeated notes when turning the tempo down. According to Justin this
            // prevents the "re-sync-on-project-change logic".
            transfer.set_force_bpm(Bpm::new(MIDI_BASE_BPM));
            transfer.set_absolute_time_s(PositionInSeconds::ZERO);
            transfer.set_midi_event_list(event_list);
            self.get_samples(&transfer);
            transfer.samples_out() as usize
        };
        let num_dest_frames_written = if num_midi_frames_consumed == num_midi_frames_requested {
            request.dest_frame_count
        } else {
            let ratio = num_midi_frames_consumed as f64 / num_midi_frames_requested as f64;
            adjust_proportionally_positive(request.dest_frame_count as f64, ratio)
        };
        // The lower the sample rate, the higher the tempo, the more inner source material we
        // effectively grabbed.
        SupplyResponse::limited_by_total_frame_count(
            num_midi_frames_consumed,
            num_dest_frames_written,
            request.start_frame,
            self.frame_count(),
        )
    }
}

impl WithTempo for OwnedPcmSource {
    fn tempo(&self) -> Option<Bpm> {
        if pcm_source_is_midi(self) {
            Some(Bpm::new(MIDI_BASE_BPM))
        } else {
            None
        }
    }
}

fn transfer_audio(source: &OwnedPcmSource, mut req: SourceMaterialRequest) -> SupplyResponse {
    let time_s = convert_duration_in_frames_to_seconds(req.start_frame, req.source_sample_rate);
    let num_frames_written = unsafe {
        let mut transfer = PcmSourceTransfer::default();
        transfer.set_nch(req.dest_buffer.channel_count() as _);
        transfer.set_length(req.dest_buffer.frame_count() as _);
        transfer.set_sample_rate(req.dest_sample_rate);
        transfer.set_samples(req.dest_buffer.data_as_mut_ptr());
        transfer.set_time_s(time_s.into());
        source.get_samples(&transfer);
        transfer.samples_out() as usize
    };
    // The lower the sample rate, the higher the tempo, the more inner source material we
    // effectively grabbed.
    let consumed_time_in_seconds =
        DurationInSeconds::new(num_frames_written as f64 / req.dest_sample_rate.get());
    let num_frames_consumed =
        convert_duration_in_seconds_to_frames(consumed_time_in_seconds, req.source_sample_rate);
    SupplyResponse::limited_by_total_frame_count(
        num_frames_consumed,
        num_frames_written,
        req.start_frame as isize,
        source.frame_count(),
    )
}

/// We could use just any unit to represent a position within a MIDI source, but we choose frames
/// with regard to the following frame rate. Choosing frames allows us to treat MIDI similar to
/// audio, which results in fewer special cases. The frame rate of 1,024,000 is also the unit which
/// is used in REAPER's MIDI events, so this corresponds nicely to the audio world where one sample
/// frame is the smallest possible unit.
const MIDI_FRAME_RATE: f64 = 1_024_000.0;

/// MIDI data is tempo-less. But pretending that all MIDI clips have a fixed tempo allows us to
/// treat MIDI similar to audio. E.g. if we want it to play faster, we just lower the output sample
/// rate. Plus, we can use the same time stretching supplier. Fewer special cases, nice!
pub const MIDI_BASE_BPM: f64 = 120.0;
