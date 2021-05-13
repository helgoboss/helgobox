#![feature(option_result_contains, trait_alias)]
#![recursion_limit = "512"]
#[macro_use]
mod core;
mod application;
mod domain;
mod infrastructure;

#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
