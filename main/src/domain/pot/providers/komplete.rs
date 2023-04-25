use crate::base::blocking_lock;
use crate::domain::pot::api::{OptFilter, PotFilterExcludeList};
use crate::domain::pot::provider_database::{Database, ProviderContext, SortablePresetId};
use crate::domain::pot::{
    BuildInput, Fil, FiledBasedPresetKind, InnerPresetId, MacroParamBank, Preset, PresetCommon,
    PresetKind,
};
use crate::domain::pot::{
    FilterItem, FilterItemCollections, FilterItemId, Filters, MacroParam, ParamAssignment, PluginId,
};
use enum_iterator::IntoEnumIterator;
use fallible_iterator::FallibleIterator;
use realearn_api::persistence::PotFilterItemKind;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::{Connection, OpenFlags, Row, ToSql};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct KompleteDatabase {
    primary_preset_db: Mutex<PresetDb>,
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
            primary_preset_db: PresetDb::open()?,
            secondary_preset_db: PresetDb::open()?,
        };
        Ok(db)
    }
}

impl Database for KompleteDatabase {
    fn filter_item_name(&self) -> String {
        "Komplete".to_string()
    }

    fn refresh(&mut self, _: &ProviderContext) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn query_filter_collections(
        &self,
        input: &BuildInput,
    ) -> Result<FilterItemCollections, Box<dyn Error>> {
        let mut preset_db = blocking_lock(
            &self.primary_preset_db,
            "Komplete DB query_filter_collections",
        );
        preset_db.query_filter_collections(input)
    }

    fn query_presets(&self, input: &BuildInput) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let mut preset_db = blocking_lock(&self.primary_preset_db, "Komplete DB query_presets");
        preset_db.query_presets(input)
    }

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset> {
        let preset_db = blocking_lock(&self.secondary_preset_db, "Komplete DB find_preset_by_id");
        let (common, kind) = preset_db.find_preset_by_id(preset_id)?;
        Some(Preset::new(common, PresetKind::FileBased(kind)))
    }

    fn find_preview_by_preset_id(&self, preset_id: InnerPresetId) -> Option<PathBuf> {
        let preset_db = blocking_lock(
            &self.primary_preset_db,
            "Komplete DB find_preview_by_preset_id",
        );
        preset_db.find_preset_preview_file(preset_id)
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
    pub plugin_id: PluginId,
    pub vst_chunk: &'a [u8],
    pub macro_param_banks: Vec<MacroParamBank>,
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
            plugin_id: {
                let bytes = self.relevant_bytes_of_chunk(&plid_chunk);
                let value: PlidChunkContent =
                    rmp_serde::from_slice(bytes).map_err(|_| "couldn't find VST magic number")?;
                if let Some(vst3_uid) = value.vst3_uid {
                    PluginId::Vst3 { vst_uid: vst3_uid }
                } else {
                    PluginId::Vst2 {
                        vst_magic_number: value.vst_magic,
                    }
                }
            },
            vst_chunk: self.relevant_bytes_of_chunk(&pchk_chunk),
            macro_param_banks: {
                nica_chunk
                    .and_then(|nica_chunk| {
                        let bytes = self.relevant_bytes_of_chunk(&nica_chunk);
                        let value: NicaChunkContent = rmp_serde::from_slice(bytes).ok()?;
                        Some(value.extract_macro_param_banks())
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
    pub fn extract_macro_param_banks(self) -> Vec<MacroParamBank> {
        self.ni8
            .into_iter()
            .map(|params| {
                let params = params
                    .into_iter()
                    .map(move |param| MacroParam {
                        name: param.name,
                        section_name: param.section.unwrap_or_default(),
                        param_index: param.id,
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

    pub fn find_preset_preview_file(&self, id: InnerPresetId) -> Option<PathBuf> {
        let (_, kind) = self.find_preset_by_id(id)?;
        match kind.file_ext.as_str() {
            "wav" | "aif" => Some(kind.path),
            _ => {
                let preview_dir = kind.path.parent()?.join(".previews");
                let pure_file_name = kind.path.file_name()?;
                let preview_file_name = format!("{}.ogg", pure_file_name.to_string_lossy());
                Some(preview_dir.join(preview_file_name))
            }
        }
    }

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
        id: InnerPresetId,
    ) -> Option<(PresetCommon, FiledBasedPresetKind)> {
        self.connection
            .query_row(
                r#"
                SELECT i.name, i.file_name, i.file_ext, i.favorite_id, bc.entry1
                FROM k_sound_info i LEFT OUTER JOIN k_bank_chain bc ON i.bank_chain_id = bc.id
                WHERE i.id = ?
                "#,
                [id.0],
                |row| {
                    let common = PresetCommon {
                        favorite_id: row.get(3)?,
                        name: row.get(0)?,
                        product_name: row.get(4)?,
                    };
                    let kind = FiledBasedPresetKind {
                        path: {
                            let s: String = row.get(1)?;
                            s.into()
                        },
                        file_ext: row.get(2)?,
                    };
                    Ok((common, kind))
                },
            )
            .ok()
    }

    pub fn query_filter_collections(
        &mut self,
        input: &BuildInput,
    ) -> Result<FilterItemCollections, Box<dyn Error>> {
        let mut filter_items = self.build_filter_items(
            &input.filter_settings,
            input.affected_kinds.into_iter(),
            &input.filter_exclude_list,
        );
        let banks_are_affected = input.affected_kinds.contains(PotFilterItemKind::NksBank);
        let sub_banks_are_affected = input.affected_kinds.contains(PotFilterItemKind::NksSubBank);
        if banks_are_affected || sub_banks_are_affected {
            let non_empty_banks =
                self.find_non_empty_banks(input.filter_settings, &input.filter_exclude_list)?;
            if banks_are_affected {
                filter_items.narrow_down(PotFilterItemKind::NksBank, &non_empty_banks);
            }
            if sub_banks_are_affected {
                filter_items.narrow_down(PotFilterItemKind::NksSubBank, &non_empty_banks);
            }
        }
        let categories_are_affected = input
            .affected_kinds
            .contains(PotFilterItemKind::NksCategory);
        let sub_categories_are_affected = input
            .affected_kinds
            .contains(PotFilterItemKind::NksSubCategory);
        if categories_are_affected || sub_categories_are_affected {
            let non_empty_categories =
                self.find_non_empty_categories(input.filter_settings, &input.filter_exclude_list)?;
            if categories_are_affected {
                filter_items.narrow_down(PotFilterItemKind::NksCategory, &non_empty_categories);
            }
            if sub_categories_are_affected {
                filter_items.narrow_down(PotFilterItemKind::NksSubCategory, &non_empty_categories);
            }
        }
        if input.affected_kinds.contains(PotFilterItemKind::NksMode) {
            let non_empty_modes =
                self.find_non_empty_modes(input.filter_settings, &input.filter_exclude_list)?;
            filter_items.narrow_down(PotFilterItemKind::NksMode, &non_empty_modes);
        }
        Ok(filter_items)
    }

    pub fn query_presets(
        &mut self,
        input: &BuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        let search_criteria = SearchCriteria {
            expression: &input.search_expression,
            use_wildcards: input.use_wildcard_search,
        };
        let preset_collection = self.build_preset_collection(
            &input.filter_settings,
            search_criteria,
            &input.filter_exclude_list,
        )?;
        Ok(preset_collection)
    }

    fn build_preset_collection(
        &mut self,
        filter_settings: &Filters,
        search_criteria: SearchCriteria,
        exclude_list: &PotFilterExcludeList,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        self.execute_preset_query(
            filter_settings,
            search_criteria,
            "DISTINCT i.id, i.name",
            None,
            exclude_list,
            None,
            |row| {
                Ok(SortablePresetId::new(
                    InnerPresetId(row.get(0)?),
                    row.get(1)?,
                ))
            },
        )
    }

    fn find_non_empty_banks(
        &mut self,
        mut filters: Filters,
        exclude_list: &PotFilterExcludeList,
    ) -> Result<HashSet<FilterItemId>, Box<dyn Error>> {
        filters.set(PotFilterItemKind::NksBank, None);
        filters.set(PotFilterItemKind::NksSubBank, None);
        filters.set(PotFilterItemKind::NksCategory, None);
        filters.set(PotFilterItemKind::NksSubCategory, None);
        filters.set(PotFilterItemKind::NksMode, None);
        self.execute_preset_query(
            &filters,
            SearchCriteria::empty(),
            "DISTINCT i.bank_chain_id",
            None,
            exclude_list,
            None,
            optional_filter_item_id,
        )
    }

    fn find_non_empty_categories(
        &mut self,
        mut filters: Filters,
        exclude_list: &PotFilterExcludeList,
    ) -> Result<HashSet<FilterItemId>, Box<dyn Error>> {
        filters.set(PotFilterItemKind::NksCategory, None);
        filters.set(PotFilterItemKind::NksSubCategory, None);
        filters.set(PotFilterItemKind::NksMode, None);
        self.execute_preset_query(
            &filters,
            SearchCriteria::empty(),
            "DISTINCT ic.category_id",
            Some(CATEGORY_JOIN),
            exclude_list,
            None,
            optional_filter_item_id,
        )
    }

    fn find_non_empty_modes(
        &mut self,
        mut filters: Filters,
        exclude_list: &PotFilterExcludeList,
    ) -> Result<HashSet<FilterItemId>, Box<dyn Error>> {
        filters.set(PotFilterItemKind::NksMode, None);
        self.execute_preset_query(
            &filters,
            SearchCriteria::empty(),
            "DISTINCT im.mode_id",
            Some(MODE_JOIN),
            exclude_list,
            None,
            optional_filter_item_id,
        )
    }

    fn execute_preset_query<C, R>(
        &mut self,
        filter_settings: &Filters,
        search_criteria: SearchCriteria,
        select_clause: &str,
        from_more: Option<&str>,
        exclude_list: &PotFilterExcludeList,
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
            sql.from_more(v);
        }
        if let Some(v) = order_by {
            sql.order_by(v);
        }
        // Filter on content type (= factory or user)
        if let Some(FilterItemId(Some(Fil::Komplete(content_type)))) =
            filter_settings.get_ref(PotFilterItemKind::NksContentType)
        {
            sql.from_more(CONTENT_PATH_JOIN);
            sql.where_and_with_param("cp.content_type = ?", content_type);
        }
        // Filter on product/device type (= instrument, effect, loop or one shot)
        let empty_device_type_flags = 0;
        if let Some(product_type) = filter_settings.get_ref(PotFilterItemKind::NksProductType) {
            // We chose the filter item IDs so they correspond to the device type flags.
            let device_type_flags = match product_type.0.as_ref() {
                None => Some(&empty_device_type_flags),
                Some(Fil::Komplete(f)) => Some(f),
                _ => None,
            };
            if let Some(flags) = device_type_flags {
                sql.where_and_with_param("i.device_type_flags = ?", flags);
            } else {
                sql.where_and_false();
            }
        };
        // Filter on favorite or not
        if let Some(FilterItemId(Some(favorite))) =
            filter_settings.get_ref(PotFilterItemKind::NksFavorite)
        {
            let is_favorite = *favorite == Fil::Komplete(1);
            if self.ensure_favorites_db_is_attached().is_ok() {
                if is_favorite {
                    // The IN query is vastly superior compared to the other two (EXISTS and JOIN)!
                    sql.where_and("i.favorite_id IN (SELECT id FROM favorites_db.favorites)");
                    // sql.from_more(FAVORITES_JOIN);
                    // sql.where_and(
                    //     "EXISTS (SELECT 1 FROM favorites_db.favorites f WHERE f.id = i.favorite_id)",
                    // );
                } else {
                    // NOT EXISTS is in the same ballpark ... takes long. Fortunately,  this filter
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
        } else if let Some(bank_id) = filter_settings.get_ref(PotFilterItemKind::NksBank) {
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
                    sql.from_more(CATEGORY_JOIN);
                    sql.where_and_with_param("ic.category_id = ?", id);
                }
                _ => {
                    sql.where_and_false();
                }
            }
        } else if let Some(category_id) = filter_settings.get_ref(PotFilterItemKind::NksCategory) {
            match &category_id.0 {
                None => unreachable!("effective_sub_category() should have prevented this"),
                Some(Fil::Komplete(id)) => {
                    sql.from_more(CATEGORY_JOIN);
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
        if let Some(mode_id) = filter_settings.get_ref(PotFilterItemKind::NksMode) {
            match &mode_id.0 {
                None => sql.where_and("i.id NOT IN (SELECT sound_info_id FROM k_sound_info_mode)"),
                Some(Fil::Komplete(id)) => {
                    sql.from_more(MODE_JOIN);
                    sql.where_and_with_param("im.mode_id = ?", id);
                }
                _ => {
                    sql.where_and_false();
                }
            }
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
            sql.where_and_with_param("i.name LIKE ?", &like_expression);
        }
        // Exclude filters
        for kind in PotFilterItemKind::into_enum_iter() {
            if !exclude_list.has_excludes(kind) {
                continue;
            }
            use PotFilterItemKind::*;
            let selector = match kind {
                NksBank | NksSubBank => "i.bank_chain_id",
                NksCategory | NksSubCategory => {
                    sql.from_more(CATEGORY_JOIN);
                    "ic.category_id"
                }
                NksMode => {
                    sql.from_more(MODE_JOIN);
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
                }
            }
        }
        // Put it all together
        let mut statement = self.connection.prepare_cached(&sql.to_string())?;
        let collection: Result<C, _> = statement
            .query(sql.params.as_slice())?
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
        kinds: impl Iterator<Item = PotFilterItemKind>,
        exclude_list: &PotFilterExcludeList,
    ) -> FilterItemCollections {
        let mut collections = FilterItemCollections::empty();
        for kind in kinds {
            let mut filter_items = self.build_filter_items_of_kind(kind, settings);
            filter_items.retain(|i| !exclude_list.contains(kind, i.id));
            collections.set(kind, filter_items);
        }
        collections
    }

    fn build_filter_items_of_kind(
        &self,
        kind: PotFilterItemKind,
        settings: &Filters,
    ) -> Vec<FilterItem> {
        use PotFilterItemKind::*;
        match kind {
            NksBank => self.select_nks_filter_items(
                "SELECT id, '', entry1 FROM k_bank_chain GROUP BY entry1 ORDER BY entry1",
                None,
                true,
            ),
            NksSubBank => {
                let mut sql = "SELECT id, entry1, entry2 FROM k_bank_chain".to_string();
                let parent_bank_filter = settings.get(PotFilterItemKind::NksBank);
                if parent_bank_filter.is_some() {
                    sql += " WHERE entry1 = (SELECT entry1 FROM k_bank_chain WHERE id = ?)";
                }
                sql += " ORDER BY entry2";
                self.select_nks_filter_items(&sql, parent_bank_filter, false)
            }
            NksCategory => self.select_nks_filter_items(
                "SELECT id, '', category FROM k_category GROUP BY category ORDER BY category",
                None,
                true,
            ),
            NksSubCategory => {
                let mut sql = "SELECT id, category, subcategory FROM k_category".to_string();
                let parent_category_filter = settings.get(PotFilterItemKind::NksCategory);
                if parent_category_filter.is_some() {
                    sql += " WHERE category = (SELECT category FROM k_category WHERE id = ?)";
                }
                sql += " ORDER BY subcategory";
                self.select_nks_filter_items(&sql, parent_category_filter, false)
            }
            NksMode => self.select_nks_filter_items(
                "SELECT id, '', name FROM k_mode ORDER BY name",
                None,
                true,
            ),
            _ => vec![],
        }
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
        let rows = if let Some(FilterItemId(Some(Fil::Komplete(parent_filter)))) = parent_filter {
            statement.query([parent_filter])?
        } else {
            statement.query([])?
        };
        let existing_filter_items = rows.map(|row| {
            let name: Option<String> = row.get(2)?;
            let item = FilterItem {
                persistent_id: name.clone().unwrap_or_default(),
                id: {
                    let id: Option<u32> = row.get(0)?;
                    let fil = id.map(Fil::Komplete);
                    FilterItemId(fil)
                },
                name,
                parent_name: row.get(1)?,
                icon: None,
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

fn path_to_main_and_favorites_db() -> Result<(PathBuf, PathBuf), &'static str> {
    let data_dir = dirs::data_local_dir().ok_or("couldn't identify data-local dir")?;
    let main_db_path = data_dir.join("Native Instruments/Komplete Kontrol/komplete.db3");
    let favorites_db_path = data_dir.join("Native Instruments/Shared/favorites.db3");
    Ok((main_db_path, favorites_db_path))
}

fn optional_filter_item_id(row: &Row) -> Result<FilterItemId, rusqlite::Error> {
    let id: Option<u32> = row.get(0)?;
    let fil = id.map(Fil::Komplete);
    Ok(FilterItemId(fil))
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

    pub fn from_more(&mut self, value: impl Into<Cow<'a, str>>) {
        self.from_joins.insert(value.into());
    }

    pub fn where_and_with_param(&mut self, value: impl Into<Cow<'a, str>>, param: &'a dyn ToSql) {
        self.where_and(value);
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

impl<'a> Display for Sql<'a> {
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

const CONTENT_PATH_JOIN: &str = "JOIN k_content_path cp ON cp.id = i.content_path_id";
const CATEGORY_JOIN: &str = "JOIN k_sound_info_category ic ON i.id = ic.sound_info_id";
const MODE_JOIN: &str = "JOIN k_sound_info_mode im ON i.id = im.sound_info_id";
