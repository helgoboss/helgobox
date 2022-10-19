use crate::base::blocking_lock;
use riff_io::{ChunkMeta, Entry, RiffFile};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct State {
    preset_id: Option<PresetId>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PresetId(u32);

impl State {
    pub fn preset_id(&self) -> Option<PresetId> {
        self.preset_id
    }

    pub fn set_preset_id(&mut self, id: Option<PresetId>) {
        self.preset_id = id;
    }
}

pub struct PresetDb {
    connection: Connection,
    index_by_preset_id: HashMap<PresetId, u32>,
}

#[derive(Debug)]
pub struct Preset {
    pub id: PresetId,
    pub name: String,
    pub file_name: PathBuf,
    pub file_ext: String,
}

pub struct NksFile {
    file: RiffFile,
}

#[derive(Debug)]
pub struct NksFileContent<'a> {
    pub vst_magic_number: u32,
    pub vst_chunk: &'a [u8],
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
        for entry in entries {
            if let Entry::Chunk(chunk_meta) = entry {
                match &chunk_meta.chunk_id {
                    b"PLID" => plid_chunk = Some(chunk_meta),
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

impl PresetDb {
    fn open() -> Result<Mutex<Self>, Box<dyn Error>> {
        let path = path_to_preset_db()?;
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut db = Self {
            connection,
            index_by_preset_id: Default::default(),
        };
        db.refresh_index()?;
        Ok(Mutex::new(db))
    }

    pub fn refresh_index(&mut self) -> Result<(), Box<dyn Error>> {
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

    pub fn count_presets(&self) -> u32 {
        self.index_by_preset_id.len() as u32
        // self.connection
        //     .query_row("SELECT count(*) FROM k_sound_info", [], |row| row.get(0))
        //     .unwrap_or(0)
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

    pub fn find_index_of_preset(&self, id: PresetId) -> Option<u32> {
        self.index_by_preset_id.get(&id).copied()
        // // TODO-medium This is not cheap. We should probably build an in-memory index instead.
        // self.connection
        //     .query_row(
        //         "SELECT row - 1
        //          FROM (
        //              SELECT ROW_NUMBER() OVER(ORDER BY id) as row, id
        //              FROM k_sound_info
        //          )
        //          WHERE id = ?",
        //         [id.0],
        //         |row| Ok(row.get(0)?),
        //     )
        //     .ok()
    }

    pub fn find_preset_id_at_index(&self, index: u32) -> Option<PresetId> {
        // TODO-high We could optimize this by making the index a bi-map
        self.connection
            .query_row(
                "SELECT id FROM k_sound_info ORDER BY id LIMIT 1 OFFSET ?",
                [index],
                |row| Ok(PresetId(row.get(0)?)),
            )
            .ok()
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
}

fn path_to_preset_db() -> Result<PathBuf, &'static str> {
    let data_dir = dirs::data_local_dir().ok_or("couldn't identify data-local dir")?;
    let komplete_kontrol_dir = data_dir.join("Native Instruments/Komplete Kontrol");
    Ok(komplete_kontrol_dir.join("komplete.db3"))
}
