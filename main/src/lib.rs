#![recursion_limit = "512"]
extern crate vst;

#[macro_use]
mod base;
mod application;
mod domain;
mod infrastructure;

// TODO-high CONTINUE Activate again!!!
// #[cfg(debug_assertions)]
// #[global_allocator]
// static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
