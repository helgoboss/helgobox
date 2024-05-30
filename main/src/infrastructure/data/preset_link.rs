use crate::application::{FxId, FxPresetLinkConfig, PresetLinkManager, PresetLinkMutator};
use camino::Utf8PathBuf;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

pub type SharedPresetLinkManager = Rc<RefCell<FileBasedPresetLinkManager>>;

#[derive(Debug)]
pub struct FileBasedPresetLinkManager {
    auto_load_configs_dir_path: Utf8PathBuf,
    config: FxPresetLinkConfig,
}

impl FileBasedPresetLinkManager {
    pub fn new(auto_load_configs_dir_path: Utf8PathBuf) -> FileBasedPresetLinkManager {
        FileBasedPresetLinkManager {
            auto_load_configs_dir_path,
            config: Default::default(),
        }
    }

    pub fn config(&self) -> &FxPresetLinkConfig {
        &self.config
    }

    fn fx_config_file_path(&self) -> Utf8PathBuf {
        self.auto_load_configs_dir_path.join("fx.json")
    }

    pub fn load_preset_links_from_disk(&mut self) -> Result<(), String> {
        let json = fs::read_to_string(self.fx_config_file_path())
            .map_err(|_| "couldn't read FX preset link config file".to_string())?;
        self.config = serde_json::from_str(&json)
            .map_err(|e| format!("FX preset link config file isn't valid. Details:\n\n{e}"))?;
        Ok(())
    }

    fn save_fx_config(&self) -> Result<(), String> {
        fs::create_dir_all(&self.auto_load_configs_dir_path)
            .map_err(|_| "couldn't create auto-load-configs directory")?;
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|_| "couldn't serialize FX preset link config")?;
        fs::write(self.fx_config_file_path(), json)
            .map_err(|_| "couldn't write FX preset link config file")?;
        Ok(())
    }
}

impl PresetLinkManager for SharedPresetLinkManager {
    fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String> {
        self.borrow().config().find_preset_linked_to_fx(fx_id)
    }
}

impl PresetLinkMutator for FileBasedPresetLinkManager {
    fn update_fx_id(&mut self, old_fx_id: FxId, new_fx_id: FxId) {
        self.config.update_fx_id(old_fx_id, new_fx_id);
        self.save_fx_config().unwrap();
    }

    fn remove_link(&mut self, fx_id: &FxId) {
        self.config.remove_link(fx_id);
        self.save_fx_config().unwrap();
    }

    fn link_preset_to_fx(&mut self, preset_id: String, fx_id: FxId) {
        self.config.link_preset_to_fx(preset_id, fx_id);
        self.save_fx_config().unwrap();
    }
}
