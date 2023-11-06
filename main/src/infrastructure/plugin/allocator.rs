use base::metrics_util::record_occurrence;
use helgoboss_allocator::{AsyncDeallocationIntegration, Deallocate, HelgobossAllocator};
use reaper_high::Reaper;
use std::alloc::{GlobalAlloc, Layout, System};

#[global_allocator]
pub static GLOBAL_ALLOCATOR: HelgobossAllocator<RealearnAllocatorIntegration, RealearnDeallocator> =
    HelgobossAllocator::new(RealearnDeallocator::without_metrics());

/// Allocator integration which defers deallocation when in real-time thread.
pub struct RealearnAllocatorIntegration {
    is_in_real_time_audio: IsInRealTimeAudio,
}

pub type IsInRealTimeAudio = extern "C" fn() -> ::std::os::raw::c_int;

impl RealearnAllocatorIntegration {
    pub fn new(is_in_real_time_audio: IsInRealTimeAudio) -> Self {
        Self {
            is_in_real_time_audio,
        }
    }
}

impl AsyncDeallocationIntegration for RealearnAllocatorIntegration {
    fn offload_deallocation(&self) -> bool {
        // Defer deallocation whenever we are in a real-time audio thread.
        (self.is_in_real_time_audio)() != 0
    }
}

pub struct RealearnDeallocator {
    metric_id: Option<&'static str>,
}

impl RealearnDeallocator {
    pub const fn without_metrics() -> Self {
        Self { metric_id: None }
    }

    pub const fn with_metrics(metric_id: &'static str) -> Self {
        Self {
            metric_id: Some(metric_id),
        }
    }
}

impl Deallocate for RealearnDeallocator {
    fn deallocate(&self, ptr: *mut u8, layout: Layout) {
        if let Some(id) = self.metric_id {
            record_occurrence(id);
        }
        unsafe { System.dealloc(ptr, layout) };
    }
}
