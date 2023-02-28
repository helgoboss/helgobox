use crate::base::blocking_lock;
use crate::base::default_util::{deserialize_null_default, is_default};
use crate::domain::pot::{
    Collections, CurrentPreset, FilterItem, FilterItemCollections, FilterSettings, ParamAssignment,
    Preset, PresetCollection, RuntimeState,
};
use fallible_iterator::FallibleIterator;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::{Connection, OpenFlags, ToSql};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// TODO-medium Introduce target "Pot: Mark preset"
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct PresetId(u32);

#[derive(
    Copy, Clone, Eq, PartialEq, Hash, Debug, Default, serde::Serialize, serde::Deserialize,
)]
pub struct FilterItemId(u32);

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

#[derive(Copy, Clone, Debug, Default)]
pub struct NksFilterSettings {
    pub bank: Option<FilterItemId>,
    pub sub_bank: Option<FilterItemId>,
    pub category: Option<FilterItemId>,
    pub sub_category: Option<FilterItemId>,
    pub mode: Option<FilterItemId>,
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
    pub current_preset: CurrentPreset,
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
            current_preset: CurrentPreset {
                param_mapping: nica_chunk
                    .and_then(|nica_chunk| {
                        let bytes = self.relevant_bytes_of_chunk(&nica_chunk);
                        let value: NicaChunkContent = rmp_serde::from_slice(bytes).ok()?;
                        Some(value.extract_param_mapping())
                    })
                    .unwrap_or_default(),
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

    pub fn build_collections(
        &self,
        state: &RuntimeState,
    ) -> Result<(RuntimeState, Collections), Box<dyn Error>> {
        let (nks_filter_settings, nks_filter_item_collections) =
            self.build_filter_items(state.filter_settings.nks)?;
        let preset_collection = self.build_preset_collection(&state.filter_settings.nks)?;
        let state = RuntimeState {
            filter_settings: FilterSettings {
                nks: nks_filter_settings,
            },
            preset_id: state.preset_id,
        };
        let indexes = Collections {
            filter_item_collections: FilterItemCollections {
                databases: vec![FilterItem {
                    persistent_id: "Nks".to_string(),
                    id: Default::default(),
                    parent_name: Default::default(),
                    name: "NKS".to_string(),
                }],
                nks: nks_filter_item_collections,
            },
            preset_collection,
        };
        Ok((state, indexes))
    }

    fn build_preset_collection(
        &self,
        filter_settings: &NksFilterSettings,
    ) -> Result<PresetCollection, Box<dyn Error>> {
        let mut from_extras = String::new();
        let mut where_extras = String::new();
        let mut params: Vec<&dyn ToSql> = vec![];
        // Bank and sub bank (= "Instrument" and "Bank")
        if let Some(sub_bank_id) = &filter_settings.sub_bank {
            where_extras += " AND i.bank_chain_id = ?";
            params.push(&sub_bank_id.0);
        } else if let Some(bank_id) = &filter_settings.bank {
            where_extras += r#"
                AND i.bank_chain_id IN (
                    SELECT child.id FROM k_bank_chain child WHERE child.entry1 = (
                        SELECT parent.entry1 FROM k_bank_chain parent WHERE parent.id = ?
                    ) 
                )"#;
            params.push(&bank_id.0);
        }
        // Category and sub category (= "Type" and "Sub type")
        if let Some(sub_category_id) = &filter_settings.sub_category {
            from_extras += " JOIN k_sound_info_category ic ON i.id = ic.sound_info_id";
            where_extras += " AND ic.category_id = ?";
            params.push(&sub_category_id.0);
        } else if let Some(category_id) = &filter_settings.category {
            from_extras += " JOIN k_sound_info_category ic ON i.id = ic.sound_info_id";
            where_extras += r#"
                AND ic.category_id IN (
                    SELECT child.id FROM k_category child WHERE child.category = (
                        SELECT parent.category FROM k_category parent WHERE parent.id = ?
                    )
                )"#;
            params.push(&category_id.0);
        }
        // Mode (= "Character")
        if let Some(mode_id) = &filter_settings.mode {
            from_extras += " JOIN k_sound_info_mode im ON i.id = im.sound_info_id";
            where_extras += " AND im.mode_id = ?";
            params.push(&mode_id.0);
        }
        // Put it all together
        let sql = format!("SELECT i.id FROM k_sound_info i{from_extras} WHERE true{where_extras}");
        let mut statement = self.connection.prepare_cached(&sql)?;
        let collection: Result<PresetCollection, _> = statement
            .query(params.as_slice())?
            .mapped(|row| Ok(PresetId(row.get(0)?)))
            .collect();
        Ok(collection?)
    }

    pub fn build_filter_items(
        &self,
        mut settings: NksFilterSettings,
    ) -> Result<(NksFilterSettings, FilterNksItemCollections), Box<dyn Error>> {
        let collections = FilterNksItemCollections {
            banks: self.select_nks_filter_items(
                "SELECT id, '', entry1 FROM k_bank_chain GROUP BY entry1 ORDER BY entry1",
                None,
            ),
            sub_banks: {
                let mut sql = "SELECT id, entry1, entry2 FROM k_bank_chain".to_string();
                let parent_bank_id = settings.bank;
                if parent_bank_id.is_some() {
                    sql += " WHERE entry1 = (SELECT entry1 FROM k_bank_chain WHERE id = ?)";
                }
                sql += " ORDER BY entry2";
                self.select_nks_filter_items(&sql, parent_bank_id)
            },
            categories: self.select_nks_filter_items(
                "SELECT id, '', category FROM k_category GROUP BY category ORDER BY category",
                None,
            ),
            sub_categories: {
                let mut sql = "SELECT id, category, subcategory FROM k_category".to_string();
                let parent_category_id = settings.category;
                if parent_category_id.is_some() {
                    sql += " WHERE category = (SELECT category FROM k_category WHERE id = ?)";
                }
                sql += " ORDER BY subcategory";
                self.select_nks_filter_items(&sql, parent_category_id)
            },
            modes: self
                .select_nks_filter_items("SELECT id, '', name FROM k_mode ORDER BY name", None),
        };
        let clear_setting_if_invalid =
            |setting: &mut Option<FilterItemId>, items: &[FilterItem]| {
                if let Some(id) = setting {
                    if !items.iter().any(|item| item.id == *id) {
                        *setting = None;
                    }
                }
            };
        clear_setting_if_invalid(&mut settings.sub_bank, &collections.sub_banks);
        clear_setting_if_invalid(&mut settings.sub_category, &collections.sub_categories);
        Ok((settings, collections))
    }

    fn select_nks_filter_items(
        &self,
        query: &str,
        parent_id: Option<FilterItemId>,
    ) -> Vec<FilterItem> {
        match self.select_nks_filter_items_internal(query, parent_id) {
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
        parent_id: Option<FilterItemId>,
    ) -> rusqlite::Result<Vec<FilterItem>> {
        let mut statement = self.connection.prepare_cached(query)?;
        let rows = if let Some(parent_id) = parent_id {
            statement.query([parent_id.0])?
        } else {
            statement.query([])?
        };
        rows.map(|row| {
            let name: Option<String> = row.get(2)?;
            let item = FilterItem {
                persistent_id: name.clone().unwrap_or_default(),
                id: FilterItemId(row.get(0)?),
                parent_name: row.get(1)?,
                name: name.unwrap_or_else(|| "Default".to_string()),
            };
            Ok(item)
        })
        .collect()
    }
}

fn path_to_preset_db() -> Result<PathBuf, &'static str> {
    let data_dir = dirs::data_local_dir().ok_or("couldn't identify data-local dir")?;
    let komplete_kontrol_dir = data_dir.join("Native Instruments/Komplete Kontrol");
    Ok(komplete_kontrol_dir.join("komplete.db3"))
}
