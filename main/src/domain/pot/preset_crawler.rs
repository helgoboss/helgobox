use crate::base::{blocking_lock_arc, Global};
use crate::domain::enigo::EnigoMouse;
use crate::domain::pot::{spawn_in_pot_worker, EscapeCatcher};
use crate::domain::{Mouse, MouseCursorPosition};
use indexmap::IndexMap;
use realearn_api::persistence::MouseButton;
use reaper_high::{Fx, Reaper};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter, Seek, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs, io};

pub type SharedPresetCrawlingState = Arc<Mutex<PresetCrawlingState>>;

#[derive(Debug)]
pub struct PresetCrawlingState {
    crawled_presets: IndexMap<String, CrawledPreset>,
    status: PresetCrawlingStatus,
    duplicate_preset_names: Vec<String>,
    same_preset_name: Option<String>,
    same_preset_name_attempts: u32,
    bytes_crawled: usize,
}

#[derive(Debug)]
pub enum PresetCrawlingStatus {
    Ongoing,
    Stopped { reason: String },
}

impl PresetCrawlingState {
    pub fn new() -> SharedPresetCrawlingState {
        let state = Self {
            crawled_presets: Default::default(),
            status: PresetCrawlingStatus::Ongoing,
            duplicate_preset_names: Default::default(),
            same_preset_name: None,
            same_preset_name_attempts: 0,
            bytes_crawled: 0,
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

    pub fn status(&self) -> &PresetCrawlingStatus {
        &self.status
    }

    pub fn bytes_crawled(&self) -> usize {
        self.bytes_crawled
    }

    pub fn preset_count(&self) -> u32 {
        self.crawled_presets.len() as _
    }

    pub fn duplicate_preset_name_count(&self) -> u32 {
        self.duplicate_preset_names.len() as _
    }

    pub fn duplicate_preset_names(&self) -> &[String] {
        &self.duplicate_preset_names
    }

    pub fn mark_interrupted(&mut self) {
        self.status = PresetCrawlingStatus::Stopped {
            reason: "Interrupted".to_string(),
        };
    }

    /// Returns `false` if crawling should stop.
    fn add_preset(&mut self, preset: CrawledPreset) -> bool {
        // Give stop signal if we reached the end of the list or are at its beginning again.
        if let Some((_, last_preset)) = self.crawled_presets.last() {
            // I also tried to take the chunk into account but it's not deterministic. Getting the
            // chunk for one preset multiple times can yield different results!
            if preset.name == last_preset.name {
                // Same name like last crawled preset
                if self.same_preset_name_attempts <= MAX_SAME_PRESET_NAME_ATTEMPTS {
                    // Let's tolerate that right now and still continue crawling.
                    // It's possible that the plug-in crops the preset name and therefore
                    // presets that seemingly have the same name, in fact have different ones
                    // but have the same prefix. This happened with Zebra2 VSTi, for example.
                    self.same_preset_name_attempts += 1;
                    // Don't add it to the list of duplicates right away because it might just be
                    // the end of the preset list! If it turns out it isn't, we still add it to
                    // the list of duplicates a bit further down.
                    self.same_preset_name = Some(preset.name);
                    return true;
                } else {
                    // More than max same preset names in a row! That either means we the
                    // "Next preset" button doesn't work at all or we have reached the end of the
                    // preset list.
                    self.status = PresetCrawlingStatus::Stopped {
                        reason: format!(
                            "Preset name doesn't seem to change anymore. Maybe reached end of the \
                        preset list? Last reported preset name: \"{}\" ",
                            &preset.name
                        ),
                    };
                    return false;
                }
            }
            if self.crawled_presets.len() > 1 {
                let (_, first_preset) = self.crawled_presets.first().expect("must exist");
                if preset.name == first_preset.name {
                    // Same name like first crawled preset. We are back at the first preset again,
                    // no need to crawl anymore.
                    self.status = PresetCrawlingStatus::Stopped {
                        reason: format!(
                            "Current preset seems to have the same name as the first crawled \
                            preset. This usually indicates that we have crawled all presets. \
                            Last reported preset name: \"{}\" ",
                            &preset.name
                        ),
                    };
                    return false;
                }
            }
        }
        // Reset "same preset name attempts" logic
        self.same_preset_name_attempts = 0;
        if let Some(last_same_preset_name) = self.same_preset_name.take() {
            // Turns out that the last discovered same preset name was actually not the end
            // of the preset list but just an intermediate duplicate. Treat it as such!
            self.duplicate_preset_names.push(last_same_preset_name);
        }
        // Add or skip
        if self.crawled_presets.contains_key(&preset.name) {
            // Duplicate name. Skip preset!
            self.duplicate_preset_names.push(preset.name);
        } else {
            // Add preset
            self.bytes_crawled += preset.size_in_bytes;
            self.crawled_presets.insert(preset.name.clone(), preset);
        }
        true
    }
}

#[derive(Debug)]
pub struct CrawledPreset {
    name: String,
    file: File,
    size_in_bytes: usize,
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
    bring_focus_back_to_crawler: impl Fn() + 'static,
) {
    Global::future_support().spawn_in_main_thread_from_main_thread(async move {
        let mut mouse = EnigoMouse::default();
        let escape_catcher = EscapeCatcher::new();
        loop {
            // Check if escape has been pressed
            if escape_catcher.escape_was_pressed() {
                // Interrupted
                blocking_lock_arc(&state, "crawl_presets 1").mark_interrupted();
                bring_focus_back_to_crawler();
                break;
            }
            // Get preset name
            let name = fx
                .preset_name()
                .ok_or("couldn't get preset name")?
                .into_string();
            // Query chunk and save it in temporary file
            let fx_chunk = fx.chunk()?;
            let mut file = tempfile::tempfile()?;
            let fx_chunk_content = fx_chunk.content();
            let fx_chunk_bytes = fx_chunk_content.as_bytes();
            file.write_all(fx_chunk_bytes)?;
            file.flush()?;
            file.rewind()?;
            // Build crawled preset
            let crawled_preset = CrawledPreset {
                name,
                file,
                size_in_bytes: fx_chunk_bytes.len(),
            };
            if !blocking_lock_arc(&state, "crawl_presets 2").add_preset(crawled_preset) {
                // Finished
                bring_focus_back_to_crawler();
                break;
            }
            // Click "Next preset" button
            fx.show_in_floating_window();
            mouse.set_cursor_position(next_preset_cursor_pos)?;
            moment().await;
            mouse.press(MouseButton::Left)?;
            moment().await;
            mouse.release(MouseButton::Left)?;
            a_bit_longer().await;
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
            let Some(p) = p else {
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

async fn a_bit_longer() {
    millis(100).await;
}

async fn moment() {
    millis(50).await;
}

async fn millis(amount: u64) {
    futures_timer::Delay::new(Duration::from_millis(amount)).await;
}

const MAX_SAME_PRESET_NAME_ATTEMPTS: u32 = 3;
