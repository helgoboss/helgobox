use crate::api::{OptFilter, PotFilterExcludes};
use crate::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
    FIL_IS_AVAILABLE_TRUE, FIL_IS_FAVORITE_TRUE, FIL_IS_SUPPORTED_FALSE, FIL_IS_SUPPORTED_TRUE,
    FIL_IS_USER_PRESET_TRUE,
};
use crate::{
    Fil, FiledBasedPotPresetKind, InnerBuildInput, InnerPresetId, MacroParamBank,
    PersistentDatabaseId, PersistentInnerPresetId, PersistentPresetId, PluginKind, PotFxParam,
    PotFxParamId, PotPreset, PotPresetCommon, PotPresetKind, PotPresetMetaData, ProductId,
    SearchEvaluator, SearchField, SearchOptions,
};
use crate::{FilterItem, FilterItemId, Filters, MacroParam, ParamAssignment, PluginId};
use base::blocking_lock;
use enumset::{enum_set, EnumSet};
use helgobox_api::persistence::PotFilterKind;

use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::NaiveDateTime;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::{Connection, OpenFlags, Row, ToSql};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use strum::IntoEnumIterator;

pub struct KompleteDatabase {
    persistent_id: PersistentDatabaseId,
    primary_preset_db: Mutex<PresetDb>,
    nks_filter_item_collections: NksFilterItemCollections,
    nks_bank_id_by_product_id: NonCryptoHashMap<ProductId, u32>,
    nks_product_id_by_bank_id: NonCryptoHashMap<u32, ProductId>,
    nks_product_id_by_extension: NonCryptoHashMap<String, ProductId>,
    /// This returns a second connection to the preset database.
    ///
    /// At the moment, the UI thread continuously queries the database for the currently visible rows.
    /// This runs in parallel with expensive background queries. In order to not get UI freezes due
    /// to mutex contention, we need a second connection to the same DB.
    ///
    /// This is probably temporary. Might be better performance-wise to keep the complete table data
    /// (names and other fields to be shown) in-memory.
    secondary_preset_db: Mutex<PresetDb>,
}

impl KompleteDatabase {
    pub fn open() -> Result<Self, Box<dyn Error>> {
        let db = Self {
            persistent_id: PersistentDatabaseId::new("komplete".to_string()),
            primary_preset_db: PresetDb::open()?,
            nks_filter_item_collections: Default::default(),
            nks_bank_id_by_product_id: Default::default(),
            nks_product_id_by_bank_id: Default::default(),
            nks_product_id_by_extension: Default::default(),
            secondary_preset_db: PresetDb::open()?,
        };
        Ok(db)
    }

    /// Translates all neutral (NKS-independent) filters into NKS filters.
    ///
    /// At the moment, this only affects the product filter. That means it translates any neutral
    /// product filter (representing one of the installed plug-ins) into an NKS bank filter.
    fn translate_neutral_filters_to_nks(&self, mut filters: Filters) -> Filters {
        if let Some(FilterItemId(Some(fil))) = filters.get_ref(PotFilterKind::Bank) {
            if let Some(translated_fil) = self.translate_neutral_product_filter_to_nks(fil) {
                filters.set(
                    PotFilterKind::Bank,
                    Some(FilterItemId(Some(translated_fil))),
                );
            }
        }
        filters
    }

    /// Translates all neutral (NKS-independent) excludes into NKS excludes.
    fn translate_neutral_excludes_to_nks(&self, excludes: &PotFilterExcludes) -> PotFilterExcludes {
        let mut translated_excludes = excludes.clone();
        for fil in excludes.normal_excludes_by_kind(PotFilterKind::Bank) {
            if let Some(translated_fil) = self.translate_neutral_product_filter_to_nks(fil) {
                translated_excludes.remove(PotFilterKind::Bank, FilterItemId(Some(*fil)));
                translated_excludes.add(PotFilterKind::Bank, FilterItemId(Some(translated_fil)));
            }
        }
        translated_excludes
    }

    /// Translates all NKS filter items into neutral ones.
    fn translate_nks_filter_items_to_neutral(&self, collections: &mut InnerFilterItemCollections) {
        for (kind, filter_items) in collections.iter_mut() {
            for filter_item in filter_items {
                if let InnerFilterItem::Unique(it) = filter_item {
                    if let FilterItemId(Some(Fil::Komplete(id))) = it.id {
                        if let Some(translated) =
                            self.translate_nks_filter_item_to_neutral(kind, id)
                        {
                            *filter_item = translated;
                        }
                    }
                }
            }
        }
    }

    fn translate_neutral_product_filter_to_nks(&self, fil: &Fil) -> Option<Fil> {
        if let Fil::Product(pid) = fil {
            if let Some(bank_id) = self.nks_bank_id_by_product_id.get(pid) {
                return Some(Fil::Komplete(*bank_id));
            }
        }
        None
    }

    fn translate_nks_filter_item_to_neutral(
        &self,
        kind: PotFilterKind,
        id: u32,
    ) -> Option<InnerFilterItem> {
        if kind == PotFilterKind::Bank {
            let product_id = self.nks_product_id_by_bank_id.get(&id)?;
            Some(InnerFilterItem::Product(*product_id))
        } else {
            None
        }
    }

    fn find_preset_by_id_internal(
        &self,
        preset_db: &PresetDb,
        id: InnerPresetId,
    ) -> Option<(PotPresetCommon, FiledBasedPotPresetKind)> {
        preset_db.find_preset_by_id(&self.persistent_id, id, |bank_id, extension| {
            // Try to translate bank ID - a number representing either a plug-in product like
            // "Zebra2" or a sub product like "Vintage Organs". If it represents a plug-in product,
            // translating the bank ID can work (if we have that plug-in installed).
            if let Some(bank_id) = bank_id {
                if let Some(product_id) = self.nks_product_id_by_bank_id.get(&bank_id).copied() {
                    tracing::debug!("Looked up product {product_id} for bank {bank_id}.");
                    return Some(product_id);
                } else {
                    tracing::debug!("Looking up product for bank {bank_id} not successful.");
                }
            }
            // If that didn't work because we don't have a bank ID, we have sub product or the
            // plug-in product is simply not installed, try at least to translate the extension.
            self.nks_product_id_by_extension.get(extension).copied()
        })
    }

    fn build_filter_item_collections(
        &self,
        affected_kinds: EnumSet<PotFilterKind>,
        non_empty_filters: &NonEmptyNksFilters,
        excludes: &PotFilterExcludes,
        filters: &Filters,
    ) -> InnerFilterItemCollections {
        let mut api_collections = InnerFilterItemCollections::empty();
        use PotFilterKind::*;
        for kind in affected_kinds {
            let items = match kind {
                Bank => self.build_parent_filter_items(
                    &self
                        .nks_filter_item_collections
                        .bank_collections
                        .parent_items,
                    &non_empty_filters.banks_and_sub_banks,
                    excludes,
                    kind,
                ),
                SubBank => self.build_child_filter_items(
                    &self
                        .nks_filter_item_collections
                        .bank_collections
                        .child_items,
                    &non_empty_filters.banks_and_sub_banks,
                    excludes,
                    filters.get(Bank),
                    kind,
                ),
                Category => self.build_parent_filter_items(
                    &self
                        .nks_filter_item_collections
                        .category_collections
                        .parent_items,
                    &non_empty_filters.categories_and_sub_categories,
                    excludes,
                    kind,
                ),
                SubCategory => self.build_child_filter_items(
                    &self
                        .nks_filter_item_collections
                        .category_collections
                        .child_items,
                    &non_empty_filters.categories_and_sub_categories,
                    excludes,
                    filters.get(Category),
                    kind,
                ),
                Mode => self.build_simple_filter_items(
                    &self.nks_filter_item_collections.modes,
                    &non_empty_filters.modes,
                ),
                _ => continue,
            };
            api_collections.set(kind, items)
        }
        api_collections
    }
    fn build_parent_filter_items(
        &self,
        nks_items: &[ParentNksFilterItem],
        non_empty_filter: &NonEmptyNksFilter,
        excludes: &PotFilterExcludes,
        kind: PotFilterKind,
    ) -> Vec<InnerFilterItem> {
        let existing_filter_items = nks_items
            .iter()
            // Respect exclude list
            .filter(|b| !excludes.contains(kind, FilterItemId(Some(Fil::Komplete(b.id)))))
            // Narrow down to non-empty items
            .filter(|b| {
                non_empty_filter.non_empty_ids.contains(&b.id)
                    || b.child_ids
                        .intersection(&non_empty_filter.non_empty_ids)
                        .next()
                        .is_some()
            })
            // Create actual filter item
            .map(|item| FilterItem {
                persistent_id: item.name.clone(),
                id: FilterItemId(Some(Fil::Komplete(item.id))),
                parent_name: None,
                name: Some(item.name.clone()),
                icon: None,
                more_info: None,
            })
            .map(InnerFilterItem::Unique);
        if non_empty_filter.has_non_associated_presets {
            iter::once(InnerFilterItem::Unique(FilterItem::none()))
                .chain(existing_filter_items)
                .collect()
        } else {
            existing_filter_items.collect()
        }
    }

    fn build_child_filter_items(
        &self,
        nks_items: &[ChildNksFilterItem],
        non_empty_filter: &NonEmptyNksFilter,
        excludes: &PotFilterExcludes,
        parent_filter: OptFilter,
        kind: PotFilterKind,
    ) -> Vec<InnerFilterItem> {
        nks_items
            .iter()
            // Respect parent filter
            .filter(|b| {
                if let Some(filter_item_id) = parent_filter {
                    // Filter set
                    if let Some(Fil::Komplete(parent_id)) = filter_item_id.0 {
                        b.parent_id == parent_id
                    } else {
                        // Filter set to <None> or non-Komplete ID
                        false
                    }
                } else {
                    // Filter not set (= <Any>)
                    true
                }
            })
            // Respect exclude list
            .filter(|b| !excludes.contains(kind, FilterItemId(Some(Fil::Komplete(b.id)))))
            // Narrow down to non-empty items
            .filter(|b| non_empty_filter.non_empty_ids.contains(&b.id))
            // Create actual filter item
            .map(|item| FilterItem {
                persistent_id: "".to_string(),
                id: FilterItemId(Some(Fil::Komplete(item.id))),
                parent_name: Some(item.parent_name.clone()),
                name: item.name.clone(),
                icon: None,
                more_info: None,
            })
            .map(InnerFilterItem::Unique)
            .collect()
    }

    fn build_simple_filter_items(
        &self,
        nks_items: &[SimpleNksFilterItem],
        non_empty_filter: &NonEmptyNksFilter,
    ) -> Vec<InnerFilterItem> {
        nks_items
            .iter()
            // Narrow down to non-empty items
            .filter(|b| non_empty_filter.non_empty_ids.contains(&b.id))
            // Create actual filter item
            .map(|item| FilterItem {
                persistent_id: "".to_string(),
                id: FilterItemId(Some(Fil::Komplete(item.id))),
                parent_name: None,
                name: Some(item.name.clone()),
                icon: None,
                more_info: None,
            })
            .map(InnerFilterItem::Unique)
            .collect()
    }
}

impl Database for KompleteDatabase {
    fn persistent_id(&self) -> &PersistentDatabaseId {
        &self.persistent_id
    }

    fn name(&self) -> Cow<str> {
        "Komplete".into()
    }

    fn description(&self) -> Cow<str> {
        "All presets in your local Native Instruments Komplete database.\nPreset files only show up here after you have scanned them using the Komplete Kontrol software!".into()
    }

    fn supported_advanced_filter_kinds(&self) -> EnumSet<PotFilterKind> {
        enum_set!(
            PotFilterKind::Bank
                | PotFilterKind::SubBank
                | PotFilterKind::Category
                | PotFilterKind::SubCategory
                | PotFilterKind::Mode
        )
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        let preset_db = blocking_lock(
            &self.primary_preset_db,
            "Komplete DB query_filter_collections",
        );
        self.nks_filter_item_collections = preset_db.build_nks_filter_item_collections()?;
        // Obtain all NKS banks and find installed products (= groups of similar plug-ins) that
        // match the bank name. They will be treated as the same in terms of filtering.
        self.nks_bank_id_by_product_id = self
            .nks_filter_item_collections
            .bank_collections
            .parent_items
            .iter()
            .filter_map(|bank| {
                let product_id = ctx.plugin_db.products().find_map(|(product_id, product)| {
                    if bank.name == product.name {
                        tracing::debug!(
                            "Associated bank {} {} with product {} {}",
                            bank.id,
                            bank.name,
                            product_id.0,
                            &product.name,
                        );
                        Some(product_id)
                    } else {
                        None
                    }
                })?;
                Some((product_id, bank.id))
            })
            .collect();
        // Make fast reverse lookup possible as well
        self.nks_product_id_by_bank_id = self
            .nks_bank_id_by_product_id
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        // And associate special extensions with products as well
        self.nks_product_id_by_extension = EXTENSION_TO_PRODUCT_NAME_MAPPING
            .iter()
            .filter_map(|(ext, product_name)| {
                let product_id = ctx.plugin_db.products().find_map(|(i, p)| {
                    if &p.name == product_name {
                        Some(i)
                    } else {
                        None
                    }
                })?;
                Some((ext.to_string(), product_id))
            })
            .collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &ProviderContext,
        input: InnerBuildInput,
        affected_kinds: EnumSet<PotFilterKind>,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        // When a project filter is set, we know that Komplete filters don't matter. This
        // could probably be generalized into Pot database somehow but at the moment, "Project"
        // is the only unsupported foreign filter
        if input
            .filter_input
            .filters
            .is_set_to_concrete_value(PotFilterKind::Project)
        {
            return Ok(InnerFilterItemCollections::empty());
        }
        // Translate possibly incoming "neutral" product filters to "NKS bank" product filters
        let translated_filters = self.translate_neutral_filters_to_nks(*input.filter_input.filters);
        let translated_excludes =
            self.translate_neutral_excludes_to_nks(input.filter_input.excludes);
        let banks_are_affected = affected_kinds.contains(PotFilterKind::Bank);
        let sub_banks_are_affected = affected_kinds.contains(PotFilterKind::SubBank);
        let categories_are_affected = affected_kinds.contains(PotFilterKind::Category);
        let sub_categories_are_affected = affected_kinds.contains(PotFilterKind::SubCategory);
        let modes_are_affected = affected_kinds.contains(PotFilterKind::Mode);
        let mut preset_db = blocking_lock(
            &self.primary_preset_db,
            "Komplete DB query_filter_collections",
        );
        let non_empty_filters = NonEmptyNksFilters {
            banks_and_sub_banks: if banks_are_affected || sub_banks_are_affected {
                NonEmptyNksFilter::from_vec(
                    preset_db.find_non_empty_banks(translated_filters, &translated_excludes)?,
                )
            } else {
                Default::default()
            },
            categories_and_sub_categories: if categories_are_affected || sub_categories_are_affected
            {
                NonEmptyNksFilter::from_vec(
                    preset_db
                        .find_non_empty_categories(translated_filters, &translated_excludes)?,
                )
            } else {
                Default::default()
            },
            modes: if modes_are_affected {
                NonEmptyNksFilter::from_vec(
                    preset_db.find_non_empty_modes(translated_filters, &translated_excludes)?,
                )
            } else {
                Default::default()
            },
        };
        let mut filter_item_collections = self.build_filter_item_collections(
            affected_kinds,
            &non_empty_filters,
            &translated_excludes,
            &translated_filters,
        );
        // Translate some Komplete-specific filter items to shared filter items. It's important that
        // we do this at the end, otherwise the narrow-down logic doesn't work correctly.
        self.translate_nks_filter_items_to_neutral(&mut filter_item_collections);
        Ok(filter_item_collections)
    }

    fn query_presets(
        &self,
        _: &ProviderContext,
        input: InnerBuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let translated_filters = self.translate_neutral_filters_to_nks(*input.filter_input.filters);
        let translated_excludes =
            self.translate_neutral_excludes_to_nks(input.filter_input.excludes);
        let mut preset_db = blocking_lock(&self.primary_preset_db, "Komplete DB query_presets");
        preset_db.query_presets(
            &translated_filters,
            input.search_evaluator,
            &translated_excludes,
        )
    }

    fn find_preset_by_id(
        &self,
        _: &ProviderContext,
        preset_id: InnerPresetId,
    ) -> Option<PotPreset> {
        let preset_db = blocking_lock(&self.secondary_preset_db, "Komplete DB find_preset_by_id");
        let (common, kind) = self.find_preset_by_id_internal(&preset_db, preset_id)?;
        Some(PotPreset::new(common, PotPresetKind::FileBased(kind)))
    }

    fn find_unsupported_preset_matching(
        &self,
        product_id: ProductId,
        preset_name: &str,
    ) -> Option<PotPreset> {
        // Look for corresponding Komplete bank
        let bank_id = self.nks_bank_id_by_product_id.get(&product_id)?;
        // Make sure we only get results from that bank
        let mut filters = Filters::empty();
        filters.set(
            PotFilterKind::Bank,
            Some(FilterItemId(Some(Fil::Komplete(*bank_id)))),
        );
        // Make sure we only get unsupported presets
        filters.set(
            PotFilterKind::IsSupported,
            Some(FilterItemId(Some(FIL_IS_SUPPORTED_FALSE))),
        );
        // Look for exact preset name match
        let search_evaluator = SearchEvaluator::new(
            preset_name,
            SearchOptions {
                use_wildcards: true,
                search_fields: enum_set!(SearchField::PresetName),
            },
        );
        let mut preset_db = blocking_lock(
            &self.primary_preset_db,
            "Komplete DB find_unsupported_preset_matching",
        );
        let preset_ids = preset_db
            .query_presets(&filters, &search_evaluator, &Default::default())
            .ok()?;
        let first_preset_id = preset_ids.first()?;
        let (common, kind) =
            self.find_preset_by_id_internal(&preset_db, first_preset_id.inner_preset_id)?;
        Some(PotPreset::new(common, PotPresetKind::FileBased(kind)))
    }
}

struct PresetDb {
    connection: Connection,
    favorites_db_path: PathBuf,
    attached_favorites_db: bool,
}

pub struct NksFile {
    file: RiffFile,
}

#[derive(Debug)]
pub struct NksFileContent<'a> {
    pub metadata: NisiChunkContent,
    pub plugin_id: PluginId,
    pub vst_chunk: &'a [u8],
    pub macro_param_banks: Vec<MacroParamBank>,
}

impl NksFile {
    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let file = RiffFile::open(&path.to_string_lossy())
            .map_err(|_| "Couldn't fine preset file or doesn't have RIFF format")?;
        Ok(Self { file })
    }

    pub fn content(&self) -> Result<NksFileContent, &'static str> {
        // Find relevant chunks
        let entries = self
            .file
            .read_entries()
            .map_err(|_| "couldn't read NKS file entries")?;
        let mut plid_chunk = None;
        let mut pchk_chunk = None;
        let mut nica_chunk = None;
        let mut nisi_chunk = None;
        for entry in entries {
            if let Entry::Chunk(chunk_meta) = entry {
                // Log chunk contents if trace log level enabled
                if tracing::enabled!(tracing::Level::TRACE) {
                    let bytes = self.relevant_bytes_of_chunk(&chunk_meta);
                    let value: Option<serde_json::Value> = rmp_serde::from_slice(bytes).ok();
                    if let Some(value) = value {
                        tracing::trace!(
                            "# Chunk {}{}{}{}\n{}\n",
                            chunk_meta.chunk_id[0] as char,
                            chunk_meta.chunk_id[1] as char,
                            chunk_meta.chunk_id[2] as char,
                            chunk_meta.chunk_id[3] as char,
                            serde_json::to_string_pretty(&value).unwrap_or_default()
                        );
                    }
                }
                match &chunk_meta.chunk_id {
                    b"PLID" => plid_chunk = Some(chunk_meta),
                    b"NICA" => nica_chunk = Some(chunk_meta),
                    b"PCHK" => pchk_chunk = Some(chunk_meta),
                    b"NISI" => nisi_chunk = Some(chunk_meta),
                    _ => {}
                }
            }
        }
        let plid_chunk = plid_chunk.ok_or("couldn't find PLID chunk")?;
        let pchk_chunk = pchk_chunk.ok_or("couldn't find PCHK chunk")?;
        // Build content from relevant chunks
        let plugin_id = {
            let bytes = self.relevant_bytes_of_chunk(&plid_chunk);
            let value: PlidChunkContent =
                rmp_serde::from_slice(bytes).map_err(|_| "couldn't find VST magic number")?;
            if let Some(vst3_uid) = value.vst3_uid {
                PluginId::Vst3 { vst_uid: vst3_uid }
            } else {
                PluginId::Vst2 {
                    vst_magic_number: value.vst_magic as i32,
                }
            }
        };
        let plugin_kind = plugin_id.kind();
        let content = NksFileContent {
            metadata: nisi_chunk
                .and_then(|ch| {
                    let bytes = self.relevant_bytes_of_chunk(&ch);
                    rmp_serde::from_slice(bytes).ok()
                })
                .unwrap_or_default(),
            plugin_id,
            vst_chunk: self.relevant_bytes_of_chunk(&pchk_chunk),
            macro_param_banks: {
                nica_chunk
                    .and_then(|nica_chunk| {
                        let bytes = self.relevant_bytes_of_chunk(&nica_chunk);
                        let value: NicaChunkContent = rmp_serde::from_slice(bytes).ok()?;
                        Some(value.extract_macro_param_banks(plugin_kind))
                    })
                    .unwrap_or_default()
            },
        };
        Ok(content)
    }

    fn relevant_bytes_of_chunk(&self, chunk: &ChunkMeta) -> &[u8] {
        let skip = 4;
        let offset = chunk.data_offset + skip;
        let size = chunk.chunk_size - skip;
        let range = offset..(offset + size);
        self.file.read_bytes(range)
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct NisiChunkContent {
    #[serde(rename = "author")]
    pub author: Option<String>,
    #[serde(rename = "bankchain")]
    pub bankchain: Vec<String>,
    #[serde(rename = "deviceType")]
    pub device_type: Option<String>,
    #[serde(rename = "modes")]
    pub modes: Vec<String>,
    #[serde(rename = "name")]
    pub name: Option<String>,
    #[serde(rename = "types")]
    pub types: Vec<String>,
    #[serde(rename = "vendor")]
    pub vendor: Option<String>,
}

#[derive(serde::Deserialize)]
struct PlidChunkContent {
    #[serde(rename = "VST.magic")]
    vst_magic: u32,
    // 4 * u32 (5 byte) = 128 bit (8 byte)
    #[serde(rename = "VST3.uid")]
    vst3_uid: Option<[u32; 4]>,
}

#[derive(serde::Deserialize)]
struct NicaChunkContent {
    ni8: Vec<Vec<ParamAssignment>>,
}

impl NicaChunkContent {
    pub fn extract_macro_param_banks(self, plugin_kind: PluginKind) -> Vec<MacroParamBank> {
        self.ni8
            .into_iter()
            .map(|params| {
                let params = params
                    .into_iter()
                    .map(move |param| MacroParam {
                        name: param.name,
                        section: param.section,
                        fx_param: param.id.map(|id| PotFxParam {
                            param_id: match plugin_kind {
                                PluginKind::Vst2 => PotFxParamId::Index(id),
                                PluginKind::Vst3 => PotFxParamId::Id(id),
                                _ => unreachable!("NKS only supports VST2 and VST3"),
                            },
                            // Can be resolved later on demand.
                            resolved_param_index: None,
                        }),
                    })
                    .collect();
                MacroParamBank::new(params)
            })
            .collect()
    }
}

impl PresetDb {
    fn open() -> Result<Mutex<Self>, Box<dyn Error>> {
        let (main_db_path, favorites_db_path) = path_to_main_and_favorites_db()?;
        let connection =
            Connection::open_with_flags(main_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let db = Self {
            connection,
            favorites_db_path,
            attached_favorites_db: false,
        };
        Ok(Mutex::new(db))
    }

    fn ensure_favorites_db_is_attached(&mut self) -> Result<(), Box<dyn Error>> {
        if self.attached_favorites_db {
            return Ok(());
        }
        let mut stmt = self
            .connection
            .prepare_cached("ATTACH DATABASE ? AS favorites_db")?;
        let favorites_db_utf8_path = self
            .favorites_db_path
            .to_str()
            .ok_or("non-UTF8 characters in favorite db path")?;
        stmt.execute([favorites_db_utf8_path])?;
        self.attached_favorites_db = true;
        Ok(())
    }

    fn build_nks_filter_item_collections(&self) -> rusqlite::Result<NksFilterItemCollections> {
        let collections = NksFilterItemCollections {
            bank_collections: {
                let rows = self.read_hierarchy_rows(
                    "SELECT id, entry1, entry2, entry3 FROM k_bank_chain ORDER BY entry1",
                )?;
                NksParentChildCollections::from_sorted_rows(rows)
            },
            category_collections: {
                let rows = self.read_hierarchy_rows(
                    "SELECT id, category, subcategory, subsubcategory FROM k_category ORDER BY category"
                )?;
                NksParentChildCollections::from_sorted_rows(rows)
            },
            modes: {
                let mut statement = self
                    .connection
                    .prepare_cached("SELECT id, name FROM k_mode ORDER BY name")?;
                let rows = statement.query([])?;
                let result: Result<Vec<_>, _> = rows
                    .mapped(|row| {
                        Ok(SimpleNksFilterItem {
                            id: row.get(0)?,
                            name: row.get(1)?,
                        })
                    })
                    .collect();
                result?
            },
        };
        Ok(collections)
    }

    fn read_hierarchy_rows(&self, sql_query: &str) -> rusqlite::Result<Vec<HierarchyRow>> {
        let mut statement = self.connection.prepare_cached(sql_query)?;
        let rows = statement.query([])?;
        rows.mapped(|row| {
            Ok(HierarchyRow {
                id: row.get(0)?,
                level1: row.get(1)?,
                level2: row.get(2)?,
                level3: row.get(3)?,
            })
        })
        .collect()
    }

    #[allow(dead_code)]
    pub fn find_preset_id_by_favorite_id(&self, favorite_id: &str) -> Option<InnerPresetId> {
        self.connection
            .query_row(
                "SELECT id FROM k_sound_info WHERE favorite_id = ?",
                [favorite_id],
                |row| Ok(InnerPresetId(row.get(0)?)),
            )
            .ok()
    }

    pub fn find_preset_by_id(
        &self,
        persistent_db_id: &PersistentDatabaseId,
        id: InnerPresetId,
        translate_bank_or_ext_to_product_id: impl FnOnce(Option<u32>, &str) -> Option<ProductId>,
    ) -> Option<(PotPresetCommon, FiledBasedPotPresetKind)> {
        let sql = format!(
            r#"
                    SELECT i.name, i.file_name, i.file_ext, i.favorite_id, bc.entry1, parent_bc.id, i.vendor, i.author, i.comment, i.file_size, i.mod_date, cp.state
                    FROM k_sound_info i 
                        LEFT OUTER JOIN k_content_path cp ON cp.id = i.content_path_id
                        LEFT OUTER JOIN k_bank_chain bc ON i.bank_chain_id = bc.id
                        LEFT OUTER JOIN ({BANK_SQL_QUERY}) AS parent_bc ON bc.entry1 = parent_bc.entry1
                    WHERE i.id = ?
                    "#
        );
        self.connection
            .query_row(&sql, [id.0], |row| {
                let name: String = row.get(0)?;
                let path: String = row.get(1)?;
                let path: Utf8PathBuf = path.into();
                let file_ext: String = row.get(2)?;
                let favorite_id: String = row.get(3)?;
                let product_name: Option<String> = row.get(4)?;
                let bank_id: Option<u32> = row.get(5)?;
                let product_id = translate_bank_or_ext_to_product_id(bank_id, &file_ext);
                let preview_file = determine_preview_file(&path);
                let content_path_state: Option<u32> = row.get(11)?;
                let common = PotPresetCommon {
                    persistent_id: PersistentPresetId::new(
                        persistent_db_id.clone(),
                        PersistentInnerPresetId::new(favorite_id),
                    ),
                    name,
                    // In Komplete, "product" refers either to a top-level "plug-in product"
                    // (such as Zebra or Massive) or to a "product within a plug-in product"
                    // (e.g. "Abbey Road 60s Drums" within "Kontakt"). Only in the first case,
                    // the bank-to-product-ID translation will find something, because our
                    // plug-in database of course only knows products that represent plug-ins.
                    product_ids: product_id.into_iter().collect(),
                    plugin_ids: vec![],
                    product_name,
                    // We could make a hash of the file contents but since we would have to do that
                    // each time we look up the preset (not at refresh time), we don't do that for
                    // now. It probably would slow scrolling down quite a bit.
                    content_hash: None,
                    db_specific_preview_file: preview_file,
                    is_supported: SUPPORTED_FILE_EXTENSIONS.contains(&file_ext.as_str()),
                    is_available: content_path_state == Some(1),
                    metadata: PotPresetMetaData {
                        author: row.get(6).ok(),
                        vendor: row.get(7).ok(),
                        comment: row.get(8).ok(),
                        file_size_in_bytes: row.get(9)?,
                        modification_date: {
                            let nanos: Option<u64> = row.get(10).ok();
                            nanos.and_then(|n| {
                                NaiveDateTime::from_timestamp_opt(n as i64 / 1_000_000_000, 0)
                            })
                        },
                    },
                    context_name: None,
                };
                let kind = FiledBasedPotPresetKind { path, file_ext };
                Ok((common, kind))
            })
            .ok()
    }

    pub fn query_presets(
        &mut self,
        filters: &Filters,
        search_evaluator: &SearchEvaluator,
        exclude_list: &PotFilterExcludes,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let preset_collection =
            self.build_preset_collection(filters, search_evaluator, exclude_list)?;
        Ok(preset_collection)
    }

    fn build_preset_collection(
        &mut self,
        filter_settings: &Filters,
        search_evaluator: &SearchEvaluator,
        exclude_list: &PotFilterExcludes,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        tracing::trace!("build_preset_collection...");
        self.execute_preset_query(
            filter_settings,
            search_evaluator,
            "DISTINCT i.id, i.name",
            None,
            exclude_list,
            None,
            |row| Ok(SortablePresetId::new(row.get(0)?, row.get(1)?)),
        )
    }

    fn find_non_empty_banks(
        &mut self,
        mut filters: Filters,
        exclude_list: &PotFilterExcludes,
    ) -> Result<Vec<Option<u32>>, Box<dyn Error>> {
        filters.clear_this_and_dependent_filters(PotFilterKind::Bank);
        tracing::trace!("find_non_empty_banks...");
        self.execute_preset_query(
            &filters,
            &SearchEvaluator::default(),
            "DISTINCT i.bank_chain_id",
            None,
            exclude_list,
            None,
            map_to_komplete_filter_id,
        )
    }

    fn find_non_empty_categories(
        &mut self,
        mut filters: Filters,
        exclude_list: &PotFilterExcludes,
    ) -> Result<Vec<Option<u32>>, Box<dyn Error>> {
        filters.clear_this_and_dependent_filters(PotFilterKind::Category);
        tracing::trace!("find_non_empty_categories...");
        self.execute_preset_query(
            &filters,
            &SearchEvaluator::default(),
            "DISTINCT ic.category_id",
            Some(CATEGORY_JOIN),
            exclude_list,
            None,
            map_to_komplete_filter_id,
        )
    }

    fn find_non_empty_modes(
        &mut self,
        mut filters: Filters,
        exclude_list: &PotFilterExcludes,
    ) -> Result<Vec<Option<u32>>, Box<dyn Error>> {
        filters.clear_this_and_dependent_filters(PotFilterKind::Mode);
        tracing::trace!("find_non_empty_modes...");
        self.execute_preset_query(
            &filters,
            &SearchEvaluator::default(),
            "DISTINCT im.mode_id",
            Some(MODE_JOIN),
            exclude_list,
            None,
            map_to_komplete_filter_id,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_preset_query<C, R>(
        &mut self,
        filter_settings: &Filters,
        search_evaluator: &SearchEvaluator,
        select_clause: &str,
        from_more: Option<&str>,
        exclude_list: &PotFilterExcludes,
        order_by: Option<&str>,
        row_mapper: impl Fn(&Row) -> Result<R, rusqlite::Error>,
    ) -> Result<C, Box<dyn Error>>
    where
        C: FromIterator<R>,
    {
        let mut sql = Sql::default();
        sql.select(select_clause);
        sql.from("k_sound_info i");
        if let Some(v) = from_more {
            sql.more_from(v);
        }
        if let Some(v) = order_by {
            sql.order_by(v);
        }
        // Filter on state (= available or not)
        if let Some(FilterItemId(Some(fil))) = filter_settings.get(PotFilterKind::IsAvailable) {
            let state = if fil == FIL_IS_AVAILABLE_TRUE {
                &ONE
            } else {
                &FOUR
            };
            sql.more_from(CONTENT_PATH_JOIN);
            sql.where_and_with_param("cp.state = ?", state);
        }
        // Filter on support (= supported by us to load or not)
        if let Some(FilterItemId(Some(fil))) = filter_settings.get(PotFilterKind::IsSupported) {
            let op = if fil == FIL_IS_SUPPORTED_TRUE {
                "IN"
            } else {
                "NOT IN"
            };
            let file_ext_csv = SUPPORTED_FILE_EXTENSIONS.join(r#"', '"#);
            sql.where_and(format!("i.file_ext {op} ('{}')", file_ext_csv));
        }
        // Filter on content type (= factory or user)
        if let Some(FilterItemId(Some(fil))) = filter_settings.get(PotFilterKind::IsUser) {
            let content_type = if fil == FIL_IS_USER_PRESET_TRUE {
                &ONE
            } else {
                &TWO
            };
            sql.more_from(CONTENT_PATH_JOIN);
            sql.where_and_with_param("cp.content_type = ?", content_type);
        }
        // Filter on product/device type (= instrument, effect, loop or one shot)
        if let Some(product_type) = filter_settings.get(PotFilterKind::ProductKind) {
            // We chose the filter item IDs so they correspond to the device type flags.
            let device_type_flags = match product_type.0.as_ref() {
                None => Some(&ZERO),
                Some(Fil::ProductKind(k)) => Some(k.komplete_id()),
                _ => None,
            };
            if let Some(flags) = device_type_flags {
                sql.where_and_with_param("i.device_type_flags = ?", flags);
            } else {
                sql.where_and_false();
            }
        };
        // Filter on favorite or not
        if let Some(FilterItemId(Some(fil))) = filter_settings.get(PotFilterKind::IsFavorite) {
            let is_favorite = fil == FIL_IS_FAVORITE_TRUE;
            if self.ensure_favorites_db_is_attached().is_ok() {
                if is_favorite {
                    // The IN query is vastly superior compared to the other two (EXISTS and JOIN)!
                    sql.where_and("i.favorite_id IN (SELECT id FROM favorites_db.favorites)");
                    // sql.from_more(FAVORITES_JOIN);
                    // sql.where_and(
                    //     "EXISTS (SELECT 1 FROM favorites_db.favorites f WHERE f.id = i.favorite_id)",
                    // );
                } else {
                    // NOT EXISTS is in the same ballpark ... takes long. Fortunately, this filter
                    // is not popular.
                    sql.where_and("i.favorite_id NOT IN (SELECT id FROM favorites_db.favorites)");
                }
            } else if is_favorite {
                // If the favorites database doesn't exist, it means we have no favorites!
                sql.where_and("false");
            }
        }
        // Filter on bank and sub bank (= "Instrument" and "Bank")
        if let Some(sub_bank_id) = filter_settings.effective_sub_bank() {
            match &sub_bank_id.0 {
                None => {
                    sql.where_and("i.bank_chain_id IS NULL");
                }
                Some(Fil::Komplete(id)) => {
                    sql.where_and_with_param("i.bank_chain_id = ?", id);
                }
                _ => {
                    sql.where_and_false();
                }
            }
        } else if let Some(bank_id) = filter_settings.get_ref(PotFilterKind::Bank) {
            match &bank_id.0 {
                None => unreachable!("effective_sub_bank() should have prevented this"),
                Some(Fil::Komplete(id)) => {
                    sql.where_and_with_param(
                        r#"
                        i.bank_chain_id IN (
                            SELECT child.id FROM k_bank_chain child WHERE child.entry1 = (
                                SELECT parent.entry1 FROM k_bank_chain parent WHERE parent.id = ?
                            ) 
                        )
                        "#,
                        id,
                    );
                }
                _ => {
                    sql.where_and_false();
                }
            }
        }
        // Filter on category and sub category (= "Type" and "Sub type")
        if let Some(sub_category_id) = filter_settings.effective_sub_category() {
            match &sub_category_id.0 {
                None => {
                    sql.where_and("i.id NOT IN (SELECT sound_info_id FROM k_sound_info_category)")
                }
                Some(Fil::Komplete(id)) => {
                    sql.more_from(CATEGORY_JOIN);
                    sql.where_and_with_param("ic.category_id = ?", id);
                }
                _ => {
                    sql.where_and_false();
                }
            }
        } else if let Some(category_id) = filter_settings.get_ref(PotFilterKind::Category) {
            match &category_id.0 {
                None => unreachable!("effective_sub_category() should have prevented this"),
                Some(Fil::Komplete(id)) => {
                    sql.more_from(CATEGORY_JOIN);
                    sql.where_and_with_param(
                        r#"
                        ic.category_id IN (
                            SELECT child.id FROM k_category child WHERE child.category = (
                                SELECT parent.category FROM k_category parent WHERE parent.id = ?
                            )
                        )
                        "#,
                        id,
                    );
                }
                _ => {
                    sql.where_and_false();
                }
            }
        }
        // Filter on mode (= "Character")
        if let Some(mode_id) = filter_settings.get_ref(PotFilterKind::Mode) {
            match &mode_id.0 {
                None => sql.where_and("i.id NOT IN (SELECT sound_info_id FROM k_sound_info_mode)"),
                Some(Fil::Komplete(id)) => {
                    sql.more_from(MODE_JOIN);
                    sql.where_and_with_param("im.mode_id = ?", id);
                }
                _ => {
                    sql.where_and_false();
                }
            }
        }
        // Search expression
        let search_expression = &search_evaluator.processed_search_expression;
        let like_expression: String = if search_evaluator.options.use_wildcards {
            search_expression
                .chars()
                .map(|x| match x {
                    '*' => '%',
                    '?' => '_',
                    _ => x,
                })
                .collect()
        } else {
            format!("%{search_expression}%")
        };
        if !search_expression.is_empty() {
            if search_evaluator.options.search_fields.is_empty() {
                sql.where_and_false();
            } else {
                let mut conjunction = String::new();
                conjunction += "(";
                for (i, field) in search_evaluator.options.search_fields.iter().enumerate() {
                    if i > 0 {
                        conjunction += " OR ";
                    }
                    match field {
                        SearchField::PresetName => {
                            conjunction += "i.name LIKE ?";
                            sql.add_param(&like_expression);
                        }
                        SearchField::ProductName => {
                            sql.more_from(BANK_CHAIN_JOIN);
                            conjunction += "bc.entry1 LIKE ?";
                            sql.add_param(&like_expression);
                        }
                        SearchField::FileExtension => {
                            conjunction += "i.file_ext LIKE ?";
                            sql.add_param(search_expression);
                        }
                    }
                }
                conjunction += ")";
                sql.where_and(conjunction);
            }
        }
        // Exclude filters
        for kind in PotFilterKind::iter() {
            if exclude_list.is_empty(kind) {
                continue;
            }
            use PotFilterKind::*;
            let selector = match kind {
                Bank | SubBank => "i.bank_chain_id",
                Category | SubCategory => {
                    sql.more_from(CATEGORY_JOIN);
                    "ic.category_id"
                }
                Mode => {
                    sql.more_from(MODE_JOIN);
                    "im.mode_id"
                }
                _ => continue,
            };
            if exclude_list.contains_none(kind) {
                sql.where_and(format!("{selector} IS NOT NULL"));
            }
            for exclude in exclude_list.normal_excludes_by_kind(kind) {
                if let Fil::Komplete(id) = exclude {
                    sql.where_and_with_param(format!("{selector} <> ?"), id);
                    // For parent filter excludes such as banks and categories, we also need to
                    // exclude the child filters.
                    match kind {
                        Bank => {
                            sql.where_and_with_param(
                                r#"
                                i.bank_chain_id NOT IN (
                                    SELECT child.id FROM k_bank_chain child WHERE child.entry1 = (
                                        SELECT parent.entry1 FROM k_bank_chain parent WHERE parent.id = ?
                                    ) 
                                )
                                "#,
                                id,
                            );
                        }
                        Category => {
                            sql.where_and_with_param(
                                r#"
                                ic.category_id NOT IN (
                                    SELECT child.id FROM k_category child WHERE child.category = (
                                        SELECT parent.category FROM k_category parent WHERE parent.id = ?
                                    )
                                )
                                "#,
                                id,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        // Put it all together
        let sql_query = sql.to_string();
        tracing::trace!("{sql_query}");
        let mut statement = self.connection.prepare_cached(&sql_query)?;
        let collection: Result<C, _> = statement
            .query(sql.params.as_slice())?
            .mapped(|row| row_mapper(row))
            .collect();
        Ok(collection?)
    }
}

fn path_to_main_and_favorites_db() -> Result<(PathBuf, PathBuf), &'static str> {
    let data_dir = dirs::data_local_dir().ok_or("couldn't identify data-local dir")?;
    let main_db_path = data_dir.join("Native Instruments/Komplete Kontrol/komplete.db3");
    let favorites_db_path = data_dir.join("Native Instruments/Shared/favorites.db3");
    Ok((main_db_path, favorites_db_path))
}

fn map_to_komplete_filter_id(row: &Row) -> Result<Option<u32>, rusqlite::Error> {
    row.get(0)
}

#[derive(Default)]
struct Sql<'a> {
    select_clause: Cow<'a, str>,
    from_main: Cow<'a, str>,
    from_joins: BTreeSet<Cow<'a, str>>,
    where_conjunctions: Vec<Cow<'a, str>>,
    order_by_conditions: Vec<Cow<'a, str>>,
    params: Vec<&'a dyn ToSql>,
}

impl<'a> Sql<'a> {
    pub fn select(&mut self, value: impl Into<Cow<'a, str>>) {
        self.select_clause = value.into();
    }

    pub fn from(&mut self, value: impl Into<Cow<'a, str>>) {
        self.from_main = value.into();
    }

    pub fn more_from(&mut self, value: impl Into<Cow<'a, str>>) {
        self.from_joins.insert(value.into());
    }

    pub fn where_and_with_param(&mut self, value: impl Into<Cow<'a, str>>, param: &'a dyn ToSql) {
        self.where_and(value);
        self.params.push(param);
    }

    pub fn add_param(&mut self, param: &'a dyn ToSql) {
        self.params.push(param);
    }

    pub fn where_and(&mut self, value: impl Into<Cow<'a, str>>) {
        self.where_conjunctions.push(value.into());
    }

    pub fn where_and_false(&mut self) {
        self.where_and("false");
    }

    pub fn order_by(&mut self, value: impl Into<Cow<'a, str>>) {
        self.order_by_conditions.push(value.into());
    }
}

impl Display for Sql<'_> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "SELECT {}", &self.select_clause)?;
        writeln!(f, "FROM {}", &self.from_main)?;
        for join in &self.from_joins {
            writeln!(f, "    {}", join)?;
        }
        for (i, cond) in self.where_conjunctions.iter().enumerate() {
            if i == 0 {
                writeln!(f, "WHERE {}", cond)?;
            } else {
                writeln!(f, "    AND {}", cond)?;
            }
        }
        if !self.order_by_conditions.is_empty() {
            write!(f, "ORDER BY ")?;
        }
        for (i, cond) in self.order_by_conditions.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            cond.fmt(f)?;
        }
        Ok(())
    }
}

/// This picks one of the bank entries to serve as "canonical" parent bank. Which one is not
/// really important as long as the result is the same in all of our queries. It must be
/// deterministic! In our case, SQLite will always choose the one with the lowest ID.
///
/// Note: If there would always be a row whose entry2/entry3 entries are NULL, we would pick
/// this one. But often, there's no such row (because no preset is mapped to the parent directly).
/// So at the end, it still doesn't matter which one we pick as "canonical" parent.
const BANK_SQL_QUERY: &str =
    "SELECT id, '', entry1, '' FROM k_bank_chain GROUP BY entry1 ORDER BY entry1";
const CONTENT_PATH_JOIN: &str = "JOIN k_content_path cp ON cp.id = i.content_path_id";
const CATEGORY_JOIN: &str = "JOIN k_sound_info_category ic ON i.id = ic.sound_info_id";
const MODE_JOIN: &str = "JOIN k_sound_info_mode im ON i.id = im.sound_info_id";
const BANK_CHAIN_JOIN: &str = "JOIN k_bank_chain bc ON i.bank_chain_id = bc.id";
const ZERO: u32 = 0;
const ONE: u32 = 1;
const TWO: u32 = 2;
const FOUR: u32 = 4;

const SUPPORTED_FILE_EXTENSIONS: &[&str] = &["wav", "aif", "ogg", "mp3", "nksf", "nksfx"];

/// In Komplete, product (top-level bank) doesn't necessarily need be a plug-in product. It can be
/// a sub product *for* a plug-in product (e.g. "Vintage Organs" is a product *for*
/// plug-in product "Kontakt"). In the database, there's no association that indicates which sub
/// product belongs to which plug-in product. So we need another way to draw that association.
/// The most accurate way is to use the extension.
///
/// This info is *not* used for loading the preset (not implemented anyway for below plug-ins)
/// but for presenting a list of associated plug-ins when right-clicking the preset.
const EXTENSION_TO_PRODUCT_NAME_MAPPING: &[(&str, &str)] = &[
    ("nki", "Kontakt"),
    ("nksn", "Kontakt"),
    ("ens", "Reaktor"),
    ("nrkt", "Reaktor"),
    ("nksr", "Reaktor"),
    // Other associations are not necessary because above products seem to be the only ones which
    // allow sub products. But for documentation purposes, we leave the other extensions here as
    // well:
    // ("nabs", "Absynth"),
    // ("nbkt", "Battery"),
    // ("nmsv", "Massive"),
    // ("nfm8", "Fm8"),
    // ("ngrr", "Guitar Rig"),
];

fn determine_preview_file(preset_file: &Utf8Path) -> Option<Utf8PathBuf> {
    let preview_dir = preset_file.parent()?.join(".previews");
    let pure_file_name = preset_file.file_name()?;
    let preview_file_name = format!("{pure_file_name}.ogg");
    Some(preview_dir.join(preview_file_name))
}

#[derive(Default)]
struct NksFilterItemCollections {
    bank_collections: NksParentChildCollections,
    category_collections: NksParentChildCollections,
    modes: Vec<SimpleNksFilterItem>,
}

#[derive(Default)]
struct NksParentChildCollections {
    parent_items: Vec<ParentNksFilterItem>,
    child_items: Vec<ChildNksFilterItem>,
}

impl NksParentChildCollections {
    /// Rows must be sorted by `entry1`, otherwise it won't work.
    pub fn from_sorted_rows(rows: impl IntoIterator<Item = HierarchyRow>) -> Self {
        let mut parent_items: Vec<ParentNksFilterItem> = Vec::new();
        let child_items = rows
            .into_iter()
            .map(|row| {
                let parent_id = if let Some(last_parent_item) = parent_items.last_mut() {
                    if row.level1 == last_parent_item.name {
                        // We are still in the same bank. Add child ID to that bank.
                        last_parent_item.child_ids.insert(row.id);
                        last_parent_item.id
                    } else {
                        // New bank
                        parent_items.push(ParentNksFilterItem::from_hierarchy_row(&row));
                        row.id
                    }
                } else {
                    // No bank yet. Add first one.
                    parent_items.push(ParentNksFilterItem::from_hierarchy_row(&row));
                    row.id
                };
                ChildNksFilterItem {
                    id: row.id,
                    name: if let Some(entry2) = row.level2 {
                        if let Some(entry3) = row.level3 {
                            Some(format!("{entry2} / {entry3}"))
                        } else {
                            Some(entry2)
                        }
                    } else {
                        None
                    },
                    parent_id,
                    parent_name: row.level1,
                }
            })
            .collect();
        Self {
            parent_items,
            child_items,
        }
    }
}

struct NonEmptyNksFilters {
    banks_and_sub_banks: NonEmptyNksFilter,
    categories_and_sub_categories: NonEmptyNksFilter,
    modes: NonEmptyNksFilter,
}

#[derive(Default)]
struct NonEmptyNksFilter {
    /// The IDs of all filter items of this kind that have presets.
    non_empty_ids: NonCryptoHashSet<u32>,
    /// Whether there are presets not associated to any filter item of this kind.
    has_non_associated_presets: bool,
}

impl NonEmptyNksFilter {
    pub fn from_vec(vec: Vec<Option<u32>>) -> Self {
        let mut has_non_associated_presets = false;
        let non_empty_ids = vec
            .into_iter()
            .filter_map(|id| {
                if id.is_none() {
                    has_non_associated_presets = true;
                }
                id
            })
            .collect();
        Self {
            non_empty_ids,
            has_non_associated_presets,
        }
    }
}

/// For each parent filter item, there's also one child filter item with the same ID! The one that
/// represents direct association to the parent.
#[derive(Eq, PartialEq, Debug)]
struct ParentNksFilterItem {
    id: u32,
    name: String,
    child_ids: NonCryptoHashSet<u32>,
}

impl ParentNksFilterItem {
    pub fn from_hierarchy_row(r: &HierarchyRow) -> Self {
        Self {
            id: r.id,
            name: r.level1.clone(),
            child_ids: [r.id].into_iter().collect(),
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
struct ChildNksFilterItem {
    id: u32,
    name: Option<String>,
    parent_id: u32,
    parent_name: String,
}

struct SimpleNksFilterItem {
    id: u32,
    name: String,
}

struct HierarchyRow {
    id: u32,
    level1: String,
    level2: Option<String>,
    level3: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ParentNksFilterItem {
        pub fn new(id: u32, name: String, child_ids: NonCryptoHashSet<u32>) -> Self {
            Self {
                id,
                name,
                child_ids,
            }
        }
    }
    impl ChildNksFilterItem {
        pub fn new(id: u32, name: Option<String>, parent_id: u32, parent_name: String) -> Self {
            Self {
                id,
                name,
                parent_id,
                parent_name,
            }
        }
    }

    impl HierarchyRow {
        pub fn new(
            id: u32,
            entry1: String,
            entry2: Option<String>,
            entry3: Option<String>,
        ) -> Self {
            Self {
                id,
                level1: entry1,
                level2: entry2,
                level3: entry3,
            }
        }
    }

    #[test]
    fn nks_bank_collections() {
        // Give
        let rows = vec![
            HierarchyRow::new(1, "a".to_string(), None, None),
            HierarchyRow::new(2, "a".to_string(), Some("aa".to_string()), None),
            HierarchyRow::new(3, "a".to_string(), Some("ab".to_string()), None),
            HierarchyRow::new(4, "b".to_string(), Some("ba".to_string()), None),
            HierarchyRow::new(5, "b".to_string(), Some("bb".to_string()), None),
            HierarchyRow::new(6, "b".to_string(), Some("bc".to_string()), None),
            HierarchyRow::new(7, "c".to_string(), Some("ca".to_string()), None),
            HierarchyRow::new(8, "c".to_string(), None, None),
            HierarchyRow::new(9, "d".to_string(), None, None),
            HierarchyRow::new(10, "e".to_string(), Some("ea".to_string()), None),
            HierarchyRow::new(
                11,
                "f".to_string(),
                Some("fa".to_string()),
                Some("faa".to_string()),
            ),
            HierarchyRow::new(
                12,
                "f".to_string(),
                Some("fa".to_string()),
                Some("fab".to_string()),
            ),
        ];
        // When
        let bank_collections = NksParentChildCollections::from_sorted_rows(rows.into_iter());
        // Then
        assert_eq!(
            bank_collections.parent_items,
            vec![
                ParentNksFilterItem::new(1, "a".to_string(), [1, 2, 3].into_iter().collect()),
                ParentNksFilterItem::new(4, "b".to_string(), [4, 5, 6].into_iter().collect()),
                ParentNksFilterItem::new(7, "c".to_string(), [7, 8].into_iter().collect()),
                ParentNksFilterItem::new(9, "d".to_string(), [9].into_iter().collect()),
                ParentNksFilterItem::new(10, "e".to_string(), [10].into_iter().collect()),
                ParentNksFilterItem::new(11, "f".to_string(), [11, 12].into_iter().collect())
            ]
        );
        assert_eq!(
            bank_collections.child_items,
            vec![
                ChildNksFilterItem::new(1, None, 1, "a".to_string()),
                ChildNksFilterItem::new(2, Some("aa".to_string()), 1, "a".to_string()),
                ChildNksFilterItem::new(3, Some("ab".to_string()), 1, "a".to_string()),
                ChildNksFilterItem::new(4, Some("ba".to_string()), 4, "b".to_string()),
                ChildNksFilterItem::new(5, Some("bb".to_string()), 4, "b".to_string()),
                ChildNksFilterItem::new(6, Some("bc".to_string()), 4, "b".to_string()),
                ChildNksFilterItem::new(7, Some("ca".to_string()), 7, "c".to_string()),
                ChildNksFilterItem::new(8, None, 7, "c".to_string()),
                ChildNksFilterItem::new(9, None, 9, "d".to_string()),
                ChildNksFilterItem::new(10, Some("ea".to_string()), 10, "e".to_string()),
                ChildNksFilterItem::new(11, Some("fa / faa".to_string()), 11, "f".to_string()),
                ChildNksFilterItem::new(12, Some("fa / fab".to_string()), 11, "f".to_string()),
            ]
        );
    }
}
