use crate::application::{FxId, PresetLinkManager};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

pub type SharedPresetLinkManager = Rc<RefCell<FileBasedPresetLinkManager>>;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FxPresetLinkConfig {
    links: Vec<FxPresetLink>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxPresetLink {
    #[serde(rename = "fx")]
    pub fx_id: FxId,
    #[serde(rename = "presetId")]
    pub preset_id: String,
}

#[derive(Debug)]
pub struct FileBasedPresetLinkManager {
    auto_load_configs_dir_path: PathBuf,
    config: FxPresetLinkConfig,
}

impl FileBasedPresetLinkManager {
    pub fn new(auto_load_configs_dir_path: PathBuf) -> FileBasedPresetLinkManager {
        let mut manager = FileBasedPresetLinkManager {
            auto_load_configs_dir_path,
            config: Default::default(),
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
        self.config = serde_json::from_str(&json)
            .map_err(|e| format!("FX preset link config file isn't valid. Details:\n\n{}", e))?;
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

    pub fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String> {
        // Let the links with preset name have precedence.
        find_match(
            self.config
                .links
                .iter()
                .filter(|l| l.fx_id.has_preset_name()),
            fx_id,
        )
        .or_else(|| {
            find_match(
                self.config
                    .links
                    .iter()
                    .filter(|l| !l.fx_id.has_preset_name()),
                fx_id,
            )
        })
    }

    pub fn links(&self) -> impl Iterator<Item = &FxPresetLink> + ExactSizeIterator + '_ {
        self.config.links.iter()
    }

    pub fn update_fx_id(&mut self, old_fx_id: FxId, new_fx_id: FxId) {
        for link in &mut self.config.links {
            if link.fx_id == old_fx_id {
                link.fx_id = new_fx_id;
                return;
            }
        }
        self.save_fx_config().unwrap();
    }

    pub fn remove_link(&mut self, fx_id: &FxId) {
        self.config.links.retain(|l| &l.fx_id != fx_id);
        self.save_fx_config().unwrap();
    }

    pub fn link_preset_to_fx(&mut self, preset_id: String, fx_id: FxId) {
        let link = FxPresetLink { fx_id, preset_id };
        if let Some(l) = self.config.links.iter_mut().find(|l| l.fx_id == link.fx_id) {
            *l = link;
        } else {
            self.config.links.push(link);
        }
        self.save_fx_config().unwrap();
    }
}

impl PresetLinkManager for SharedPresetLinkManager {
    fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String> {
        self.borrow().find_preset_linked_to_fx(fx_id)
    }
}

fn find_match<'a>(
    mut links: impl Iterator<Item = &'a FxPresetLink>,
    fx_id: &FxId,
) -> Option<String> {
    links.find_map(|link| {
        if fx_id.matches(&link.fx_id) {
            Some(link.preset_id.clone())
        } else {
            None
        }
    })
}
