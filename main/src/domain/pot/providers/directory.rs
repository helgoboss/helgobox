use crate::domain::pot::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
};
use crate::domain::pot::{
    BuildInput, FiledBasedPresetKind, Filters, InnerPresetId, PluginId, PotFilterExcludeList,
    Preset, PresetCommon, PresetKind,
};
use std::borrow::Cow;

use crate::domain::pot::plugins::{PluginCore, PluginDatabase};
use either::Either;
use enumset::{enum_set, EnumSet};
use itertools::Itertools;
use realearn_api::persistence::PotFilterKind;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{fs, iter};
use walkdir::WalkDir;

pub struct DirectoryDatabase {
    root_dir: PathBuf,
    valid_extensions: HashSet<&'static OsStr>,
    name: &'static str,
    description: &'static str,
    publish_relative_path: bool,
    entries: Vec<PresetEntry>,
}

pub struct DirectoryDbConfig {
    pub root_dir: PathBuf,
    pub valid_extensions: &'static [&'static str],
    pub name: &'static str,
    pub description: &'static str,
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
            description: config.description,
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
            e.plugin_cores
                .values()
                .any(|core| filters.plugin_core_matches(core, excludes))
        });
        Either::Right(iter)
    }
}

struct PresetEntry {
    preset_name: String,
    relative_path: String,
    plugin_cores: HashMap<PluginId, PluginCore>,
}

impl Database for DirectoryDatabase {
    fn name(&self) -> Cow<str> {
        self.name.into()
    }

    fn description(&self) -> Cow<str> {
        self.description.into()
    }

    fn supported_advanced_filter_kinds(&self) -> EnumSet<PotFilterKind> {
        enum_set!(PotFilterKind::Bank)
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        self.entries = WalkDir::new(&self.root_dir)
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
                    plugin_cores: find_used_plugins(entry.path(), ctx.plugin_db)
                        .unwrap_or_default(),
                };
                Some(preset_entry)
            })
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
            .flat_map(|(_, entry)| entry.plugin_cores.values().map(|core| core.product_id))
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

    fn find_preset_by_id(&self, _: &ProviderContext, preset_id: InnerPresetId) -> Option<Preset> {
        let preset_entry = self.entries.get(preset_id.0 as usize)?;
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

    fn find_preview_by_preset_id(
        &self,
        _: &ProviderContext,
        _preset_id: InnerPresetId,
    ) -> Option<PathBuf> {
        None
    }
}

/// Finds used plug-ins in a REAPER-XML-like text file (e.g. RPP, RfxChain, RTrackTemplate).
///
/// Examples entries:
///
///     <VST "VSTi: Zebra2 (u-he)" Zebra2.vst 0 Schmackes 1397572658<565354534D44327A6562726132000000> ""
///     <VST "VSTi: ReaSamplOmatic5000 (Cockos)"
///     <CLAP "CLAPi: Surge XT (Surge Synth Team)"
fn find_used_plugins(
    path: &Path,
    plugin_db: &PluginDatabase,
) -> Result<HashMap<PluginId, PluginCore>, Box<dyn Error>> {
    // TODO-high It would be better to look for the unique plug-in ID.
    let regex = regex!(r#"(?m)^\s*<(VST|CLAP) "(.*?)""#);
    let content = fs::read_to_string(path)?;
    let map = regex
        .captures_iter(&content)
        .filter_map(|captures| {
            let name = captures.get(2)?.as_str();
            let plugin = plugin_db.plugins().find(|p| {
                // TODO-medium Testing containment is not very good. Also, we don't consider the
                //  plug-in framework. Really, we should extract plug-in IDs!
                name.contains(&p.common.name)
            })?;
            dbg!(&plugin.common.name);
            Some((plugin.common.core.id, plugin.common.core.clone()))
        })
        .collect();
    Ok(map)
}
