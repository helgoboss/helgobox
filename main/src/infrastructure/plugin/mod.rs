mod debug_util;
mod realearn_editor;
use realearn_editor::*;
mod app;
pub use app::*;
mod realearn_plugin;
mod realearn_plugin_parameters;

vst::plugin_main!(realearn_plugin::RealearnPlugin);
