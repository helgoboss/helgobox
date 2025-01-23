use crate::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
};
use crate::{
    Fil, FilterInput, FilterItem, FilterItemId, InnerBuildInput, InnerPresetId,
    PersistentDatabaseId, PersistentInnerPresetId, PersistentPresetId, PipeEscaped, PluginId,
    PotPreset, PotPresetCommon, PotPresetKind, ProjectBasedPotPresetKind, ProjectId, SearchInput,
};
use std::borrow::Cow;

use crate::plugins::{PluginCore, PluginDatabase};
use base::hash_util::{
    calculate_persistent_non_crypto_hash_one_shot, NonCryptoIndexMap, PersistentHash,
};
use either::Either;
use enumset::{enum_set, EnumSet};
use helgobox_api::persistence::PotFilterKind;
use itertools::Itertools;

use std::error::Error;
use std::ffi::OsStr;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::{fs, iter};
use walkdir::WalkDir;

pub struct ProjectDatabase {
    persistent_id: PersistentDatabaseId,
    root_dir: PathBuf,
    name: String,
    description: String,
    projects: Vec<Proj>,
    preset_entries: Vec<PresetEntry>,
}

pub struct ProjectDbConfig {
    pub persistent_id: PersistentDatabaseId,
    pub root_dir: PathBuf,
    pub name: String,
}

impl ProjectDatabase {
    pub fn open(config: ProjectDbConfig) -> Result<Self, Box<dyn Error>> {
        if !config.root_dir.try_exists()? {
            return Err("path to projects root directory doesn't exist".into());
        }
        let db = Self {
            persistent_id: config.persistent_id,
            name: config.name,
            preset_entries: Default::default(),
            description: format!("Projects in {}", config.root_dir.to_string_lossy()),
            root_dir: config.root_dir,
            projects: vec![],
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
        let iter = self.preset_entries.iter().enumerate().filter(|(id, e)| {
            if let Some(FilterItemId(Some(Fil::Project(id)))) =
                filter_input.filters.get(PotFilterKind::Project)
            {
                if e.project_id != id {
                    return false;
                }
            }
            let id = InnerPresetId(*id as _);
            e.track_preset
                .used_plugins
                .values()
                .any(|core| filter_input.everything_matches(Some(core), id))
        });
        Either::Right(iter)
    }
}

struct PresetEntry {
    project_id: ProjectId,
    track_preset: TrackPreset,
}

struct Proj {
    name: String,
    relative_path_to_rpp: String,
}

impl Database for ProjectDatabase {
    fn persistent_id(&self) -> &PersistentDatabaseId {
        &self.persistent_id
    }

    fn name(&self) -> Cow<str> {
        self.name.as_str().into()
    }

    fn description(&self) -> Cow<str> {
        self.description.as_str().into()
    }

    fn supported_advanced_filter_kinds(&self) -> EnumSet<PotFilterKind> {
        enum_set!(PotFilterKind::Bank | PotFilterKind::Project)
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        self.preset_entries = WalkDir::new(&self.root_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().is_file() {
                    return None;
                }
                let extension = entry.path().extension()?;
                if extension != OsStr::new("RPP") {
                    return None;
                }
                let relative_path = entry.path().strip_prefix(&self.root_dir).ok()?;
                let stem = entry.path().file_stem()?;
                // Immediately exclude relative paths that can't be represented as valid UTF-8.
                // Otherwise we will potentially open a can of worms (regarding persistence etc.).
                let project = Proj {
                    name: stem.to_str()?.to_string(),
                    relative_path_to_rpp: relative_path.to_str()?.to_string(),
                };
                self.projects.push(project);
                let project_id = ProjectId(self.projects.len() as u32 - 1);
                process_file(entry.path(), ctx.plugin_db, project_id).ok()
            })
            .flatten()
            .collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &ProviderContext,
        input: InnerBuildInput,
        affected_kinds: EnumSet<PotFilterKind>,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        let mut collections = InnerFilterItemCollections::empty();
        if affected_kinds.contains(PotFilterKind::Project) {
            let mut new_filters = *input.filter_input.filters;
            new_filters.clear_this_and_dependent_filters(PotFilterKind::Project);
            let project_items = self
                .query_presets_internal(&input.filter_input.with_filters(&new_filters))
                .map(|(_, entry)| entry.project_id)
                .unique()
                .filter_map(|project_id| {
                    let project = self.projects.get(project_id.0 as usize)?;
                    let item = FilterItem {
                        persistent_id: "".to_string(),
                        id: FilterItemId(Some(Fil::Project(project_id))),
                        parent_name: None,
                        name: Some(project.name.clone()),
                        icon: None,
                        more_info: Some(project.relative_path_to_rpp.to_string()),
                    };
                    Some(InnerFilterItem::Unique(item))
                })
                .collect();
            collections.set(PotFilterKind::Project, project_items);
        }
        if affected_kinds.contains(PotFilterKind::Bank) {
            let mut new_filters = *input.filter_input.filters;
            new_filters.clear_this_and_dependent_filters(PotFilterKind::Bank);
            let product_items = self
                .query_presets_internal(&input.filter_input.with_filters(&new_filters))
                .flat_map(|(_, entry)| {
                    entry
                        .track_preset
                        .used_plugins
                        .values()
                        .map(|core| core.product_id)
                })
                .unique()
                .map(InnerFilterItem::Product)
                .collect();
            collections.set(PotFilterKind::Bank, product_items);
        }
        Ok(collections)
    }

    fn query_presets(
        &self,
        ctx: &ProviderContext,
        input: InnerBuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let preset_ids = self
            .query_presets_internal(&input.filter_input)
            .filter(|(_, preset_entry)| {
                let search_input = ProjectSearchInput { ctx, preset_entry };
                input.search_evaluator.matches(search_input)
            })
            .map(|(i, entry)| SortablePresetId::new(i as _, entry.track_preset.preset_name.clone()))
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(
        &self,
        ctx: &ProviderContext,
        preset_id: InnerPresetId,
    ) -> Option<PotPreset> {
        let preset_entry = self.preset_entries.get(preset_id.0 as usize)?;
        let project = self.projects.get(preset_entry.project_id.0 as usize)?;
        let relative_path = PathBuf::from(&project.relative_path_to_rpp);
        let preset = PotPreset {
            common: PotPresetCommon {
                persistent_id: PersistentPresetId::new(
                    self.persistent_id().clone(),
                    create_persistent_inner_id(project, preset_entry),
                ),
                name: preset_entry.track_preset.preset_name.clone(),
                context_name: Some(project.name.clone()),
                plugin_ids: preset_entry
                    .track_preset
                    .used_plugins
                    .values()
                    .map(|c| c.id)
                    .collect(),
                product_ids: preset_entry
                    .track_preset
                    .used_plugins
                    .values()
                    .map(|c| c.product_id)
                    .collect(),
                product_name: build_product_name(ctx, preset_entry).map(|n| n.to_string()),
                content_hash: Some(preset_entry.track_preset.content_hash),
                db_specific_preview_file: None,
                is_supported: true,
                is_available: !preset_entry.track_preset.used_plugins.is_empty(),
                metadata: Default::default(),
            },
            kind: PotPresetKind::ProjectBased(ProjectBasedPotPresetKind {
                path_to_rpp: self.root_dir.join(relative_path),
                fx_chain_range: preset_entry.track_preset.fx_chain_range.clone(),
            }),
        };
        Some(preset)
    }
}

struct TrackPreset {
    preset_name: String,
    track_id: String,
    fx_chain_range: Range<usize>,
    used_plugins: NonCryptoIndexMap<PluginId, PluginCore>,
    content_hash: PersistentHash,
}

fn process_file(
    path: &Path,
    plugin_db: &PluginDatabase,
    project_id: ProjectId,
) -> Result<Vec<PresetEntry>, Box<dyn Error>> {
    let rppxml = fs::read_to_string(path)?;
    Ok(extract_presets(&rppxml, plugin_db, project_id))
}

/// Example: `maojiao/2023-02-03-ben/2023-02-03-ben.RPP|0FF9F738-7CF6-8A49-9AEA-A9AF26DF9C46`
fn create_persistent_inner_id(
    project: &Proj,
    preset_entry: &PresetEntry,
) -> PersistentInnerPresetId {
    let escaped_path = PipeEscaped(project.relative_path_to_rpp.as_str());
    let id = format!("{escaped_path}|{}", preset_entry.track_preset.track_id);
    PersistentInnerPresetId::new(id)
}

fn extract_presets(
    rppxml: &str,
    plugin_db: &PluginDatabase,
    project_id: ProjectId,
) -> Vec<PresetEntry> {
    use rppxml_parser::*;
    let parser = OneShotParser::new(rppxml);
    #[derive(Debug, Default)]
    struct P<'a> {
        track_id: &'a str,
        name: Option<&'a str>,
        rfx_chain_start: Option<usize>,
        rfx_chain_end: Option<usize>,
        used_plugins: NonCryptoIndexMap<PluginId, PluginCore>,
    }
    impl<'a> P<'a> {
        pub fn new(track_id: &'a str) -> Self {
            Self {
                track_id,
                name: None,
                rfx_chain_start: None,
                rfx_chain_end: None,
                used_plugins: Default::default(),
            }
        }
    }
    let mut stack: Vec<&str> = Vec::with_capacity(10);
    let mut preset: Option<P> = None;
    let mut presets: Vec<P> = vec![];
    for e in parser.events() {
        let line = e.line();
        match e.item {
            Item::StartTag(el) => {
                stack.push(el.name());
                match *stack.as_slice() {
                    ["REAPER_PROJECT", "TRACK"] => {
                        let track_id = el.into_values().next().unwrap_or_default();
                        preset = Some(P::new(track_id));
                    }
                    ["REAPER_PROJECT", "TRACK", "FXCHAIN", _] => {
                        if let Some(p) = &mut preset {
                            if let Some(plugin) =
                                plugin_db.detect_plugin_from_rxml_line(line.trim())
                            {
                                p.used_plugins
                                    .insert(plugin.common.core.id, plugin.common.core);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Item::EndTag => {
                match *stack.as_slice() {
                    ["REAPER_PROJECT", "TRACK"] => {
                        presets.extend(preset.take());
                    }
                    ["REAPER_PROJECT", "TRACK", "FXCHAIN"] => {
                        if let Some(p) = &mut preset {
                            p.rfx_chain_end = Some(e.start);
                        }
                    }
                    _ => {}
                }
                stack.pop();
            }
            Item::Attribute(el) => match *stack.as_slice() {
                ["REAPER_PROJECT", "TRACK"] => match el.name() {
                    "NAME" => {
                        if let Some(p) = &mut preset {
                            let name = el.into_values().next().unwrap_or_default();
                            p.name = Some(name)
                        }
                    }
                    _ => {}
                },
                ["REAPER_PROJECT", "TRACK", "FXCHAIN"] => match el.name() {
                    "BYPASS" => {
                        if let Some(p) = &mut preset {
                            if p.rfx_chain_start.is_none() {
                                p.rfx_chain_start = Some(e.start);
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            Item::Content(_) => {}
            Item::Empty => {}
        }
    }
    presets
        .into_iter()
        .filter_map(|p| {
            if p.used_plugins.is_empty() {
                return None;
            }
            let fx_chain_range = p.rfx_chain_start?..p.rfx_chain_end?;
            let track_id = p.track_id.strip_prefix('{')?.strip_suffix('}')?.to_string();
            let preset_name = p.name?.to_string();
            let content_hash = calculate_persistent_non_crypto_hash_one_shot(
                rppxml[fx_chain_range.clone()].as_bytes(),
            );
            let track_preset = TrackPreset {
                preset_name,
                track_id,
                fx_chain_range,
                used_plugins: p.used_plugins,
                content_hash,
            };
            let preset_entry = PresetEntry {
                project_id,
                track_preset,
            };
            Some(preset_entry)
        })
        .collect()
}

struct ProjectSearchInput<'a> {
    ctx: &'a ProviderContext<'a>,
    preset_entry: &'a PresetEntry,
}

impl SearchInput for ProjectSearchInput<'_> {
    fn preset_name(&self) -> &str {
        &self.preset_entry.track_preset.preset_name
    }

    fn product_name(&self) -> Option<Cow<str>> {
        build_product_name(self.ctx, self.preset_entry)
    }

    fn file_extension(&self) -> Option<&str> {
        None
    }
}

fn build_product_name(
    ctx: &ProviderContext,
    preset_entry: &PresetEntry,
) -> Option<Cow<'static, str>> {
    if preset_entry.track_preset.used_plugins.len() > 1 {
        Some("<Multiple>".into())
    } else if let Some(first) = preset_entry.track_preset.used_plugins.values().next() {
        ctx.plugin_db
            .find_plugin_by_id(&first.id)
            .map(|p| p.common.to_string().into())
    } else {
        None
    }
}
