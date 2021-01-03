use crate::application::{FxId, PresetLinkManager};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

pub type SharedPresetLinkManager = Rc<RefCell<FileBasedPresetLinkManager>>;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxPresetLinkConfig {
    links: Vec<FxPresetLink>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxPresetLink {
    fx: FxDescriptor,
    preset_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxDescriptor {
    file_name: String,
}

#[derive(Debug)]
pub struct FileBasedPresetLinkManager {
    auto_load_configs_dir_path: PathBuf,
    preset_by_fx: HashMap<FxId, String>,
}

impl FileBasedPresetLinkManager {
    pub fn new(auto_load_configs_dir_path: PathBuf) -> FileBasedPresetLinkManager {
        let mut manager = FileBasedPresetLinkManager {
            auto_load_configs_dir_path,
            preset_by_fx: Default::default(),
        };
        let _ = manager.load_fx_config();
        manager
    }

    fn fx_config_file_path(&self) -> PathBuf {
        self.auto_load_configs_dir_path.join("fx.json")
    }

    fn load_fx_config(&mut self) -> Result<(), String> {
        let json = fs::read_to_string(&self.fx_config_file_path())
            .map_err(|_| "couldn't read FX preset link config file".to_string())?;
        let config: FxPresetLinkConfig = serde_json::from_str(&json)
            .map_err(|e| format!("FX preset link config file isn't valid. Details:\n\n{}", e))?;
        self.preset_by_fx = config
            .links
            .into_iter()
            .map(|link| (FxId::new(link.fx.file_name), link.preset_id))
            .collect();
        Ok(())
    }

    fn save_fx_config(&self) -> Result<(), String> {
        fs::create_dir_all(&self.auto_load_configs_dir_path)
            .map_err(|_| "couldn't create auto-load-configs directory")?;
        let mut fx_config = FxPresetLinkConfig {
            links: self
                .preset_by_fx
                .iter()
                .map(|(fx_id, preset_id)| FxPresetLink {
                    fx: FxDescriptor {
                        file_name: fx_id.file_name().to_string(),
                    },
                    preset_id: preset_id.clone(),
                })
                .collect(),
        };
        let json = serde_json::to_string_pretty(&fx_config)
            .map_err(|_| "couldn't serialize FX preset link config")?;
        fs::write(self.fx_config_file_path(), json)
            .map_err(|_| "couldn't write FX preset link config file")?;
        Ok(())
    }

    pub fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String> {
        self.preset_by_fx.get(fx_id).cloned()
    }

    pub fn find_fx_that_preset_is_linked_to(&self, preset_id: &str) -> Option<FxId> {
        self.preset_by_fx
            .iter()
            .find_map(|(fx_id, p_id)| if p_id == preset_id { Some(fx_id) } else { None })
            .cloned()
    }

    pub fn link_preset_to_fx(&mut self, preset_id: String, fx_id: FxId) {
        self.preset_by_fx.insert(fx_id, preset_id);
        self.save_fx_config();
    }

    pub fn unlink_preset_from_fx(&mut self, preset_id: &str) {
        self.preset_by_fx.retain(|fx_id, p_id| p_id != preset_id);
        self.save_fx_config();
    }
}

impl PresetLinkManager for SharedPresetLinkManager {
    fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String> {
        self.borrow().find_preset_linked_to_fx(fx_id)
    }
}
