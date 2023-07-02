use crate::rt::audio_hook::FxInputClipRecordTask;
use crate::rt::supplier::WriteAudioRequest;
use crate::rt::{AudioBuf, BasicAudioRequestProps};

#[derive(Debug, Default)]
pub struct ClipEngineFxHook {
    clip_record_task: Option<FxInputClipRecordTask>,
}

impl ClipEngineFxHook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_clip_recording(&mut self, task: FxInputClipRecordTask) {
        debug!("Real-time processor received clip record task");
        self.clip_record_task = Some(task);
    }

    pub fn process_clip_record_task(
        &mut self,
        inputs: &impl ChannelInputs,
        block_props: BasicAudioRequestProps,
    ) {
        if let Some(t) = &mut self.clip_record_task {
            if !process_clip_record_task(t, inputs, block_props) {
                debug!("Clearing clip record task from real-time processor");
                self.clip_record_task = None;
            }
        }
    }
}

pub trait ChannelInputs {
    fn channel_count(&self) -> usize;
    fn get_channel_data(&self, channel_index: usize) -> &[f64];
}

/// Returns whether task still relevant.
fn process_clip_record_task(
    record_task: &mut FxInputClipRecordTask,
    inputs: &impl ChannelInputs,
    block_props: BasicAudioRequestProps,
) -> bool {
    let column_source = match record_task.destination.column_source.upgrade() {
        None => return false,
        Some(s) => s,
    };
    let mut src = column_source.lock();
    if !src.recording_poll(record_task.destination.slot_index, block_props) {
        return false;
    }
    let channel_offset = record_task.input.channel_offset().unwrap();
    let write_audio_request =
        RealTimeProcessorWriteAudioRequest::new(inputs, block_props, channel_offset as _);
    src.write_clip_audio(record_task.destination.slot_index, write_audio_request)
        .unwrap();
    true
}

#[derive(Copy, Clone)]
struct RealTimeProcessorWriteAudioRequest<'a, I> {
    channel_offset: usize,
    inputs: &'a I,
    block_props: BasicAudioRequestProps,
}

impl<'a, I: ChannelInputs> RealTimeProcessorWriteAudioRequest<'a, I> {
    pub fn new(inputs: &'a I, block_props: BasicAudioRequestProps, channel_offset: usize) -> Self {
        Self {
            channel_offset,
            inputs,
            block_props,
        }
    }
}

impl<'a, I: ChannelInputs> WriteAudioRequest for RealTimeProcessorWriteAudioRequest<'a, I> {
    fn audio_request_props(&self) -> BasicAudioRequestProps {
        self.block_props
    }

    fn get_channel_buffer(&self, channel_index: usize) -> Option<AudioBuf> {
        let effective_channel_index = self.channel_offset + channel_index;
        if effective_channel_index >= self.inputs.channel_count() {
            return None;
        }
        let slice = self.inputs.get_channel_data(effective_channel_index);
        AudioBuf::from_slice(slice, 1, self.block_props.block_length).ok()
    }
}
