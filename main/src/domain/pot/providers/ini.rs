use crate::domain::pot::provider_database::{
    Database, ProviderContext, SortablePresetId, FIL_CONTENT_TYPE_FACTORY, FIL_FAVORITE_FAVORITE,
};
use crate::domain::pot::{
    BuildInput, FilterItemCollections, FilterItemId, InnerPresetId, InternalPresetKind, PluginId,
    Preset, PresetCommon, PresetKind, SimplePluginKind,
};

use crate::domain::pot::plugins::{PluginDatabase, PluginKind};
use ini::Ini;
use realearn_api::persistence::PotFilterItemKind;
use std::error::Error;
use std::path::PathBuf;
use walkdir::WalkDir;

/// TODO-high CONTINUE Also scan JS presets!
pub struct IniDatabase {
    root_dir: PathBuf,
    entries: Vec<PresetEntry>,
    plugin_db: PluginDatabase,
}

impl IniDatabase {
    pub fn open(root_dir: PathBuf) -> Result<Self, Box<dyn Error>> {
        if !root_dir.try_exists()? {
            return Err("path to presets root directory doesn't exist".into());
        }
        let db = Self {
            entries: Default::default(),
            root_dir,
            plugin_db: Default::default(),
        };
        Ok(db)
    }
}

struct PresetEntry {
    preset_name: String,
    plugin_identifier: String,
    plugin_id: Option<PluginId>,
}

impl Database for IniDatabase {
    fn filter_item_name(&self) -> String {
        "FX presets".to_string()
    }

    fn refresh(&mut self, context: &ProviderContext) -> Result<(), Box<dyn Error>> {
        self.plugin_db = context.plugin_db.clone();
        let file_name_regex = regex!(r#"(?i)(.*?)-(.*).ini"#);
        self.entries = WalkDir::new(&self.root_dir)
            .max_depth(1)
            .follow_links(true)
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().is_file() {
                    return None;
                }
                let file_name = entry.file_name().to_str()?;
                // Example file names:
                // - vst-Zebra2.ini
                // - vst3-FM8-1168312232-builtin.ini
                // - vst-TDR Nova-builtin.ini
                // - vst-reacomp.ini
                // - vst3-Massive.ini
                // - clap-org_surge-synth-team_surge-xt.ini
                let captures = file_name_regex.captures(file_name)?;
                let simple_plugin_kind = match captures.get(1)?.as_str() {
                    "vst" => SimplePluginKind::Vst2,
                    "vst3" => SimplePluginKind::Vst3,
                    "clap" => SimplePluginKind::Clap,
                    _ => return None,
                };
                let plugin_identifier = captures.get(2)?.as_str();
                if plugin_identifier.ends_with("-builtin") {
                    return None;
                }
                let (main_plugin_identifier, shell_qualifier) =
                    match plugin_identifier.rsplit_once('-') {
                        // Example: vst3-Zebra2-959560201.ini
                        // (interpret the number behind the dash as shell qualifier)
                        Some((left, right))
                            if right.len() >= 5 && right.chars().all(|ch| ch.is_digit(10)) =>
                        {
                            (left, Some(right))
                        }
                        // Examples: "vst-Tritik-Irid.ini", "vst-Zebra2.ini"
                        _ => (plugin_identifier, None),
                    };
                let plugin = context.plugin_db.plugins().find(|p| {
                    if p.common.id.simple_kind() != simple_plugin_kind {
                        return false;
                    }
                    match &p.kind {
                        PluginKind::Vst(k) => {
                            let unsafe_char_regex = regex!(r#"[^a-zA-Z0-9_]"#);
                            let safe_main_plugin_identifier =
                                unsafe_char_regex.replace(main_plugin_identifier, "_");
                            let file_name_prefix = format!("{safe_main_plugin_identifier}.");
                            if !k.safe_file_name.starts_with(&file_name_prefix) {
                                return false;
                            }
                            let plugin_shell_qualifier =
                                k.shell_qualifier.as_ref().map(|q| q.as_str());
                            if shell_qualifier != plugin_shell_qualifier {
                                return false;
                            }
                            true
                        }
                        PluginKind::Clap(k) => {
                            let safe_plugin_id = k.id.replace('.', "_");
                            if plugin_identifier != &safe_plugin_id {
                                return false;
                            }
                            true
                        }
                    }
                });
                let ini_file = Ini::load_from_file(entry.path()).ok()?;
                let general_section = ini_file.section(Some("General"))?;
                let nb_presets = general_section.get("NbPresets")?;
                let preset_count: u32 = nb_presets.parse().ok()?;
                let plugin_identifier = plugin_identifier.to_string();
                let iter = (0..preset_count).filter_map(move |i| {
                    let section_name = format!("Preset{i}");
                    let section = ini_file.section(Some(section_name))?;
                    let name = section.get("Name")?;
                    let preset_entry = PresetEntry {
                        preset_name: name.to_string(),
                        plugin_identifier: plugin_identifier.clone(),
                        plugin_id: plugin.map(|p| p.common.id),
                    };
                    Some(preset_entry)
                });
                Some(iter)
            })
            .flatten()
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
                IsUser => filter != Some(FilterItemId(Some(FIL_CONTENT_TYPE_FACTORY))),
                IsFavorite => filter != Some(FilterItemId(Some(FIL_FAVORITE_FAVORITE))),
                ProductKind | Bank | SubBank | Category | SubCategory | Mode => {
                    matches!(filter, None | Some(FilterItemId::NONE))
                }
                _ => true,
            };
            if !matches {
                return Ok(vec![]);
            }
        }
        let preset_ids = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, preset_entry)| {
                if !input.search_evaluator.matches(&preset_entry.preset_name) {
                    return None;
                }
                let preset_id = SortablePresetId::new(i as _, preset_entry.preset_name.clone());
                Some(preset_id)
            })
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset> {
        let preset_entry = self.entries.get(preset_id.0 as usize)?;
        let plugin = preset_entry
            .plugin_id
            .as_ref()
            .and_then(|pid| self.plugin_db.find_plugin_by_id(pid));
        let preset = Preset {
            common: PresetCommon {
                favorite_id: "".to_string(),
                name: preset_entry.preset_name.clone(),
                product_name: {
                    let name = match plugin {
                        None => preset_entry.plugin_identifier.to_string(),
                        Some(p) => p.common.to_string(),
                    };
                    Some(name)
                },
            },
            kind: PresetKind::Internal(InternalPresetKind {
                plugin_id: preset_entry.plugin_id,
            }),
        };
        Some(preset)
    }

    fn find_preview_by_preset_id(&self, _preset_id: InnerPresetId) -> Option<PathBuf> {
        None
    }
}
