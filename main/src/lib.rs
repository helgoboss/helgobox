#![feature(option_result_contains, trait_alias)]
#[macro_use]
mod core;
mod application;
mod domain;
mod infrastructure;

#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
