use crate::conversion_util::adjust_proportionally_positive;
use crate::rt::supplier::{
    MidiSilencer, SupplyMidiRequest, SupplyRequest, SupplyResponse, SupplyResponseStatus,
};
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use playtime_api::persistence::MidiResetMessages;
use reaper_medium::{BorrowedMidiEventList, Hz, MidiEvent, MidiFrameOffset};

/// Helper function for MIDI suppliers that read from sources and don't want to deal with
/// negative start frames themselves.
///
/// Below logic assumes that the destination frame rate is comparable to the source frame
/// rate. The resampler makes sure of it. However, it's not necessarily equal since we use
/// frame rate changes for tempo changes. It's only equal if the clip is played in
/// MIDI_BASE_BPM. That's fine!
pub fn supply_midi_material(
    request: &SupplyMidiRequest,
    supply_inner: impl FnOnce(MidiSourceMaterialRequest) -> SupplyResponse,
) -> SupplyResponse {
    let ideal_num_consumed_frames = request.dest_frame_count;
    let ideal_end_frame = request.start_frame + ideal_num_consumed_frames as isize;
    if ideal_end_frame <= 0 {
        // Requested portion is located entirely before the actual source material.
        // We haven't reached the end of the source, so still tell the caller that we
        // wrote all frames. And advance the count-in phase.
        return SupplyResponse::please_continue(ideal_num_consumed_frames);
    }
    // Requested portion contains playable material.
    if request.start_frame < 0 {
        // Portion overlaps start of material.
        let num_skipped_frames_in_source = -request.start_frame as usize;
        let proportion_skipped =
            num_skipped_frames_in_source as f64 / ideal_num_consumed_frames as f64;
        let num_skipped_frames_in_dest =
            adjust_proportionally_positive(ideal_num_consumed_frames as f64, proportion_skipped);
        let req = MidiSourceMaterialRequest {
            start_frame: 0,
            dest_frame_count: ideal_num_consumed_frames - num_skipped_frames_in_dest,
            dest_sample_rate: request.dest_sample_rate,
        };
        let res = supply_inner(req);
        return SupplyResponse {
            num_frames_consumed: num_skipped_frames_in_source + res.num_frames_consumed,
            status: match res.status {
                SupplyResponseStatus::PleaseContinue => SupplyResponseStatus::PleaseContinue,
                SupplyResponseStatus::ReachedEnd { num_frames_written } => {
                    // Oh, that's short material.
                    SupplyResponseStatus::ReachedEnd {
                        num_frames_written: num_skipped_frames_in_dest + num_frames_written,
                    }
                }
            },
        };
    }
    // Requested portion is located on or after start of the actual source material.
    let req = MidiSourceMaterialRequest {
        start_frame: request.start_frame as usize,
        dest_frame_count: ideal_num_consumed_frames,
        dest_sample_rate: request.dest_sample_rate,
    };
    supply_inner(req)
}

pub struct MidiSourceMaterialRequest {
    pub start_frame: usize,
    pub dest_frame_count: usize,
    pub dest_sample_rate: Hz,
}

pub fn silence_midi(
    event_list: &mut BorrowedMidiEventList,
    reset_messages: MidiResetMessages,
    block_mode: SilenceMidiBlockMode,
    silencer: &mut dyn MidiSilencer,
) {
    if !reset_messages.at_least_one_enabled() {
        return;
    }
    use SilenceMidiBlockMode::*;
    let frame_offset = match block_mode {
        Prepend => {
            for evt in event_list.iter_mut() {
                if evt.frame_offset() == MidiFrameOffset::MIN {
                    evt.set_frame_offset(MidiFrameOffset::new(1));
                }
            }
            MidiFrameOffset::MIN
        }
        Append => event_list
            .iter()
            .map(|evt| evt.frame_offset())
            .max()
            .map(|o| MidiFrameOffset::new(o.get() + 1))
            .unwrap_or(MidiFrameOffset::MIN),
    };
    // At the moment, resetting on-notes is only supported as append (= at the end). If we would ask
    // to release notes when prepending (at the start), we would need to make sure that the
    // to-be-released notes are captured *before* filling the event list. But this function is
    // usually called *after* filling the event list.
    if reset_messages.on_notes_off && block_mode == Append {
        silencer.release_notes(frame_offset, event_list);
    }
    for ch in 0..16 {
        let mut append_reset = |cc| {
            let msg = RawShortMessage::control_change(Channel::new(ch), cc, U7::MIN);
            add_midi_event(event_list, frame_offset, msg);
        };
        if reset_messages.all_notes_off {
            append_reset(controller_numbers::ALL_NOTES_OFF);
        }
        if reset_messages.all_sound_off {
            append_reset(controller_numbers::ALL_SOUND_OFF);
        }
        if reset_messages.reset_all_controllers {
            append_reset(controller_numbers::RESET_ALL_CONTROLLERS);
        }
        if reset_messages.damper_pedal_off {
            append_reset(controller_numbers::DAMPER_PEDAL_ON_OFF);
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum SilenceMidiBlockMode {
    Prepend,
    Append,
}

fn add_midi_event(
    event_list: &mut BorrowedMidiEventList,
    frame_offset: MidiFrameOffset,
    msg: RawShortMessage,
) {
    let mut event = MidiEvent::default();
    event.set_frame_offset(frame_offset);
    event.set_message(msg);
    event_list.add_item(&event);
}
