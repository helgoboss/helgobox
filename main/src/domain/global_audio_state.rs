use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use atomic::Atomic;
use reaper_medium::Hz;
use static_assertions::const_assert;

use crate::domain::AudioBlockProps;

pub static GLOBAL_AUDIO_STATE: GlobalAudioState = GlobalAudioState::new();

#[derive(Debug)]
pub struct GlobalAudioState {
    block_count: AtomicU64,
    block_size: AtomicU32,
    sample_rate: Atomic<f64>,
}

impl Default for GlobalAudioState {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalAudioState {
    pub const fn new() -> Self {
        const_assert!(Atomic::<f64>::is_lock_free());
        Self {
            block_count: AtomicU64::new(0),
            block_size: AtomicU32::new(0),
            sample_rate: Atomic::new(1.0),
        }
    }

    /// Returns previous block count
    pub fn advance(&self, block_props: AudioBlockProps) -> u64 {
        let prev_block_count = self.block_count.fetch_add(1, Ordering::Relaxed);
        self.block_size
            .store(block_props.block_length as u32, Ordering::Relaxed);
        self.sample_rate
            .store(block_props.frame_rate.get(), Ordering::Relaxed);
        prev_block_count
    }

    pub fn load_block_count(&self) -> u64 {
        self.block_count.load(Ordering::Relaxed)
    }

    pub fn load_block_size(&self) -> u32 {
        self.block_size.load(Ordering::Relaxed)
    }

    pub fn load_sample_rate(&self) -> Hz {
        Hz::new_panic(self.sample_rate.load(Ordering::Relaxed))
    }
}
