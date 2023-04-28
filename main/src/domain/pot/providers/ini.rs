use crate::domain::pot::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
};
use crate::domain::pot::{
    BuildInput, Filters, InnerPresetId, InternalPresetKind, PotFilterExcludeList, Preset,
    PresetCommon, PresetKind, SimplePluginKind,
};
use std::borrow::Cow;

use crate::domain::pot::plugins::{PluginCore, PluginKind};
use either::Either;
use enumset::{enum_set, EnumSet};
use ini::Ini;
use itertools::Itertools;
use realearn_api::persistence::PotFilterKind;
use std::error::Error;
use std::iter;
use std::path::PathBuf;
use walkdir::WalkDir;

/// TODO-high CONTINUE Also scan JS presets!
pub struct IniDatabase {
    root_dir: PathBuf,
    entries: Vec<PresetEntry>,
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

    fn query_presets_internal<'a>(
        &'a self,
        filters: &'a Filters,
        excludes: &'a PotFilterExcludeList,
    ) -> impl Iterator<Item = (usize, &PresetEntry)> + 'a {
        let matches = !filters.wants_factory_presets_only()
            && !filters.wants_favorites_only()
            && !filters.any_filter_below_is_set_to_concrete_value(PotFilterKind::Bank);
        if !matches {
            return Either::Left(iter::empty());
        }
        let iter = self.entries.iter().enumerate().filter(|(_, e)| {
            if let Some(core) = &e.plugin {
                filters.plugin_core_matches(core, excludes)
            } else {
                false
            }
        });
        Either::Right(iter)
    }
}

struct PresetEntry {
    preset_name: String,
    plugin_identifier: String,
    plugin: Option<PluginCore>,
}

impl Database for IniDatabase {
    fn name(&self) -> Cow<str> {
        "FX presets".into()
    }

    fn description(&self) -> Cow<str> {
        "All FX presets that you saved via \"Save preset...\" in REAPER's FX window".into()
    }

    fn supported_advanced_filter_kinds(&self) -> EnumSet<PotFilterKind> {
        enum_set!(PotFilterKind::Bank)
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
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
                let plugin = ctx.plugin_db.plugins().find(|p| {
                    if p.common.core.id.simple_kind() != simple_plugin_kind {
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
                        plugin: plugin.map(|p| p.common.core.clone()),
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
        _: &ProviderContext,
        input: &BuildInput,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        let mut filter_settings = input.filters;
        filter_settings.clear_this_and_dependent_filters(PotFilterKind::Bank);
        let product_items = self
            .query_presets_internal(&filter_settings, &input.filter_exclude_list)
            .filter_map(|(_, entry)| Some(entry.plugin.as_ref()?.product_id))
            .unique()
            .map(InnerFilterItem::Product)
            .collect();
        let mut collections = InnerFilterItemCollections::empty();
        collections.set(PotFilterKind::Bank, product_items);
        Ok(collections)
    }

    fn query_presets(
        &self,
        _: &ProviderContext,
        input: &BuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let preset_ids = self
            .query_presets_internal(&input.filters, &input.filter_exclude_list)
            .filter(|(_, entry)| input.search_evaluator.matches(&entry.preset_name))
            .map(|(i, entry)| SortablePresetId::new(i as _, entry.preset_name.clone()))
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, ctx: &ProviderContext, preset_id: InnerPresetId) -> Option<Preset> {
        let preset_entry = self.entries.get(preset_id.0 as usize)?;
        let plugin = preset_entry
            .plugin
            .as_ref()
            .and_then(|entry| ctx.plugin_db.find_plugin_by_id(&entry.id));
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
                plugin_id: preset_entry.plugin.as_ref().map(|p| p.id),
            }),
        };
        Some(preset)
    }

    fn find_preview_by_preset_id(
        &self,
        _: &ProviderContext,
        _preset_id: InnerPresetId,
    ) -> Option<PathBuf> {
        None
    }
}
