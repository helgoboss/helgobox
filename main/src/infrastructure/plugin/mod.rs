mod api_impl;
mod backbone_shell;
mod debug_util;
mod helgobox_plugin_editor;
mod tracing_util;
pub use backbone_shell::*;
mod helgobox_plugin;
mod instance_parameter_container;
mod instance_shell;
pub use instance_shell::*;
mod auto_units;
pub use auto_units::*;
mod unit_shell;

pub use instance_parameter_container::*;

#[allow(unused)]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

vst::plugin_main!(helgobox_plugin::HelgoboxPlugin);
