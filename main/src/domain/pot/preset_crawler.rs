use crate::base::{blocking_lock_arc, Global};
use crate::domain::enigo::EnigoMouse;
use crate::domain::pot::spawn_in_pot_worker;
use crate::domain::{Mouse, MouseCursorPosition};
use indexmap::map::Entry;
use indexmap::IndexMap;
use realearn_api::persistence::MouseButton;
use reaper_high::{Fx, Reaper};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs, io};

pub type SharedPresetCrawlingState = Arc<Mutex<PresetCrawlingState>>;

#[derive(Debug)]
pub struct PresetCrawlingState {
    crawled_presets: IndexMap<String, CrawledPreset>,
    crawling_finished: bool,
}

impl PresetCrawlingState {
    pub fn new() -> SharedPresetCrawlingState {
        let state = Self {
            crawled_presets: Default::default(),
            crawling_finished: false,
        };
        Arc::new(Mutex::new(state))
    }

    pub fn last_crawled_preset(&self) -> Option<&CrawledPreset> {
        let last = self.crawled_presets.last()?;
        Some(last.1)
    }

    pub fn pop_crawled_preset(&mut self) -> Option<CrawledPreset> {
        let last = self.crawled_presets.pop()?;
        Some(last.1)
    }

    pub fn crawling_is_finished(&self) -> bool {
        self.crawling_finished
    }

    pub fn preset_count(&self) -> u32 {
        self.crawled_presets.len() as _
    }

    /// Returns `false` if finished.
    fn add_preset(&mut self, preset: CrawledPreset) -> bool {
        match self.crawled_presets.entry(preset.name.clone()) {
            Entry::Occupied(_) => {
                self.crawling_finished = true;
                false
            }
            Entry::Vacant(e) => {
                e.insert(preset);
                true
            }
        }
    }
}

#[derive(Debug)]
pub struct CrawledPreset {
    name: String,
    file: File,
}

impl CrawledPreset {
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub fn crawl_presets(
    fx: Fx,
    next_preset_cursor_pos: MouseCursorPosition,
    state: SharedPresetCrawlingState,
) {
    Global::future_support().spawn_in_main_thread_from_main_thread(async move {
        let mut mouse = EnigoMouse::default();
        loop {
            // Get preset name
            let name = fx
                .preset_name()
                .ok_or("couldn't get preset name")?
                .into_string();
            // Query chunk and save it in temporary file
            let fx_chunk = fx.chunk()?;
            let mut file = tempfile::tempfile()?;
            file.write_all(fx_chunk.content().as_bytes())?;
            file.flush()?;
            file.rewind()?;
            // Build crawled preset
            let crawled_preset = CrawledPreset { name, file };
            if !blocking_lock_arc(&state, "crawl_presets").add_preset(crawled_preset) {
                // Finished
                break;
            }
            // Click "Next preset" button
            fx.show_in_floating_window();
            moment().await;
            mouse.set_cursor_position(next_preset_cursor_pos)?;
            moment().await;
            mouse.press(MouseButton::Left)?;
            moment().await;
            mouse.release(MouseButton::Left)?;
            moment().await;
        }
        Ok(())
    });
}

pub fn import_crawled_presets(
    fx: Fx,
    state: SharedPresetCrawlingState,
) -> Result<(), Box<dyn Error>> {
    let fx_chain_dir = Reaper::get().resource_path().join("FXChains");
    let fx_info = fx.info()?;
    spawn_in_pot_worker(async move {
        loop {
            let p = blocking_lock_arc(&state, "import_crawled_presets").pop_crawled_preset();
            let Some(mut p) = p else {
                break;
            };
            let file_name = format!("{}.RfxChain", p.name);
            let dest_dir_path = fx_chain_dir.join(&fx_info.effect_name);
            fs::create_dir_all(&dest_dir_path)?;
            let dest_file_path = dest_dir_path.join(file_name);
            let dest_file = fs::File::create(dest_file_path)?;
            let mut src_file_buffered = BufReader::new(p.file);
            let mut dest_file_buffered = BufWriter::new(dest_file);
            io::copy(&mut src_file_buffered, &mut dest_file_buffered)?;
            dest_file_buffered.flush()?;
        }
        Ok(())
    });
    Ok(())
}

async fn moment() {
    millis(200).await;
}

async fn millis(amount: u64) {
    futures_timer::Delay::new(Duration::from_millis(amount)).await;
}
