use c_str_macro::c_str;
use reaper_rs::high_level;
use reaper_rs::high_level::{setup_all_with_defaults, Reaper};
use reaper_rs::low_level;
use reaper_rs::low_level::ReaperPluginContext;
use reaper_rs::medium_level;
use vst::editor::Editor;
use vst::plugin::{HostCallback, Info, Plugin};

use crate::editor::RealearnEditor;

#[derive(Default)]
pub struct Realearn {
    host: HostCallback,
}

impl Plugin for Realearn {
    fn new(host: HostCallback) -> Self {
        Self { host }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "realearn-rs".to_string(),
            unique_id: 2964,
            ..Default::default()
        }
    }

    fn init(&mut self) {
        let context = ReaperPluginContext::from_reaper_vst_plugin(self.host).unwrap();
        setup_all_with_defaults(context, "info@helgoboss.org");
        let reaper = Reaper::get();
        reaper.show_console_msg(c_str!("Loaded realearn-rs VST plugin\n"));
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        Some(Box::new(RealearnEditor::new()))
    }
}
