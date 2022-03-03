use crate::file_util::get_path_for_new_media_file;
use crate::ClipEngineResult;
use playtime_api as api;
use reaper_high::{Item, OwnedSource, Project, ReaperSource};
use reaper_medium::{MidiImportBehavior, OwnedPcmSource};
use std::error::Error;
use std::path::{Path, PathBuf};

/// Creates slot content based on the audio/MIDI file used by the given item.
///
/// If the item uses pooled MIDI instead of a file, this method exports the MIDI data to a new
/// file in the recording directory and uses that one.   
pub fn create_api_source_from_item(
    item: Item,
    force_export_to_file: bool,
) -> Result<api::Source, Box<dyn Error>> {
    let active_take = item.active_take().ok_or("item has no active take")?;
    let root_pcm_source = active_take
        .source()
        .ok_or("take has no source")?
        .root_source();
    let root_pcm_source = ReaperSource::new(root_pcm_source);
    let mode = if force_export_to_file {
        CreateApiSourceMode::ForceExportToFile {
            file_base_name: active_take.name(),
        }
    } else {
        CreateApiSourceMode::AllowEmbeddedData
    };
    create_api_source_from_pcm_source(&root_pcm_source, mode, item.project())
}

enum CreateApiSourceMode {
    AllowEmbeddedData,
    ForceExportToFile { file_base_name: String },
}

fn create_api_source_from_pcm_source(
    pcm_source: &ReaperSource,
    mode: CreateApiSourceMode,
    project: Option<Project>,
) -> Result<api::Source, Box<dyn Error>> {
    let pcm_source_type = pcm_source.r#type();
    let content = if let Some(source_file) = pcm_source.file_name() {
        create_file_api_source(project, &source_file)
    } else if matches!(pcm_source_type.as_str(), "MIDI" | "MIDIPOOL") {
        use CreateApiSourceMode::*;
        match mode {
            AllowEmbeddedData => create_midi_chunk_source(pcm_source.state_chunk()),
            ForceExportToFile { file_base_name } => {
                let file_name = get_path_for_new_media_file(&file_base_name, "mid", project);
                pcm_source
                    .export_to_file(&file_name)
                    .map_err(|_| "couldn't export MIDI source to file")?;
                create_file_api_source(project, &file_name)
            }
        }
    } else {
        return Err(format!("item source incompatible (type {})", pcm_source_type).into());
    };
    Ok(content)
}

/// Takes care of making the path project-relative (if a project is given).
fn create_file_api_source(project: Option<Project>, file: &Path) -> api::Source {
    api::Source::File(api::FileSource {
        path: make_relative(project, file),
    })
}

fn create_midi_chunk_source(chunk: String) -> api::Source {
    api::Source::MidiChunk(api::MidiChunkSource { chunk })
}

/// Creates a REAPER PCM source from the given API source.
///
/// If no project is given, the path will not be relative.
pub fn create_pcm_source_from_api_source(
    source: &api::Source,
    project_for_relative_path: Option<Project>,
) -> ClipEngineResult<OwnedPcmSource> {
    use api::Source::*;
    let pcm_source = match source {
        File(api::FileSource { path }) => {
            let absolute_file = if path.is_relative() {
                project_for_relative_path
                    .ok_or("slot source given as relative file but without project")?
                    .make_path_absolute(path)
                    .ok_or("couldn't make clip source path absolute")?
            } else {
                path.clone()
            };
            // TODO-high-record Maybe we should enforce in-project MIDI to not get recording
            //  problems? Not sure how REAPER behaves when trying to overdub onto a file-based
            //  MIDI source. Check! If it doesn't work, we must convert to MIDI chunk here or
            //  latest when the MIDI source gets overdubbed (latter probably better!). If it does,
            //  then all good but it raises the question if we have to worry about real-time thread
            //  file access. Related preference: Media => MIDI => Import existing MIDI files
            //  Anyway, take care of it as soon as we implement saving modifications
            //  (through recording or editing). Before it doesn't matter.
            OwnedSource::from_file(&absolute_file, MidiImportBehavior::UsePreference)?
        }
        MidiChunk(api::MidiChunkSource { chunk }) => {
            let mut source = OwnedSource::from_type("MIDI")?;
            let mut chunk = chunk.clone();
            chunk += ">\n";
            source.set_state_chunk("<SOURCE MIDI\n", chunk)?;
            // Make sure we don't have any association to some item on the timeline (or in
            // another slot) because that could lead to unpleasant surprises.
            source
                .remove_from_midi_pool()
                .map_err(|_| "couldn't unpool MIDI")?;
            source
        }
    };
    Ok(pcm_source.into_raw())
}

fn make_relative(project: Option<Project>, file: &Path) -> PathBuf {
    project
        .and_then(|p| p.make_path_relative_if_in_project_directory(file))
        .unwrap_or_else(|| file.to_owned())
}
