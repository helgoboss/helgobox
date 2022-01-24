use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::source_util::pcm_source_is_midi;
use crate::domain::clip_engine::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    convert_position_in_frames_to_seconds, convert_position_in_seconds_to_frames,
    supply_source_material, AudioSupplier, ExactDuration, ExactFrameCount, MidiSupplier,
    SourceMaterialRequest, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
    MIDI_BASE_BPM,
};
use crate::domain::clip_engine::WithTempo;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, Bpm, DurationInSeconds, Hz, OwnedPcmSource,
    PcmSourceTransfer, PositionInSeconds,
};

impl AudioSupplier for OwnedPcmSource {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        supply_source_material(request, dest_buffer, self.frame_rate(), |input| {
            transfer_audio(self, input)
        })
    }

    fn channel_count(&self) -> usize {
        self.get_num_channels()
            .expect("source doesn't report channel count") as usize
    }
}

impl WithFrameRate for OwnedPcmSource {
    fn frame_rate(&self) -> Hz {
        if pcm_source_is_midi(self) {
            Hz::new(MIDI_FRAME_RATE)
        } else {
            self.get_sample_rate()
                .expect("source doesn't report a sample rate")
        }
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
        convert_duration_in_seconds_to_frames(self.duration(), self.frame_rate())
    }
}

impl MidiSupplier for OwnedPcmSource {
    fn supply_midi(
        &self,
        req: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let midi_frame_rate = Hz::new(MIDI_FRAME_RATE);
        // As with audio, the ratio between output frame count and output sample rate determines
        // the playback tempo.
        let input_ratio = req.dest_frame_count as f64 / req.dest_sample_rate.get();
        let normalized_dest_frame_count = (input_ratio * midi_frame_rate.get()).round() as usize;
        // For MIDI it seems to be okay to start at a negative position. The source
        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
        // offset.
        let time_s = convert_position_in_frames_to_seconds(req.start_frame, midi_frame_rate);
        let num_frames_consumed = unsafe {
            let mut transfer = PcmSourceTransfer::default();
            transfer.set_sample_rate(midi_frame_rate);
            transfer.set_length(normalized_dest_frame_count as i32);
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
        let num_frames_written = if num_frames_consumed == normalized_dest_frame_count {
            req.dest_frame_count
        } else {
            let ratio = num_frames_consumed as f64 / normalized_dest_frame_count as f64;
            (ratio * req.dest_frame_count as f64).round() as usize
        };
        // The lower the sample rate, the higher the tempo, the more inner source material we
        // effectively grabbed.
        let next_frame = req.start_frame + num_frames_consumed as isize;
        let source_frame_count = self.frame_count();
        SupplyResponse {
            num_frames_written,
            num_frames_consumed,
            next_inner_frame: if next_frame < source_frame_count as isize {
                Some(next_frame)
            } else {
                None
            },
        }
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
    let next_frame = req.start_frame + num_frames_consumed;
    SupplyResponse {
        num_frames_written,
        num_frames_consumed,
        next_inner_frame: if next_frame < source.frame_count() {
            Some(next_frame as isize)
        } else {
            None
        },
    }
}

/// We could use just any unit to represent a position within a MIDI source, but we choose frames
/// with regard to the following frame rate. Choosing frames allows us to treat MIDI similar to
/// audio, which results in fewer special cases. The frame rate of 1,024,000 is also the unit which
/// is used in REAPER's MIDI events, so this corresponds nicely to the audio world where one sample
/// frame is the smallest possible unit.
const MIDI_FRAME_RATE: f64 = 1_024_000.0;
