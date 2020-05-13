mod realearn_editor;
mod realearn_plugin;
use realearn_editor::*;

vst::plugin_main!(realearn_plugin::RealearnPlugin);
