use crate::domain::pot::provider_database::{
    Database, ProviderContext, SortablePresetId, CONTENT_TYPE_FACTORY_ID, FAVORITE_FAVORITE_ID,
};
use crate::domain::pot::{
    BuildInput, FilterItemCollections, FilterItemId, InnerPresetId, InternalPresetKind, PluginId,
    Preset, PresetCommon, PresetKind,
};

use crate::domain::pot::plugins::PluginKind;
use ini::Ini;
use realearn_api::persistence::PotFilterItemKind;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use walkdir::WalkDir;
use wildmatch::WildMatch;

pub struct IniDatabase {
    root_dir: PathBuf,
    entries: HashMap<InnerPresetId, PresetEntry>,
}

impl IniDatabase {
    pub fn open(root_dir: PathBuf) -> Result<Self, Box<dyn Error>> {
        if !root_dir.try_exists()? {
            return Err("path to presets root directory doesn't exist".into());
        }
        let db = Self {
            entries: Default::default(),
            root_dir,
        };
        Ok(db)
    }
}

struct PresetEntry {
    plugin_id: PluginId,
    preset_name: String,
}

impl Database for IniDatabase {
    fn filter_item_name(&self) -> String {
        "FX presets".to_string()
    }

    fn refresh(&mut self, context: &ProviderContext) -> Result<(), Box<dyn Error>> {
        let file_name_regex = regex!(r#"(?i)(.*?)-(.*).ini"#);
        let preset_entries = WalkDir::new(&self.root_dir)
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
                let captures = file_name_regex.captures(file_name)?;
                let plugin_type = captures.get(1)?.as_str();
                let plugin_identifier = captures.get(2)?.as_str();
                if plugin_identifier.ends_with("-builtin") {
                    return None;
                }
                let (plugin_name, shell_qualifier) = match plugin_identifier.rsplit_once('-') {
                    // Example: vst3-Zebra2-959560201.ini
                    // (interpret the number behind the dash as shell qualifier)
                    Some((left, right))
                        if right.len() >= 5 && right.chars().any(|ch| ch.is_digit(10)) =>
                    {
                        (left, Some(right))
                    }
                    // Examples: "vst-Tritik-Irid.ini", "vst-Zebra2.ini"
                    _ => (plugin_identifier, None),
                };
                let plugin = context.plugins.iter().find(|p| {
                    let unsafe_char_regex = regex!(r#"[^a-zA-Z0-9_]"#);
                    let safe_plugin_name = unsafe_char_regex.replace(plugin_name, "_");
                    let file_name_prefix = format!("{safe_plugin_name}.");
                    if !p.safe_file_name.starts_with(&file_name_prefix) {
                        return false;
                    }
                    let plugin_shell_qualifier = p.shell_qualifier.as_ref().map(|q| q.as_str());
                    if shell_qualifier != plugin_shell_qualifier {
                        return false;
                    }
                    match plugin_type {
                        "vst" => matches!(p.kind, PluginKind::Vst2 { .. }),
                        "vst3" => matches!(p.kind, PluginKind::Vst3 { .. }),
                        _ => false,
                    }
                })?;
                let ini_file = Ini::load_from_file(entry.path()).ok()?;
                let general_section = ini_file.section(Some("General"))?;
                let nb_presets = general_section.get("NbPresets")?;
                let preset_count: u32 = nb_presets.parse().ok()?;
                let iter = (0..preset_count).filter_map(move |i| {
                    let section_name = format!("Preset{i}");
                    let section = ini_file.section(Some(section_name))?;
                    let name = section.get("Name")?;
                    let preset_entry = PresetEntry {
                        plugin_id: plugin.kind.plugin_id().ok()?,
                        preset_name: name.to_string(),
                    };
                    Some(preset_entry)
                });
                Some(iter)
            })
            .flatten();
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
                NksContentType => filter != Some(FilterItemId(Some(CONTENT_TYPE_FACTORY_ID))),
                NksFavorite => filter != Some(FilterItemId(Some(FAVORITE_FAVORITE_ID))),
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
        let preset = Preset {
            common: PresetCommon {
                favorite_id: "".to_string(),
                name: preset_entry.preset_name.clone(),
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
