mod api_impl;
mod debug_util;
mod instance_editor;
mod tracing_util;
use instance_editor::*;
mod backbone_shell;
pub use backbone_shell::*;
mod instance_parameters;
mod instance_shell;
pub use instance_parameters::*;

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

vst::plugin_main!(instance_shell::InstanceShell);
