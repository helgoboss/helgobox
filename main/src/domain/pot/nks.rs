use crate::base::blocking_lock;
use crate::domain::pot::{CurrentPreset, FilterItem, ParamAssignment, Preset};
use enum_map::EnumMap;
use realearn_api::persistence::PotFilterItemKind;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// TODO-high It would be best to choose an ID which is a hash of the preset, so it survives DB
//  rebuilds. => use UUID! it's stable
// TODO-medium Introduce target "Pot: Mark preset"
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct PresetId(u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct FilterItemId(u32);

pub struct PresetDb {
    connection: Connection,
    index_by_preset_id: HashMap<PresetId, u32>,
    filter_item_containers: EnumMap<PotFilterItemKind, Vec<FilterItem>>,
}

pub struct NksFile {
    file: RiffFile,
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
        let mut db = Self {
            connection,
            index_by_preset_id: Default::default(),
            filter_item_containers: Default::default(),
        };
        db.refresh_filter_items()?;
        db.refresh_preset_index()?;
        Ok(Mutex::new(db))
    }

    pub fn refresh_preset_index(&mut self) -> Result<(), Box<dyn Error>> {
        use fallible_iterator::FallibleIterator;
        let mut statement = self
            .connection
            .prepare_cached("SELECT id FROM k_sound_info ORDER BY id")?;
        let index: Result<_, _> = statement
            .query([])?
            .map(|row| Ok(PresetId(row.get(0)?)))
            .enumerate()
            .map(|(i, id)| Ok((id, i as u32)))
            .collect();
        self.index_by_preset_id = index?;
        Ok(())
    }

    pub fn refresh_filter_items(&mut self) -> Result<(), Box<dyn Error>> {
        use enum_iterator::IntoEnumIterator;
        for kind in PotFilterItemKind::into_enum_iter() {
            self.filter_item_containers[kind] = self.query_filter_items(kind).unwrap_or_default();
        }
        Ok(())
    }

    pub fn count_filter_items(&self, kind: PotFilterItemKind) -> u32 {
        self.filter_item_containers[kind].len() as _
    }

    pub fn count_presets(&self) -> u32 {
        self.index_by_preset_id.len() as u32
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

    pub fn find_index_of_filter_item(
        &self,
        kind: PotFilterItemKind,
        id: FilterItemId,
    ) -> Option<u32> {
        Some(self.find_filter_item_and_index_by_id(kind, id)?.0)
    }

    pub fn find_index_of_preset(&self, id: PresetId) -> Option<u32> {
        self.index_by_preset_id.get(&id).copied()
    }

    pub fn find_filter_item_id_at_index(
        &self,
        kind: PotFilterItemKind,
        index: u32,
    ) -> Option<FilterItemId> {
        let item = self.filter_item_containers[kind].get(index as usize)?;
        Some(item.id)
    }

    pub fn find_preset_id_at_index(&self, index: u32) -> Option<PresetId> {
        // TODO-medium We could optimize this by making the index a bi-map
        self.connection
            .query_row(
                "SELECT id FROM k_sound_info ORDER BY id LIMIT 1 OFFSET ?",
                [index],
                |row| Ok(PresetId(row.get(0)?)),
            )
            .ok()
    }

    pub fn find_filter_item_by_id(
        &self,
        kind: PotFilterItemKind,
        id: FilterItemId,
    ) -> Option<&FilterItem> {
        Some(self.find_filter_item_and_index_by_id(kind, id)?.1)
    }

    fn find_filter_item_and_index_by_id(
        &self,
        kind: PotFilterItemKind,
        id: FilterItemId,
    ) -> Option<(u32, &FilterItem)> {
        let (i, item) = self.filter_item_containers[kind]
            .iter()
            .enumerate()
            .find(|(i, item)| item.id == id)?;
        Some((i as u32, item))
    }

    pub fn find_preset_by_id(&self, id: PresetId) -> Option<Preset> {
        self.connection
            .query_row(
                "SELECT name, file_name, file_ext FROM k_sound_info WHERE id = ?",
                [id.0],
                |row| {
                    let preset = Preset {
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

    fn query_filter_items(&self, kind: PotFilterItemKind) -> Result<Vec<FilterItem>, String> {
        use PotFilterItemKind::*;
        match kind {
            // TODO-high
            Database => Err("TODO".into()),
            NksBank => self.select_filter_items("SELECT id, entry1 FROM k_bank_chain ORDER BY entry1"),
            NksSubBank => self.select_filter_items(
                "SELECT id, entry2 FROM k_bank_chain WHERE entry2 IS NOT NULL ORDER BY entry2",
            ),
            NksCategory => self.select_filter_items("SELECT id, category FROM k_category ORDER BY category"),
            NksSubCategory => self.select_filter_items("SELECT id, subcategory FROM k_category WHERE subcategory IS NOT NULL ORDER BY subcategory"),
            NksMode => self.select_filter_items("SELECT id, name FROM k_mode ORDER BY name"),
            // TODO-high
            NksFavorite => Err("TODO".into()),
        }
    }

    fn select_filter_items(&self, query: &str) -> Result<Vec<FilterItem>, String> {
        self.select_filter_items_internal(query)
            .map_err(|e| e.to_string())
    }

    fn select_filter_items_internal(&self, query: &str) -> rusqlite::Result<Vec<FilterItem>> {
        use fallible_iterator::FallibleIterator;
        let mut statement = self.connection.prepare_cached(query)?;
        let rows = statement.query([])?;
        rows.map(|row| {
            let item = FilterItem {
                id: FilterItemId(row.get(0)?),
                name: row.get(1)?,
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
