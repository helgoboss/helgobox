pub mod bindings;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub mod debug_util;
