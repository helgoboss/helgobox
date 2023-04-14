use crate::base::blocking_lock;
use crate::base::default_util::{deserialize_null_default, is_default};
use crate::domain::pot::{
    BuildOutcome, Collections, CurrentPreset, FilterItem, FilterItemCollections, FilterSettings,
    ParamAssignment, Preset, RuntimeState, Stats,
};
use fallible_iterator::FallibleIterator;
use indexmap::IndexSet;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{Connection, OpenFlags, Row, ToSql};
use std::collections::HashMap;
use std::error::Error;
use std::hash::Hash;
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

// TODO-medium Introduce target "Pot: Mark preset"
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct PresetId(u32);

#[derive(
    Copy, Clone, Eq, PartialEq, Hash, Debug, Default, serde::Serialize, serde::Deserialize,
)]
pub struct FilterItemId(pub Option<u32>);

impl FilterItemId {
    pub const NONE: Self = Self(None);
}

pub struct PresetDb {
    connection: Connection,
}

pub struct NksFile {
    file: RiffFile,
}

#[derive(Debug, Default)]
pub struct FilterNksItemCollections {
    pub banks: Vec<FilterItem>,
    pub sub_banks: Vec<FilterItem>,
    pub categories: Vec<FilterItem>,
    pub sub_categories: Vec<FilterItem>,
    pub modes: Vec<FilterItem>,
}

/// `Some` means a filter is set (can also be the `<None>` filter).
/// `None` means no filter is set (`<Any>`).
pub type OptFilter = Option<FilterItemId>;

#[derive(Copy, Clone, Debug, Default)]
pub struct Filters {
    pub bank: OptFilter,
    pub sub_bank: OptFilter,
    pub category: OptFilter,
    pub sub_category: OptFilter,
    pub mode: OptFilter,
}

impl Filters {
    pub fn effective_sub_bank(&self) -> &OptFilter {
        if self.bank == Some(FilterItemId::NONE) {
            &self.bank
        } else {
            &self.sub_bank
        }
    }

    pub fn effective_sub_category(&self) -> &OptFilter {
        if self.category == Some(FilterItemId::NONE) {
            &self.category
        } else {
            &self.sub_category
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentNksFilterSettings {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub bank: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub sub_bank: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub category: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub sub_category: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub mode: Option<String>,
}

#[derive(Debug)]
pub struct NksFileContent<'a> {
    pub vst_magic_number: u32,
    pub vst_chunk: &'a [u8],
    pub param_mapping: HashMap<u32, u32>,
}

impl NksFile {
    pub fn load(path: &Path) -> Result<Self, &'static str> {
        let file = RiffFile::open(&path.to_string_lossy())
            .map_err(|_| "couldn't open file as RIFF file")?;
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
        for entry in entries {
            if let Entry::Chunk(chunk_meta) = entry {
                match &chunk_meta.chunk_id {
                    b"PLID" => plid_chunk = Some(chunk_meta),
                    b"NICA" => nica_chunk = Some(chunk_meta),
                    b"PCHK" => pchk_chunk = Some(chunk_meta),
                    _ => {}
                }
            }
        }
        let plid_chunk = plid_chunk.ok_or("couldn't find PLID chunk")?;
        let pchk_chunk = pchk_chunk.ok_or("couldn't find PCHK chunk")?;
        // Build content from relevant chunks
        let content = NksFileContent {
            vst_magic_number: {
                let bytes = self.relevant_bytes_of_chunk(&plid_chunk);
                let value: PlidChunkContent =
                    rmp_serde::from_slice(bytes).map_err(|_| "couldn't find VST magic number")?;
                value.vst_magic
            },
            vst_chunk: self.relevant_bytes_of_chunk(&pchk_chunk),
            param_mapping: {
                nica_chunk
                    .and_then(|nica_chunk| {
                        let bytes = self.relevant_bytes_of_chunk(&nica_chunk);
                        let value: NicaChunkContent = rmp_serde::from_slice(bytes).ok()?;
                        Some(value.extract_param_mapping())
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

pub fn with_preset_db<R>(f: impl FnOnce(&PresetDb) -> R) -> Result<R, &'static str> {
    let preset_db = preset_db()?;
    let preset_db = blocking_lock(preset_db);
    Ok(f(&preset_db))
}

pub fn preset_db() -> Result<&'static Mutex<PresetDb>, &'static str> {
    use once_cell::sync::Lazy;
    static PRESET_DB: Lazy<Result<Mutex<PresetDb>, String>> =
        Lazy::new(|| PresetDb::open().map_err(|e| e.to_string()));
    PRESET_DB.as_ref().map_err(|s| s.as_str())
}

#[derive(serde::Deserialize)]
struct PlidChunkContent {
    #[serde(rename = "VST.magic")]
    vst_magic: u32,
}

#[derive(serde::Deserialize)]
struct NicaChunkContent {
    ni8: Vec<Vec<ParamAssignment>>,
}

impl NicaChunkContent {
    pub fn extract_param_mapping(&self) -> HashMap<u32, u32> {
        self.ni8
            .iter()
            .enumerate()
            .flat_map(|(bank_index, bank)| {
                bank.iter()
                    .enumerate()
                    .filter_map(move |(slot_index, slot)| {
                        let param_id = slot.id?;
                        Some((bank_index as u32 * 8 + slot_index as u32, param_id))
                    })
            })
            .collect()
    }
}

impl PresetDb {
    fn open() -> Result<Mutex<Self>, Box<dyn Error>> {
        let path = path_to_preset_db()?;
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Mutex::new(Self { connection }))
    }

    pub fn find_preset_preview_file(&self, id: PresetId) -> Option<PathBuf> {
        let preset = self.find_preset_by_id(id)?;
        match preset.file_ext.as_str() {
            "wav" | "aif" => Some(preset.file_name),
            _ => {
                let preview_dir = preset.file_name.parent()?.join(".previews");
                let pure_file_name = preset.file_name.file_name()?;
                let preview_file_name = format!("{}.ogg", pure_file_name.to_string_lossy());
                Some(preview_dir.join(preview_file_name))
            }
        }
    }

    pub fn find_preset_id_by_favorite_id(&self, favorite_id: &str) -> Option<PresetId> {
        self.connection
            .query_row(
                "SELECT id FROM k_sound_info WHERE favorite_id = ?",
                [favorite_id],
                |row| Ok(PresetId(row.get(0)?)),
            )
            .ok()
    }

    pub fn find_preset_by_id(&self, id: PresetId) -> Option<Preset> {
        self.connection
            .query_row(
                "SELECT name, file_name, file_ext, favorite_id FROM k_sound_info WHERE id = ?",
                [id.0],
                |row| {
                    let preset = Preset {
                        favorite_id: row.get(3)?,
                        id,
                        name: row.get(0)?,
                        file_name: {
                            let s: String = row.get(1)?;
                            s.into()
                        },
                        file_ext: row.get(2)?,
                    };
                    Ok(preset)
                },
            )
            .ok()
    }

    pub fn build_collections(&self, state: &RuntimeState) -> Result<BuildOutcome, Box<dyn Error>> {
        let before = Instant::now();
        // Build filter collections
        let mut filter_items = self.build_filter_items(&state.filter_settings.nks)?;
        // TODO-medium-performance The following ideas could be taken into consideration if the
        //  following queries are too slow:
        //  a) Use just one query to query ALL the preset IDs plus corresponding filter item IDs
        //     ... then do the rest manually in-memory. But the result is a very long list of
        //     combinations.
        //  b) Don't make unnecessary joins (it was easier to make all joins for the narrow-down
        //     logic, but it could be prevented).
        let non_empty_banks = self.find_non_empty_banks()?;
        let non_empty_categories = self.find_non_empty_categories(state.filter_settings.nks)?;
        let non_empty_modes = self.find_non_empty_modes(state.filter_settings.nks)?;
        narrow_down(&mut filter_items.banks, &non_empty_banks);
        narrow_down(&mut filter_items.sub_banks, &non_empty_banks);
        narrow_down(&mut filter_items.categories, &non_empty_categories);
        narrow_down(&mut filter_items.sub_categories, &non_empty_categories);
        narrow_down(&mut filter_items.modes, &non_empty_modes);
        // Fix now invalid filter item IDs
        let clear_setting_if_invalid = |setting: &mut OptFilter, items: &[FilterItem]| {
            if let Some(id) = setting {
                if !items.iter().any(|item| item.id == *id) {
                    *setting = None;
                }
            }
        };
        let mut fixed_settings = state.filter_settings.nks;
        clear_setting_if_invalid(&mut fixed_settings.bank, &filter_items.banks);
        clear_setting_if_invalid(&mut fixed_settings.sub_bank, &filter_items.sub_banks);
        clear_setting_if_invalid(&mut fixed_settings.category, &filter_items.categories);
        clear_setting_if_invalid(
            &mut fixed_settings.sub_category,
            &filter_items.sub_categories,
        );
        // clear_setting_if_invalid(&mut fixed_settings.mode, &filter_items.sub_banks);
        // Build preset collection
        let search_criteria = SearchCriteria {
            expression: &state.search_expression,
            use_wildcards: state.use_wildcard_search,
        };
        let preset_collection = self.build_preset_collection(&fixed_settings, search_criteria)?;
        // Put everything together
        let collections = Collections {
            filter_item_collections: FilterItemCollections {
                databases: vec![FilterItem {
                    persistent_id: "Nks".to_string(),
                    id: Default::default(),
                    parent_name: Default::default(),
                    name: Some("NKS".to_string()),
                }],
                nks: filter_items,
            },
            preset_collection,
        };
        let stats = Stats {
            query_duration: before.elapsed(),
        };
        let outcome = BuildOutcome {
            collections,
            stats,
            filter_settings: FilterSettings {
                nks: fixed_settings,
            },
        };
        Ok(outcome)
    }

    fn build_preset_collection(
        &self,
        filter_settings: &Filters,
        search_criteria: SearchCriteria,
    ) -> Result<IndexSet<PresetId>, Box<dyn Error>> {
        self.execute_preset_query(filter_settings, search_criteria, "i.id", |row| {
            Ok(PresetId(row.get(0)?))
        })
    }

    fn find_non_empty_categories(
        &self,
        filter_settings: Filters,
    ) -> Result<IndexSet<FilterItemId>, Box<dyn Error>> {
        let filter_settings = Filters {
            category: None,
            sub_category: None,
            mode: None,
            ..filter_settings
        };
        self.execute_preset_query(
            &filter_settings,
            SearchCriteria::empty(),
            "DISTINCT ic.category_id",
            optional_filter_item_id,
        )
    }

    fn find_non_empty_banks(&self) -> Result<IndexSet<FilterItemId>, Box<dyn Error>> {
        let filter_settings = Filters::default();
        self.execute_preset_query(
            &filter_settings,
            SearchCriteria::empty(),
            "DISTINCT i.bank_chain_id",
            optional_filter_item_id,
        )
    }

    fn find_non_empty_modes(
        &self,
        filter_settings: Filters,
    ) -> Result<IndexSet<FilterItemId>, Box<dyn Error>> {
        let filter_settings = Filters {
            mode: None,
            ..filter_settings
        };
        self.execute_preset_query(
            &filter_settings,
            SearchCriteria::empty(),
            "DISTINCT im.mode_id",
            optional_filter_item_id,
        )
    }

    fn execute_preset_query<R>(
        &self,
        filter_settings: &Filters,
        search_criteria: SearchCriteria,
        select_clause: &str,
        row_mapper: impl Fn(&Row) -> Result<R, rusqlite::Error>,
    ) -> Result<IndexSet<R>, Box<dyn Error>>
    where
        R: Hash + Eq,
    {
        let mut where_extras = String::new();
        let mut params: Vec<&dyn ToSql> = vec![];
        // Bank and sub bank (= "Instrument" and "Bank")
        if let Some(sub_bank_id) = filter_settings.effective_sub_bank() {
            where_extras += " AND i.bank_chain_id IS ?";
            params.push(sub_bank_id);
        } else if let Some(bank_id) = &filter_settings.bank {
            where_extras += r#"
                AND i.bank_chain_id IN (
                    SELECT child.id FROM k_bank_chain child WHERE child.entry1 = (
                        SELECT parent.entry1 FROM k_bank_chain parent WHERE parent.id = ?
                    ) 
                )"#;
            params.push(bank_id);
        }
        // Category and sub category (= "Type" and "Sub type")
        if let Some(sub_category_id) = filter_settings.effective_sub_category() {
            where_extras += " AND ic.category_id IS ?";
            params.push(sub_category_id);
        } else if let Some(category_id) = &filter_settings.category {
            where_extras += r#"
                AND ic.category_id IN (
                    SELECT child.id FROM k_category child WHERE child.category = (
                        SELECT parent.category FROM k_category parent WHERE parent.id = ?
                    )
                )"#;
            params.push(category_id);
        }
        // Mode (= "Character")
        if let Some(mode_id) = &filter_settings.mode {
            where_extras += " AND im.mode_id IS ?";
            params.push(mode_id);
        }
        // Search expression
        let search_expression = search_criteria.expression;
        let like_expression: String = if search_criteria.use_wildcards {
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
            where_extras += " AND i.name LIKE ?";
            params.push(&like_expression);
        }
        // Put it all together
        let sql = format!(
            r#"
            SELECT {select_clause}
            FROM k_sound_info i
                LEFT OUTER JOIN k_sound_info_category ic ON i.id = ic.sound_info_id
                LEFT OUTER JOIN k_sound_info_mode im ON i.id = im.sound_info_id
            WHERE true{where_extras}
            ORDER BY i.name -- COLLATE NOCASE ASC -- disabled because slow
            "#
        );
        let mut statement = self.connection.prepare_cached(&sql)?;
        let collection: Result<IndexSet<R>, _> = statement
            .query(params.as_slice())?
            .mapped(|row| row_mapper(row))
            .collect();
        Ok(collection?)
    }

    /// Creates filter collections. Narrows down sub filter items as far as possible (based on
    /// the currently selected parent filter item). Doesn't yet narrow down according to whether
    /// presets actually exists which satisfy that filter item! This must be narrowed down at a
    /// later stage!
    pub fn build_filter_items(
        &self,
        settings: &Filters,
    ) -> Result<FilterNksItemCollections, Box<dyn Error>> {
        let collections = FilterNksItemCollections {
            banks: self.select_nks_filter_items(
                "SELECT id, '', entry1 FROM k_bank_chain GROUP BY entry1 ORDER BY entry1",
                None,
                true,
            ),
            sub_banks: {
                let mut sql = "SELECT id, entry1, entry2 FROM k_bank_chain".to_string();
                let parent_bank_filter = settings.bank;
                if parent_bank_filter.is_some() {
                    sql += " WHERE entry1 = (SELECT entry1 FROM k_bank_chain WHERE id = ?)";
                }
                sql += " ORDER BY entry2";
                self.select_nks_filter_items(&sql, parent_bank_filter, false)
            },
            categories: self.select_nks_filter_items(
                "SELECT id, '', category FROM k_category GROUP BY category ORDER BY category",
                None,
                true,
            ),
            sub_categories: {
                let mut sql = "SELECT id, category, subcategory FROM k_category".to_string();
                let parent_category_filter = settings.category;
                if parent_category_filter.is_some() {
                    sql += " WHERE category = (SELECT category FROM k_category WHERE id = ?)";
                }
                sql += " ORDER BY subcategory";
                self.select_nks_filter_items(&sql, parent_category_filter, false)
            },
            modes: self.select_nks_filter_items(
                "SELECT id, '', name FROM k_mode ORDER BY name",
                None,
                true,
            ),
        };
        Ok(collections)
    }

    fn select_nks_filter_items(
        &self,
        query: &str,
        parent_filter: OptFilter,
        include_none_filter: bool,
    ) -> Vec<FilterItem> {
        match self.select_nks_filter_items_internal(query, parent_filter, include_none_filter) {
            Ok(items) => items,
            Err(e) => {
                tracing::error!("Error when selecting NKS filter items: {}", e);
                vec![]
            }
        }
    }

    fn select_nks_filter_items_internal(
        &self,
        query: &str,
        parent_filter: OptFilter,
        include_none_filter: bool,
    ) -> rusqlite::Result<Vec<FilterItem>> {
        let mut statement = self.connection.prepare_cached(query)?;
        let rows = if let Some(parent_filter) = parent_filter {
            statement.query([parent_filter])?
        } else {
            statement.query([])?
        };
        let existing_filter_items = rows.map(|row| {
            let name: Option<String> = row.get(2)?;
            let item = FilterItem {
                persistent_id: name.clone().unwrap_or_default(),
                id: FilterItemId(row.get(0)?),
                name,
                parent_name: row.get(1)?,
            };
            Ok(item)
        });
        if include_none_filter {
            iter::once(Ok(FilterItem::none()))
                .chain(existing_filter_items.iterator())
                .collect()
        } else {
            existing_filter_items.collect()
        }
    }
}

fn path_to_preset_db() -> Result<PathBuf, &'static str> {
    let data_dir = dirs::data_local_dir().ok_or("couldn't identify data-local dir")?;
    let komplete_kontrol_dir = data_dir.join("Native Instruments/Komplete Kontrol");
    Ok(komplete_kontrol_dir.join("komplete.db3"))
}

fn narrow_down(
    filter_items: &mut Vec<FilterItem>,
    non_empty_filter_item_ids: &IndexSet<FilterItemId>,
) {
    filter_items.retain(|item| non_empty_filter_item_ids.contains(&item.id))
}

fn optional_filter_item_id(row: &Row) -> Result<FilterItemId, rusqlite::Error> {
    let id: Option<u32> = row.get(0)?;
    Ok(FilterItemId(id))
}

impl ToSql for FilterItemId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self.0 {
            None => Ok(ToSqlOutput::Owned(Value::Null)),
            Some(id) => Ok(ToSqlOutput::Owned(Value::Integer(id as _))),
        }
    }
}

#[derive(Default)]
struct SearchCriteria<'a> {
    expression: &'a str,
    use_wildcards: bool,
}

impl<'a> SearchCriteria<'a> {
    pub fn empty() -> Self {
        Self::default()
    }
}
