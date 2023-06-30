use crate::base::{
    ClipRecordDestination, ClipRecordHardwareInput, ClipRecordHardwareMidiInput,
    VirtualClipRecordHardwareMidiInput,
};
use crate::rt::supplier::{WriteAudioRequest, WriteMidiRequest};
use crate::rt::{AudioBuf, BasicAudioRequestProps, RtColumn};
use crate::{global_steady_timeline_state, midi_util};
use helgoboss_midi::Channel;
use reaper_high::{MidiInputDevice, Reaper};
use reaper_medium::{AudioHookRegister, MidiInputDeviceId};
use std::sync::MutexGuard;

#[derive(Debug)]
pub struct ClipEngineAudioHook {
    clip_record_task: Option<HardwareInputClipRecordTask>,
}

impl ClipEngineAudioHook {
    pub fn new() -> Self {
        Self {
            clip_record_task: None,
        }
    }
}

#[derive(Debug)]
pub struct HardwareInputClipRecordTask {
    pub input: ClipRecordHardwareInput,
    pub destination: ClipRecordDestination,
}

impl ClipEngineAudioHook {
    pub fn start_clip_recording(&mut self, task: HardwareInputClipRecordTask) {
        debug!("Audio hook received clip record task");
        self.clip_record_task = Some(task);
    }

    /// Call very early in audio hook and only if `is_post == false`.
    pub fn poll_advance_timeline(&mut self, block_props: BasicAudioRequestProps) {
        global_steady_timeline_state().on_audio_buffer(block_props);
    }

    /// Call a bit later in audio hook.
    pub fn poll_process_clip_record_tasks(
        &mut self,
        is_post: bool,
        block_props: BasicAudioRequestProps,
        audio_hook_register: &AudioHookRegister,
    ) {
        if let Some(t) = &mut self.clip_record_task {
            let its_our_turn = (t.destination.is_midi_overdub && is_post)
                || (!t.destination.is_midi_overdub && !is_post);
            if its_our_turn && !process_clip_record_task(block_props, audio_hook_register, t) {
                debug!("Clearing clip record task from audio hook");
                self.clip_record_task = None;
            }
        }
    }
}

/// Returns whether task still relevant.
fn process_clip_record_task(
    block_props: BasicAudioRequestProps,
    audio_hook_register: &AudioHookRegister,
    record_task: &mut HardwareInputClipRecordTask,
) -> bool {
    let column_source = match record_task.destination.column_source.upgrade() {
        None => return false,
        Some(s) => s,
    };
    let mut src = column_source.lock();
    if !src.recording_poll(record_task.destination.slot_index, block_props) {
        return false;
    }
    match &mut record_task.input {
        ClipRecordHardwareInput::Midi(input) => {
            use VirtualClipRecordHardwareMidiInput::*;
            let specific_input = match input {
                Specific(s) => *s,
                Detect => {
                    // Detect
                    match find_first_dev_with_play_msg() {
                        None => {
                            // No play message detected so far in any input device.
                            return true;
                        }
                        Some(dev_id) => {
                            // Found first play message in this device. Leave "Detect" mode and
                            // capture from this specific device from now on.
                            let specific_input = ClipRecordHardwareMidiInput {
                                device_id: Some(dev_id),
                                channel: None,
                            };
                            *input = Specific(specific_input);
                            specific_input
                        }
                    }
                }
            };
            if let Some(dev_id) = specific_input.device_id {
                // Read from specific MIDI input device
                let dev = Reaper::get().midi_input_device_by_id(dev_id);
                write_midi_to_clip_slot(
                    block_props,
                    &mut src,
                    record_task.destination.slot_index,
                    dev,
                    specific_input.channel,
                );
            } else {
                // Read from all open MIDI input devices
                for dev in Reaper::get().midi_input_devices() {
                    write_midi_to_clip_slot(
                        block_props,
                        &mut src,
                        record_task.destination.slot_index,
                        dev,
                        specific_input.channel,
                    );
                }
            }
        }
        ClipRecordHardwareInput::Audio(input) => {
            let channel_offset = input.channel_offset().unwrap();
            let write_audio_request = AudioHookWriteAudioRequest::new(
                audio_hook_register,
                block_props,
                channel_offset as _,
            );
            src.write_clip_audio(record_task.destination.slot_index, write_audio_request)
                .unwrap();
        }
    }
    true
}

fn find_first_dev_with_play_msg() -> Option<MidiInputDeviceId> {
    for dev in Reaper::get().midi_input_devices() {
        let contains_play_msg = dev.with_midi_input(|mi| match mi {
            None => false,
            Some(mi) => mi
                .get_read_buf()
                .into_iter()
                .any(|e| midi_util::is_play_message(e.message())),
        });
        if contains_play_msg {
            return Some(dev.id());
        }
    }
    None
}

fn write_midi_to_clip_slot(
    block_props: BasicAudioRequestProps,
    src: &mut MutexGuard<RtColumn>,
    slot_index: usize,
    dev: MidiInputDevice,
    channel_filter: Option<Channel>,
) {
    dev.with_midi_input(|mi| {
        let mi = match mi {
            None => return,
            Some(m) => m,
        };
        let events = mi.get_read_buf();
        if events.get_size() == 0 {
            return;
        }
        let req = WriteMidiRequest {
            audio_request_props: block_props,
            events,
            channel_filter,
        };
        src.write_clip_midi(slot_index, req).unwrap();
    });
}

#[derive(Copy, Clone)]
struct AudioHookWriteAudioRequest<'a> {
    channel_offset: usize,
    register: &'a AudioHookRegister,
    block_props: BasicAudioRequestProps,
}

impl<'a> AudioHookWriteAudioRequest<'a> {
    pub fn new(
        register: &'a AudioHookRegister,
        block_props: BasicAudioRequestProps,
        channel_offset: usize,
    ) -> Self {
        Self {
            channel_offset,
            register,
            block_props,
        }
    }
}

impl<'a> WriteAudioRequest for AudioHookWriteAudioRequest<'a> {
    fn audio_request_props(&self) -> BasicAudioRequestProps {
        self.block_props
    }

    fn get_channel_buffer(&self, channel_index: usize) -> Option<AudioBuf> {
        let reg = unsafe { self.register.get().as_ref() };
        let get_buffer = match reg.GetBuffer {
            None => return None,
            Some(f) => f,
        };
        let effective_channel_index = self.channel_offset + channel_index;
        let buf = unsafe { (get_buffer)(false, effective_channel_index as _) };
        if buf.is_null() {
            return None;
        }
        let buf = unsafe { AudioBuf::from_raw(buf, 1, self.block_props.block_length) };
        Some(buf)
    }
}
