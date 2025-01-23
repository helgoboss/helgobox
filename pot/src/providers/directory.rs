use crate::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
};
use crate::{
    FiledBasedPotPresetKind, FilterInput, InnerBuildInput, InnerPresetId, PersistentDatabaseId,
    PersistentInnerPresetId, PersistentPresetId, PipeEscaped, PluginId, PotPreset, PotPresetCommon,
    PotPresetKind, SearchInput,
};
use std::borrow::Cow;

use crate::plugins::{PluginCore, PluginDatabase};
use base::hash_util::{NonCryptoHashSet, NonCryptoIndexMap, PersistentHash, PersistentHasher};
use camino::Utf8PathBuf;
use either::Either;
use enumset::{enum_set, EnumSet};
use helgobox_api::persistence::PotFilterKind;
use itertools::Itertools;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::hash::Hasher;
use std::io::{BufRead, BufReader};
use std::iter;
use std::path::Path;
use walkdir::WalkDir;

pub struct DirectoryDatabase {
    persistent_id: PersistentDatabaseId,
    root_dir: Utf8PathBuf,
    valid_extensions: NonCryptoHashSet<&'static OsStr>,
    name: &'static str,
    description: &'static str,
    entries: Vec<PresetEntry>,
}

pub struct DirectoryDbConfig {
    pub persistent_id: PersistentDatabaseId,
    pub root_dir: Utf8PathBuf,
    pub valid_extensions: &'static [&'static str],
    pub name: &'static str,
    pub description: &'static str,
}

impl DirectoryDatabase {
    pub fn open(config: DirectoryDbConfig) -> Result<Self, Box<dyn Error>> {
        if !config.root_dir.try_exists()? {
            return Err("path to root directory doesn't exist".into());
        }
        let db = Self {
            persistent_id: config.persistent_id,
            name: config.name,
            entries: Default::default(),
            root_dir: config.root_dir,
            valid_extensions: config.valid_extensions.iter().map(OsStr::new).collect(),
            description: config.description,
        };
        Ok(db)
    }

    fn query_presets_internal<'a>(
        &'a self,
        filter_input: &'a FilterInput,
    ) -> impl Iterator<Item = (usize, &'a PresetEntry)> + 'a {
        let matches = !filter_input.filters.wants_factory_presets_only();
        if !matches {
            return Either::Left(iter::empty());
        }
        let iter = self.entries.iter().enumerate().filter(|(id, e)| {
            let id = InnerPresetId(*id as _);
            e.plugin_cores
                .values()
                .any(|core| filter_input.everything_matches(Some(core), id))
        });
        Either::Right(iter)
    }
}

struct PresetEntry {
    preset_name: String,
    relative_path: String,
    plugin_cores: NonCryptoIndexMap<PluginId, PluginCore>,
    content_hash: PersistentHash,
}

impl Database for DirectoryDatabase {
    fn persistent_id(&self) -> &PersistentDatabaseId {
        &self.persistent_id
    }

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
                let processing_output = process_file(entry.path(), ctx.plugin_db).ok()?;
                let preset_entry = PresetEntry {
                    preset_name: entry.path().file_stem()?.to_str()?.to_string(),
                    relative_path: relative_path.to_str()?.to_string(),
                    plugin_cores: processing_output.used_plugins,
                    content_hash: processing_output.content_hash,
                };
                Some(preset_entry)
            })
            .collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &ProviderContext,
        input: InnerBuildInput,
        _: EnumSet<PotFilterKind>,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        let mut new_filters = *input.filter_input.filters;
        new_filters.clear_this_and_dependent_filters(PotFilterKind::Bank);
        let product_items = self
            .query_presets_internal(&input.filter_input.with_filters(&new_filters))
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
        ctx: &ProviderContext,
        input: InnerBuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let preset_ids = self
            .query_presets_internal(&input.filter_input)
            .filter(|(_, entry)| {
                let search_input = DirectorySearchInput {
                    ctx,
                    preset_entry: entry,
                };
                input.search_evaluator.matches(search_input)
            })
            .map(|(i, entry)| SortablePresetId::new(i as _, entry.preset_name.clone()))
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(
        &self,
        ctx: &ProviderContext,
        preset_id: InnerPresetId,
    ) -> Option<PotPreset> {
        let preset_entry = self.entries.get(preset_id.0 as usize)?;
        let preset = PotPreset {
            common: PotPresetCommon {
                persistent_id: PersistentPresetId::new(
                    self.persistent_id().clone(),
                    create_persistent_inner_id(preset_entry),
                ),
                name: preset_entry.preset_name.clone(),
                context_name: Path::new(&preset_entry.relative_path)
                    .parent()
                    .and_then(|p| Some(p.to_str()?.to_string())),
                plugin_ids: preset_entry.plugin_cores.values().map(|c| c.id).collect(),
                product_ids: preset_entry
                    .plugin_cores
                    .values()
                    .map(|c| c.product_id)
                    .collect(),
                product_name: build_product_name(ctx, preset_entry),
                content_hash: Some(preset_entry.content_hash),
                db_specific_preview_file: None,
                is_supported: true,
                is_available: !preset_entry.plugin_cores.is_empty(),
                metadata: Default::default(),
            },
            kind: PotPresetKind::FileBased(FiledBasedPotPresetKind {
                file_ext: get_file_extension(&preset_entry.relative_path).to_string(),
                path: self.root_dir.join(&preset_entry.relative_path),
            }),
        };
        Some(preset)
    }
}

struct FileProcessingOutput {
    content_hash: PersistentHash,
    used_plugins: NonCryptoIndexMap<PluginId, PluginCore>,
}

/// Finds used plug-ins in a REAPER-XML-like text file (e.g. RPP, RfxChain, RTrackTemplate).
///
/// Examples entries:
///
/// ```text
///     <VST "VSTi: Zebra2 (u-he)" Zebra2.vst 0 Schmackes 1397572658<565354534D44327A6562726132000000> ""
///     <VST "VSTi: ReaSamplOmatic5000 (Cockos)"
///     <CLAP "CLAPi: Surge XT (Surge Synth Team)"
/// ```
fn process_file(
    path: &Path,
    plugin_db: &PluginDatabase,
) -> Result<FileProcessingOutput, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut used_plugins = NonCryptoIndexMap::default();
    let mut buffer = String::new();
    let mut reader = BufReader::new(&file);
    let mut hasher = PersistentHasher::new();
    while let Ok(count) = reader.read_line(&mut buffer) {
        if count == 0 {
            // EOF
            break;
        }
        hasher.write(buffer.as_bytes());
        let line = buffer.trim();
        if let Some(plugin) = plugin_db.detect_plugin_from_rxml_line(line) {
            used_plugins.insert(plugin.common.core.id, plugin.common.core);
        }
        buffer.clear();
    }
    let output = FileProcessingOutput {
        content_hash: hasher.digest_128(),
        used_plugins,
    };
    Ok(output)
}

/// Example: `Synths/Lead.RTrackTemplate`
fn create_persistent_inner_id(preset_entry: &PresetEntry) -> PersistentInnerPresetId {
    let escaped_path = PipeEscaped(preset_entry.relative_path.as_str());
    PersistentInnerPresetId::new(escaped_path.to_string())
}

struct DirectorySearchInput<'a> {
    ctx: &'a ProviderContext<'a>,
    preset_entry: &'a PresetEntry,
}

impl SearchInput for DirectorySearchInput<'_> {
    fn preset_name(&self) -> &str {
        &self.preset_entry.preset_name
    }

    fn product_name(&self) -> Option<Cow<str>> {
        let product_name = build_product_name(self.ctx, self.preset_entry)?;
        Some(product_name.into())
    }

    fn file_extension(&self) -> Option<&str> {
        Some(get_file_extension(&self.preset_entry.relative_path))
    }
}

fn build_product_name(ctx: &ProviderContext, preset_entry: &PresetEntry) -> Option<String> {
    if preset_entry.plugin_cores.len() > 1 {
        Some("<Multiple>".to_string())
    } else if let Some(first) = preset_entry.plugin_cores.values().next() {
        ctx.plugin_db
            .find_plugin_by_id(&first.id)
            .map(|p| p.common.to_string())
    } else {
        None
    }
}

fn get_file_extension(relative_path: &str) -> &str {
    Path::new(relative_path)
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
}
