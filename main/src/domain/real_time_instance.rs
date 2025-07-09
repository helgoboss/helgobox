use crate::domain::AudioBlockProps;
use std::sync::{Arc, Mutex, Weak};

pub type SharedRealTimeInstance = Arc<Mutex<RealTimeInstance>>;
pub type WeakRealTimeInstance = Weak<Mutex<RealTimeInstance>>;

const NORMAL_BULK_SIZE: usize = 100;

#[derive(Debug)]
pub struct RealTimeInstance {
    task_receiver: crossbeam_channel::Receiver<RealTimeInstanceTask>,
    #[cfg(feature = "playtime")]
    playtime: PlaytimeRtInstance,
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
struct PlaytimeRtInstance {
    clip_matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
    clip_engine_fx_hook: playtime_clip_engine::rt::fx_hook::PlaytimeFxHook,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum RealTimeInstanceTask {
    #[cfg(feature = "playtime")]
    SetClipMatrix {
        matrix: Option<playtime_clip_engine::rt::WeakRtMatrix>,
    },
    #[cfg(feature = "playtime")]
    PlaytimeClipEngineCommand(playtime_clip_engine::rt::fx_hook::PlaytimeFxHookCommand),
}

impl RealTimeInstance {
    pub fn new(task_receiver: crossbeam_channel::Receiver<RealTimeInstanceTask>) -> Self {
        Self {
            task_receiver,
            #[cfg(feature = "playtime")]
            playtime: PlaytimeRtInstance {
                clip_matrix: None,
                clip_engine_fx_hook: playtime_clip_engine::rt::fx_hook::PlaytimeFxHook::new(),
            },
        }
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix(&self) -> Option<&playtime_clip_engine::rt::WeakRtMatrix> {
        self.playtime.clip_matrix.as_ref()
    }

    /// To be called from audio hook when `post == false`.
    pub fn pre_poll(&mut self, block_props: AudioBlockProps) {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = block_props;
        }
        #[cfg(feature = "playtime")]
        {
            if let Some(clip_matrix) = self.playtime.clip_matrix.as_ref().and_then(|m| m.upgrade())
            {
                clip_matrix.lock().pre_poll(block_props.to_playtime());
            }
        }
        #[allow(clippy::never_loop)]
        for task in self.task_receiver.try_iter().take(NORMAL_BULK_SIZE) {
            match task {
                #[cfg(feature = "playtime")]
                RealTimeInstanceTask::SetClipMatrix { matrix } => {
                    self.playtime.clip_matrix = matrix;
                }
                #[cfg(feature = "playtime")]
                RealTimeInstanceTask::PlaytimeClipEngineCommand(command) => {
                    let _ = self
                        .playtime
                        .clip_engine_fx_hook
                        .process_command(command, block_props.to_playtime());
                }
            }
        }
    }

    /// To be called from audio hook when `post == true`.
    pub fn post_poll(&mut self) {
        #[cfg(feature = "playtime")]
        {
            if let Some(clip_matrix) = self.playtime.clip_matrix.as_ref().and_then(|m| m.upgrade())
            {
                clip_matrix.lock().post_poll();
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
        self.playtime
            .clip_engine_fx_hook
            .poll(&inputs, block_props.to_playtime());
    }
}

#[cfg(feature = "playtime")]
struct VstChannelInputs<'a>(vst::buffer::Inputs<'a, f64>);

#[cfg(feature = "playtime")]
impl playtime_clip_engine::rt::fx_hook::ChannelInputs for VstChannelInputs<'_> {
    fn channel_count(&self) -> usize {
        self.0.len()
    }

    fn get_channel_data(&self, channel_index: usize) -> &[f64] {
        self.0.get(channel_index)
    }
}
