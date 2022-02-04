use reaper_high::{Item, OwnedSource, Project, Reaper, ReaperSource};
use reaper_medium::MidiImportBehavior;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::{Path, PathBuf};

/// Describes the content of a clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipContent {
    File { file: PathBuf },
    MidiChunk { chunk: String },
}

pub enum CreateClipContentMode {
    AllowEmbeddedData,
    ForceExportToFile { file_base_name: String },
}

impl ClipContent {
    /// Creates slot content based on the audio/MIDI file used by the given item.
    ///
    /// If the item uses pooled MIDI instead of a file, this method exports the MIDI data to a new
    /// file in the recording directory and uses that one.   
    pub fn from_item(item: Item, force_export_to_file: bool) -> Result<Self, Box<dyn Error>> {
        let active_take = item.active_take().ok_or("item has no active take")?;
        let root_source = active_take
            .source()
            .ok_or("take has no source")?
            .root_source();
        let root_source = ReaperSource::new(root_source);
        use CreateClipContentMode::*;
        let mode = if force_export_to_file {
            ForceExportToFile {
                file_base_name: active_take.name(),
            }
        } else {
            AllowEmbeddedData
        };
        Self::from_reaper_source(&root_source, mode, item.project())
    }

    pub fn from_reaper_source(
        source: &ReaperSource,
        mode: CreateClipContentMode,
        project: Option<Project>,
    ) -> Result<Self, Box<dyn Error>> {
        let source_type = source.r#type();
        let content = if let Some(source_file) = source.file_name() {
            Self::from_file(project, source_file)
        } else if matches!(source_type.as_str(), "MIDI" | "MIDIPOOL") {
            use CreateClipContentMode::*;
            match mode {
                AllowEmbeddedData => Self::from_midi_chunk(source.state_chunk()),
                ForceExportToFile { file_base_name } => {
                    let project = project.unwrap_or_else(|| Reaper::get().current_project());
                    let recording_path = project.recording_path();
                    let name_slug = slug::slugify(file_base_name);
                    let unique_id = nanoid::nanoid!(8);
                    let file_name = format!("{}-{}.mid", name_slug, unique_id);
                    let source_file = recording_path.join(file_name);
                    source
                        .export_to_file(&source_file)
                        .map_err(|_| "couldn't export MIDI source to file")?;
                    Self::from_file(Some(project), source_file)
                }
            }
        } else {
            return Err(format!("item source incompatible (type {})", source_type).into());
        };
        Ok(content)
    }

    /// Takes care of making the path project-relative (if a project is given).
    pub fn from_file(project: Option<Project>, file: PathBuf) -> Self {
        Self::File {
            file: make_relative(project, file),
        }
    }

    pub fn from_midi_chunk(chunk: String) -> Self {
        Self::MidiChunk { chunk }
    }

    /// Returns the path to the file, if the clip slot content is file-based.
    pub fn file(&self) -> Option<&Path> {
        use ClipContent::*;
        match self {
            File { file } => Some(file),
            MidiChunk { .. } => None,
        }
    }

    /// Creates a REAPER PCM source from this content.
    pub fn create_source(&self, project: Option<Project>) -> Result<OwnedSource, &'static str> {
        match self {
            ClipContent::File { file } => {
                let absolute_file = if file.is_relative() {
                    project
                        .ok_or("slot source given as relative file but without project")?
                        .make_path_absolute(&file)
                        .ok_or("couldn't make clip source path absolute")?
                } else {
                    file.clone()
                };
                // TODO-high This is very preference-dependent. I guess we should make this stable
                //  and not allow too many options, for the sake of sanity. In-project MIDI only?
                //  Latest when we do overdub, we probably want in-project MIDI. Plus, clips are
                //  short mostly, so why not.
                OwnedSource::from_file(&absolute_file, MidiImportBehavior::UsePreference)
            }
            ClipContent::MidiChunk { chunk } => {
                let mut source = OwnedSource::from_type("MIDI")?;
                let mut chunk = chunk.clone();
                chunk += ">\n";
                source.set_state_chunk("<SOURCE MIDI\n", chunk)?;
                // Make sure we don't have any association to some item on the timeline (or in
                // another slot) because that could lead to unpleasant surprises.
                source
                    .remove_from_midi_pool()
                    .map_err(|_| "couldn't unpool MIDI")?;
                Ok(source)
            }
        }
    }
}

fn make_relative(project: Option<Project>, file: PathBuf) -> PathBuf {
    project
        .and_then(|p| p.make_path_relative_if_in_project_directory(&file))
        .unwrap_or(file)
}
