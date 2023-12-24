mod api_impl;
mod backbone_shell;
mod debug_util;
mod instance_editor;
mod tracing_util;
pub use backbone_shell::*;
mod instance_param_container;
mod instance_shell;
mod instance_vst_plugin;
mod unit_shell;
pub use instance_param_container::*;

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

vst::plugin_main!(instance_vst_plugin::InstanceVstPlugin);
