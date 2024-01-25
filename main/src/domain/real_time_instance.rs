use crate::domain::AudioBlockProps;
use std::sync::{Arc, Mutex, Weak};

pub type SharedRealTimeInstance = Arc<Mutex<RealTimeInstance>>;
pub type WeakRealTimeInstance = Weak<Mutex<RealTimeInstance>>;

const NORMAL_BULK_SIZE: usize = 100;

#[derive(Debug)]
pub struct RealTimeInstance {
    task_receiver: crossbeam_channel::Receiver<RealTimeInstanceTask>,
    #[cfg(feature = "playtime")]
    clip_matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
    #[cfg(feature = "playtime")]
    clip_matrix_is_owned: bool,
    #[cfg(feature = "playtime")]
    clip_engine_fx_hook: playtime_clip_engine::rt::fx_hook::ClipEngineFxHook,
}

#[derive(Debug)]
pub enum RealTimeInstanceTask {
    #[cfg(feature = "playtime")]
    SetClipMatrix {
        is_owned: bool,
        matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
    },
    #[cfg(feature = "playtime")]
    PlaytimeClipEngineCommand(playtime_clip_engine::rt::fx_hook::ClipEngineFxHookCommand),
}

impl RealTimeInstance {
    pub fn new(task_receiver: crossbeam_channel::Receiver<RealTimeInstanceTask>) -> Self {
        Self {
            task_receiver,
            #[cfg(feature = "playtime")]
            clip_matrix: None,
            #[cfg(feature = "playtime")]
            clip_matrix_is_owned: false,
            #[cfg(feature = "playtime")]
            clip_engine_fx_hook: playtime_clip_engine::rt::fx_hook::ClipEngineFxHook::new(),
        }
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix(&self) -> Option<&playtime_clip_engine::rt::WeakRtMatrix> {
        self.clip_matrix.as_ref()
    }

    pub fn poll(&mut self, block_props: AudioBlockProps) {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = block_props;
        }
        #[cfg(feature = "playtime")]
        {
            // Poll if this is the clip matrix of this instance. If we would do polling for a foreign
            // clip matrix as well, it would be polled more than once, which is unnecessary.
            if self.clip_matrix_is_owned {
                if let Some(clip_matrix) = self.clip_matrix.as_ref().and_then(|m| m.upgrade()) {
                    clip_matrix.lock().poll(block_props.to_playtime());
                }
            }
        }
        for task in self.task_receiver.try_iter().take(NORMAL_BULK_SIZE) {
            match task {
                #[cfg(feature = "playtime")]
                RealTimeInstanceTask::SetClipMatrix { is_owned, matrix } => {
                    self.clip_matrix_is_owned = is_owned;
                    self.clip_matrix = matrix;
                }
                #[cfg(feature = "playtime")]
                RealTimeInstanceTask::PlaytimeClipEngineCommand(command) => {
                    let _ = self
                        .clip_engine_fx_hook
                        .process_command(command, block_props.to_playtime());
                }
            }
        }
    }
    #[cfg(feature = "playtime")]
    pub fn run_from_vst(
        &mut self,
        buffer: &mut vst::buffer::AudioBuffer<f64>,
        block_props: AudioBlockProps,
    ) {
        let inputs = VstChannelInputs(buffer.split().0);
        self.clip_engine_fx_hook
            .poll(&inputs, block_props.to_playtime());
    }
}

#[cfg(feature = "playtime")]
struct VstChannelInputs<'a>(vst::buffer::Inputs<'a, f64>);

#[cfg(feature = "playtime")]
impl<'a> playtime_clip_engine::rt::fx_hook::ChannelInputs for VstChannelInputs<'a> {
    fn channel_count(&self) -> usize {
        self.0.len()
    }

    fn get_channel_data(&self, channel_index: usize) -> &[f64] {
        self.0.get(channel_index)
    }
}
