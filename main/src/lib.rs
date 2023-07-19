#![recursion_limit = "512"]
extern crate vst;

use helgoboss_allocator::{AsyncDeallocationIntegration, Deallocate, HelgobossAllocator};
use reaper_high::Reaper;
use std::alloc::{GlobalAlloc, Layout, System};

#[macro_use]
mod base;
mod application;
mod domain;
mod infrastructure;

pub struct RealearnAllocatorIntegration;

impl AsyncDeallocationIntegration for RealearnAllocatorIntegration {
    fn offload_deallocation(&self) -> bool {
        Reaper::get().medium_reaper().is_in_real_time_audio()
    }
}

pub struct RealearnDeallocator;

impl Deallocate for RealearnDeallocator {
    fn deallocate(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
pub static GLOBAL_ALLOCATOR: HelgobossAllocator<RealearnAllocatorIntegration, RealearnDeallocator> =
    HelgobossAllocator::new(RealearnDeallocator);
