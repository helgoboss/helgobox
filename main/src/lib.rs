#![feature(option_result_contains, trait_alias)]
use assert_no_alloc::*;

#[macro_use]
mod core;
mod application;
mod domain;
mod infrastructure;

#[cfg(debug_assertions)]
#[global_allocator]
static A: AllocDisabler = AllocDisabler;
