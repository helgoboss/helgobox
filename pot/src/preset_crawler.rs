use crate::{
    parse_vst2_magic_number, parse_vst3_uid, pot_db, EscapeCatcher, PersistentPresetId, PluginId,
};
use base::enigo::EnigoMouse;
use base::future_util::millis;
use base::hash_util::NonCryptoIndexMap;
use base::{blocking_lock_arc, file_util, hash_util};
use base::{Mouse, MouseCursorPosition};
use camino::{Utf8Path, Utf8PathBuf};
use helgobox_api::persistence::MouseButton;
use reaper_high::{Fx, FxInfo, Reaper};
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

pub type SharedPresetCrawlingState = Arc<Mutex<PresetCrawlingState>>;

#[derive(Debug)]
pub struct PresetCrawlingState {
    crawled_presets: NonCryptoIndexMap<String, CrawledPreset>,
    duplicate_preset_names: Vec<String>,
    same_preset_name_in_a_row: Option<String>,
    same_preset_name_in_a_row_attempts: u32,
    same_preset_names_like_beginning: Vec<String>,
    same_preset_name_like_beginning_attempts: u32,
    total_bytes_crawled: usize,
}

#[derive(Debug)]
pub struct PresetCrawlingOutcome {
    /// One temporary file that holds the chunks of all FXs when crawling finished.
    /// Will be copied to separate destination files at a later stage.
    /// If not set when stopped, this means at first it's a failure. Later we take the
    /// file out of here for processing, in that case it's also `None`.
    pub chunks_file: File,
    pub reason: PresetCrawlerStopReason,
}

impl PresetCrawlingOutcome {
    pub fn new(chunks_file: File, reason: PresetCrawlerStopReason) -> Self {
        Self {
            chunks_file,
            reason,
        }
    }
}

impl PresetCrawlingState {
    pub fn new() -> SharedPresetCrawlingState {
        let state = Self {
            crawled_presets: Default::default(),
            duplicate_preset_names: Default::default(),
            same_preset_name_in_a_row: None,
            same_preset_name_in_a_row_attempts: 0,
            same_preset_names_like_beginning: Default::default(),
            same_preset_name_like_beginning_attempts: 0,
            total_bytes_crawled: 0,
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

    pub fn bytes_crawled(&self) -> usize {
        self.total_bytes_crawled
    }

    pub fn crawled_presets(&self) -> &NonCryptoIndexMap<String, CrawledPreset> {
        &self.crawled_presets
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

    fn add_preset(&mut self, preset: CrawledPreset, never_stop_crawling: bool) -> NextCrawlStep {
        // Give stop signal if we reached the end of the list or are at its beginning again.
        if !never_stop_crawling {
            if let Some(step) = self.make_stop_check(&preset) {
                return step;
            }
        }
        // Reset "same preset name attempts" logic
        self.same_preset_name_in_a_row_attempts = 0;
        if let Some(last_same_preset_name) = self.same_preset_name_in_a_row.take() {
            // Turns out that the last discovered same preset name was actually not the end
            // of the preset list but just an intermediate duplicate. Treat it as such!
            self.duplicate_preset_names.push(last_same_preset_name);
        }
        // Reset "same preset name like beginning" logic
        self.same_preset_name_like_beginning_attempts = 0;
        self.duplicate_preset_names
            .append(&mut self.same_preset_names_like_beginning);
        // Add or skip
        if self.crawled_presets.contains_key(&preset.name) {
            // Duplicate name. Skip preset!
            self.duplicate_preset_names.push(preset.name);
        } else {
            // Add preset
            self.total_bytes_crawled += preset.size_in_bytes;
            self.crawled_presets.insert(preset.name.clone(), preset);
        }
        NextCrawlStep::Continue
    }

    /// This executes a heuristic to check whether the end of the preset list has been reached and
    /// crawling should therefore stop.
    ///
    /// It looks at the preset names only. I also tried to take the chunk into account but it's not
    /// deterministic. Getting the chunk for one preset multiple times can yield different results!
    fn make_stop_check(&mut self, preset: &CrawledPreset) -> Option<NextCrawlStep> {
        // If we haven't crawled anything yet, there's nothing to check.
        let (_, last_preset) = self.crawled_presets.last()?;
        // Check if we get multiple equally named presets in a row.
        if preset.name == last_preset.name {
            // Same name like last crawled preset
            if self.same_preset_name_in_a_row_attempts <= MAX_SAME_PRESET_NAME_IN_A_ROW_ATTEMPTS {
                // Let's tolerate that right now and still continue crawling.
                // It's possible that the plug-in crops the preset name and therefore
                // presets that seemingly have the same name, in fact have different ones
                // but have the same prefix. This happened with Zebra2 VSTi, for example.
                self.same_preset_name_in_a_row_attempts += 1;
                // Don't add it to the list of duplicates right away because it *might* really
                // turn out to be the end of the preset list! If it turns out it isn't, we still add
                // it to the list of duplicates later.
                self.same_preset_name_in_a_row = Some(preset.name.clone());
                return Some(NextCrawlStep::Continue);
            } else {
                // More than max same preset names in a row! That either means the
                // "Next preset" button doesn't work at all or we have reached the end of the
                // preset list.
                return Some(NextCrawlStep::Stop(
                    PresetCrawlerStopReason::PresetNameNotChangingAnymore,
                ));
            }
        }
        // Now check if the presets that we crawl are the same ones that we crawled in the beginning.
        if let Some((_, reference_preset)) = self
            .crawled_presets
            .get_index(self.same_preset_name_like_beginning_attempts as usize)
        {
            if preset.name == reference_preset.name {
                // This preset has the same name as the reference preset, which is one of the
                // presets crawled right at the beginning.
                if self.same_preset_name_like_beginning_attempts
                    <= MAX_SAME_PRESET_NAME_LIKE_BEGINNING_ATTEMPTS
                {
                    // Let's tolerate that right now and still continue crawling.
                    // It's possible that the plug-in doesn't navigate through the preset list in
                    // a linear way.
                    self.same_preset_name_like_beginning_attempts += 1;
                    // Don't add it to the list of duplicates right away because it *might* really
                    // turn out to be the beginning of the preset list! If it turns out it isn't,
                    // we still add it to the list of duplicates later.
                    self.same_preset_names_like_beginning
                        .push(preset.name.clone());
                    return Some(NextCrawlStep::Continue);
                } else {
                    // More than max matches with the beginning! That either means the plug-in
                    // navigates in a *very* non-linear fashion through the preset list or we have
                    // reached the end of the preset list and restarted at its beginning.
                    return Some(NextCrawlStep::Stop(
                        PresetCrawlerStopReason::PresetNameLikeBeginning,
                    ));
                }
            }
        }
        None
    }
}

enum NextCrawlStep {
    Continue,
    Stop(PresetCrawlerStopReason),
}

#[derive(Debug)]
pub struct CrawledPreset {
    name: String,
    offset: u64,
    size_in_bytes: usize,
    destination: Utf8PathBuf,
}

impl CrawledPreset {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn destination(&self) -> &Utf8Path {
        &self.destination
    }
}

pub struct CrawlPresetArgs<F> {
    pub fx: Fx,
    pub next_preset_cursor_pos: MouseCursorPosition,
    pub state: SharedPresetCrawlingState,
    pub stop_if_destination_exists: bool,
    pub never_stop_crawling: bool,
    pub bring_focus_back_to_crawler: F,
}

pub async fn crawl_presets<F>(
    args: CrawlPresetArgs<F>,
) -> Result<PresetCrawlingOutcome, Box<dyn Error + Send + Sync>>
where
    F: Fn() + 'static,
{
    let reaper_resource_dir = Reaper::get().resource_path();
    let fx_info = args.fx.info()?;
    let plugin_id = get_plugin_id_from_fx_info(&fx_info);
    let mut mouse = EnigoMouse::new();
    let escape_catcher = EscapeCatcher::new();
    let mut chunks_file = tempfile::tempfile()?;
    let mut current_file_offset = 0u64;
    loop {
        // Check if escape has been pressed
        if escape_catcher.escape_was_pressed() {
            return Ok(PresetCrawlingOutcome::new(
                chunks_file,
                PresetCrawlerStopReason::Interrupted,
            ));
        }
        // Get preset name
        let name = args
            .fx
            .preset_name()
            .ok_or("couldn't get preset name")?
            .into_string();
        {
            // Query chunk and save it in temporary file
            let fx_chunk = args.fx.chunk()?;
            let fx_chunk_content = fx_chunk.content();
            let fx_chunk_bytes = fx_chunk_content.as_bytes();
            chunks_file.write_all(fx_chunk_bytes)?;
            chunks_file.flush()?;
            // Determine where on the disk the RfxChain file should end up
            let destination = determine_preset_file_destination(
                &fx_info,
                &reaper_resource_dir,
                &name,
                plugin_id.as_ref(),
            );
            if args.stop_if_destination_exists && destination.exists() {
                return Ok(PresetCrawlingOutcome::new(
                    chunks_file,
                    PresetCrawlerStopReason::DestinationFileExists,
                ));
            }
            // Build crawled preset
            let crawled_preset = CrawledPreset {
                destination,
                name,
                offset: current_file_offset,
                size_in_bytes: fx_chunk_bytes.len(),
            };
            current_file_offset += fx_chunk_bytes.len() as u64;
            let next_step = blocking_lock_arc(&args.state, "crawl_presets 3")
                .add_preset(crawled_preset, args.never_stop_crawling);
            match next_step {
                NextCrawlStep::Stop(reason) => {
                    return Ok(PresetCrawlingOutcome::new(chunks_file, reason));
                }
                NextCrawlStep::Continue => {}
            }
        }
        // Click "Next preset" button
        args.fx.show_in_floating_window()?;
        mouse.set_cursor_position(args.next_preset_cursor_pos)?;
        moment().await;
        mouse.press(MouseButton::Left)?;
        moment().await;
        mouse.release(MouseButton::Left)?;
        a_bit_longer().await;
    }
}

fn determine_preset_file_destination(
    fx_info: &FxInfo,
    reaper_resource_dir: &Utf8Path,
    preset_name: &str,
    plugin_id: Option<&PluginId>,
) -> Utf8PathBuf {
    if let Some(persistent_preset_id) = find_shimmable_preset(plugin_id, preset_name) {
        // Matched with existing unsupported preset. Create RfxChain file, a so called shim file,
        // but not in the FX chain directory because we don't want it to show up in the FX chain
        // database. Instead, we want the original preset (probably in the Komplete database)
        // to become loadable. There's logic in our preset loading mechanism that looks for
        // a shim file if it realizes that the preset can't be loaded. A kind of fallback!
        get_shim_file_path(reaper_resource_dir, &persistent_preset_id)
    } else {
        // No match with existing unsupported preset
        let sanitized_effect_name = sanitize_filename::sanitize(&fx_info.effect_name);
        let file_name = format!("{}.RfxChain", &preset_name);
        let sanitized_file_name = sanitize_filename::sanitize(file_name);
        reaper_resource_dir
            .join("FXChains/Pot")
            .join(sanitized_effect_name)
            .join(sanitized_file_name)
    }
}

/// Returns the file name of the original preset.
fn find_shimmable_preset(
    plugin_id: Option<&PluginId>,
    preset_name: &str,
) -> Option<PersistentPresetId> {
    let plugin_id = plugin_id?;
    pot_db().find_unsupported_preset_matching(plugin_id, preset_name)
}

fn get_plugin_id_from_fx_info(fx_info: &FxInfo) -> Option<PluginId> {
    let plugin_id = match fx_info.sub_type_expression.as_str() {
        "VST" | "VSTi" => PluginId::vst2(parse_vst2_magic_number(&fx_info.id).ok()?),
        "VST3" | "VST3i" => PluginId::vst3(parse_vst3_uid(&fx_info.id).ok()?),
        // Komplete doesn't support CLAP or JS anyway, so not important right now.
        _ => return None,
    };
    Some(plugin_id)
}

pub async fn import_crawled_presets(
    state: SharedPresetCrawlingState,
    mut chunks_file: File,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    loop {
        let p = blocking_lock_arc(&state, "import_crawled_presets").pop_crawled_preset();
        let Some(p) = p else {
            break;
        };
        let dest_file_path = &p.destination;
        let dest_dir_path = p.destination.parent().ok_or("destination without parent")?;
        fs::create_dir_all(dest_dir_path)?;
        chunks_file.seek(SeekFrom::Start(p.offset))?;
        let mut buf = vec![0; p.size_in_bytes];
        chunks_file.read_exact(&mut buf)?;
        fs::write(dest_file_path, buf)?;
    }
    Ok(())
}

async fn a_bit_longer() {
    millis(100).await;
}

async fn moment() {
    millis(50).await;
}

const MAX_SAME_PRESET_NAME_IN_A_ROW_ATTEMPTS: u32 = 10;
const MAX_SAME_PRESET_NAME_LIKE_BEGINNING_ATTEMPTS: u32 = 10;

pub fn get_shim_file_path(
    reaper_resource_dir: &Utf8Path,
    preset_id: &PersistentPresetId,
) -> Utf8PathBuf {
    // We don't need to
    let hash =
        hash_util::calculate_persistent_non_crypto_hash_one_shot(preset_id.to_string().as_bytes());
    let file_name = file_util::convert_hash_to_dir_structure(hash, ".RfxChain");
    reaper_resource_dir
        .join("Helgoboss/Pot/shims")
        .join(file_name)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PresetCrawlerStopReason {
    Interrupted,
    DestinationFileExists,
    PresetNameNotChangingAnymore,
    PresetNameLikeBeginning,
}
