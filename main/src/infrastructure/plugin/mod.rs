mod debug_util;
mod realearn_editor;
mod realearn_plugin;
mod realearn_plugin_parameters;
use realearn_editor::*;

vst::plugin_main!(realearn_plugin::RealearnPlugin);
