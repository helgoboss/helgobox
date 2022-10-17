use crate::base::blocking_lock;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct State {
    sound_index: u32,
}

impl State {
    pub fn sound_index(&self) -> u32 {
        self.sound_index
    }

    pub fn set_sound_index(&mut self, index: u32) {
        self.sound_index = index;
    }
}

pub struct SoundDb {
    connection: Connection,
}

pub struct Sound {
    pub name: String,
}

pub fn with_sound_db<R>(f: impl FnOnce(&SoundDb) -> R) -> Result<R, &'static str> {
    let sound_db = sound_db()?;
    let sound_db = blocking_lock(sound_db);
    Ok(f(&sound_db))
}

pub fn sound_db() -> Result<&'static Mutex<SoundDb>, &'static str> {
    use once_cell::sync::Lazy;
    static SOUND_DB: Lazy<Result<Mutex<SoundDb>, String>> = Lazy::new(SoundDb::open);
    SOUND_DB.as_ref().map_err(|s| s.as_str())
}

impl SoundDb {
    fn open() -> Result<Mutex<Self>, String> {
        let path = path_to_sound_db().map_err(|e| e.to_string())?;
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        let db = Self { connection };
        Ok(Mutex::new(db))
    }

    pub fn count_sounds(&self) -> u32 {
        self.connection
            .query_row("SELECT count(*) FROM k_sound_info", [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn sound_by_index(&self, index: u32) -> Option<Sound> {
        self.connection
            .query_row(
                "SELECT name FROM k_sound_info LIMIT 1 OFFSET ?",
                [index],
                |row| {
                    let sound = Sound { name: row.get(0)? };
                    Ok(sound)
                },
            )
            .ok()
    }
}

fn path_to_sound_db() -> Result<PathBuf, &'static str> {
    let data_dir = dirs::data_local_dir().ok_or("couldn't identify data-local dir")?;
    let komplete_kontrol_dir = data_dir.join("Native Instruments/Komplete Kontrol");
    Ok(komplete_kontrol_dir.join("komplete.db3"))
}
