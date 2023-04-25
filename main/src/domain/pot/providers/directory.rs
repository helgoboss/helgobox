use crate::domain::pot::provider_database::{
    Database, ProviderContext, SortablePresetId, CONTENT_TYPE_FACTORY_ID, FAVORITE_FAVORITE_ID,
};
use crate::domain::pot::{
    BuildInput, Fil, FiledBasedPresetKind, FilterItemCollections, FilterItemId, InnerPresetId,
    Preset, PresetCommon, PresetKind,
};

use realearn_api::persistence::PotFilterItemKind;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;
use walkdir::WalkDir;
use wildmatch::WildMatch;

pub struct DirectoryDatabase {
    root_dir: PathBuf,
    valid_extensions: HashSet<&'static OsStr>,
    name: &'static str,
    publish_relative_path: bool,
    entries: HashMap<InnerPresetId, PresetEntry>,
}

pub struct DirectoryDbConfig {
    pub root_dir: PathBuf,
    pub valid_extensions: &'static [&'static str],
    pub name: &'static str,
    pub publish_relative_path: bool,
}

impl DirectoryDatabase {
    pub fn open(config: DirectoryDbConfig) -> Result<Self, Box<dyn Error>> {
        if !config.root_dir.try_exists()? {
            return Err("path to root directory doesn't exist".into());
        }
        let db = Self {
            name: config.name,
            entries: Default::default(),
            root_dir: config.root_dir,
            valid_extensions: config
                .valid_extensions
                .into_iter()
                .map(OsStr::new)
                .collect(),
            publish_relative_path: config.publish_relative_path,
        };
        Ok(db)
    }
}

struct PresetEntry {
    preset_name: String,
    relative_path: String,
}

impl Database for DirectoryDatabase {
    fn filter_item_name(&self) -> String {
        self.name.to_string()
    }

    fn refresh(&mut self, _: &ProviderContext) -> Result<(), Box<dyn Error>> {
        let preset_entries = WalkDir::new(&self.root_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().is_file() {
                    return None;
                }
                let extension = entry.path().extension()?;
                if !self.valid_extensions.contains(extension) {
                    return None;
                }
                let relative_path = entry.path().strip_prefix(&self.root_dir).ok()?;
                // Immediately exclude relative paths that can't be represented as valid UTF-8.
                // Otherwise we will potentially open a can of worms (regarding persistence etc.).
                let preset_entry = PresetEntry {
                    preset_name: entry.path().file_stem()?.to_str()?.to_string(),
                    relative_path: relative_path.to_str()?.to_string(),
                };
                Some(preset_entry)
            });
        self.entries = preset_entries
            .enumerate()
            .map(|(i, entry)| (InnerPresetId(i as _), entry))
            .collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &BuildInput,
    ) -> Result<FilterItemCollections, Box<dyn Error>> {
        Ok(FilterItemCollections::empty())
    }

    fn query_presets(&self, input: &BuildInput) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        for (kind, filter) in input.filter_settings.iter() {
            use PotFilterItemKind::*;
            let matches = match kind {
                NksContentType => {
                    filter != Some(FilterItemId(Some(Fil::Komplete(CONTENT_TYPE_FACTORY_ID))))
                }
                NksFavorite => {
                    filter != Some(FilterItemId(Some(Fil::Komplete(FAVORITE_FAVORITE_ID))))
                }
                NksProductType | NksBank | NksSubBank | NksCategory | NksSubCategory | NksMode => {
                    matches!(filter, None | Some(FilterItemId::NONE))
                }
                _ => true,
            };
            if !matches {
                return Ok(vec![]);
            }
        }
        let lowercase_search_expression = input.search_expression.trim().to_lowercase();
        let wild_match = WildMatch::new(&lowercase_search_expression);
        let preset_ids = self
            .entries
            .iter()
            .filter_map(|(id, preset_entry)| {
                let matches = if lowercase_search_expression.is_empty() {
                    true
                } else {
                    let lowercase_preset_name = preset_entry.preset_name.to_lowercase();
                    if input.use_wildcard_search {
                        wild_match.matches(&lowercase_preset_name)
                    } else {
                        lowercase_preset_name.contains(&lowercase_search_expression)
                    }
                };
                if matches {
                    Some(SortablePresetId::new(*id, preset_entry.preset_name.clone()))
                } else {
                    None
                }
            })
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset> {
        let preset_entry = self.entries.get(&preset_id)?;
        let relative_path = PathBuf::from(&preset_entry.relative_path);
        let preset = Preset {
            common: PresetCommon {
                favorite_id: preset_entry.relative_path.clone(),
                name: preset_entry.preset_name.clone(),
                product_name: None,
            },
            kind: PresetKind::FileBased(FiledBasedPresetKind {
                file_ext: relative_path
                    .extension()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                path: if self.publish_relative_path {
                    relative_path
                } else {
                    self.root_dir.join(relative_path)
                },
            }),
        };
        Some(preset)
    }

    fn find_preview_by_preset_id(&self, _preset_id: InnerPresetId) -> Option<PathBuf> {
        None
    }
}
