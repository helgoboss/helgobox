mod api_impl;
mod debug_util;
mod tracing_util;
mod unit_panel;
use unit_panel::*;
mod backbone_shell;
pub use backbone_shell::*;
mod instance_shell;
mod unit_parameter_container;
mod unit_shell;
mod unit_vst_plugin;
pub use unit_parameter_container::*;

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

vst::plugin_main!(unit_vst_plugin::UnitVstPlugin);
