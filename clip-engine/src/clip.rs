use std::error::Error;
use std::path::{Path, PathBuf};

use reaper_high::{Item, OwnedSource, Project, Reaper, ReaperSource};
use reaper_medium::{MidiImportBehavior, ReaperVolumeValue};
use serde::{Deserialize, Serialize};

use helgoboss_learn::UnitValue;

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}

/// Describes settings and contents of one clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Clip {
    #[serde(rename = "volume", default, skip_serializing_if = "is_default")]
    pub volume: ReaperVolumeValue,
    #[serde(rename = "repeat", default, skip_serializing_if = "is_default")]
    pub repeat: bool,
    #[serde(rename = "content", default, skip_serializing_if = "is_default")]
    pub content: Option<ClipContent>,
}

impl Default for Clip {
    fn default() -> Self {
        Self {
            volume: ReaperVolumeValue::ZERO_DB,
            repeat: false,
            content: None,
        }
    }
}

impl Clip {
    pub fn is_filled(&self) -> bool {
        self.content.is_some()
    }
}

/// Describes the content of a clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipContent {
    File { file: PathBuf },
    MidiChunk { chunk: String },
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
        let source_type = root_source.r#type();
        let item_project = item.project();
        enum Res {
            File(PathBuf),
            MidiChunk(String),
        }
        let res = if let Some(source_file) = root_source.file_name() {
            Res::File(source_file)
        } else if matches!(source_type.as_str(), "MIDI" | "MIDIPOOL") {
            if force_export_to_file {
                let project = item_project.unwrap_or_else(|| Reaper::get().current_project());
                let recording_path = project.recording_path();
                let take_name = active_take.name();
                let take_name_slug = slug::slugify(take_name);
                let unique_id = nanoid::nanoid!(8);
                let file_name = format!("{}-{}.mid", take_name_slug, unique_id);
                let source_file = recording_path.join(file_name);
                root_source
                    .export_to_file(&source_file)
                    .map_err(|_| "couldn't export MIDI source to file")?;
                Res::File(source_file)
            } else {
                Res::MidiChunk(root_source.state_chunk())
            }
        } else {
            return Err(format!("item source incompatible (type {})", source_type).into());
        };
        let content = match res {
            Res::File(file) => ClipContent::File {
                file: item_project
                    .and_then(|p| p.make_path_relative_if_in_project_directory(&file))
                    .unwrap_or(file),
            },
            Res::MidiChunk(chunk) => ClipContent::MidiChunk { chunk },
        };
        Ok(content)
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

/// Play state of a clip.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ClipPlayState {
    Stopped,
    ScheduledForPlay,
    Playing,
    Paused,
    ScheduledForStop,
    Recording,
}

impl ClipPlayState {
    /// Translates this play state into a feedback value.
    pub fn feedback_value(self) -> UnitValue {
        use ClipPlayState::*;
        match self {
            Stopped => UnitValue::MIN,
            ScheduledForPlay => UnitValue::new(0.75),
            Playing => UnitValue::MAX,
            Paused => UnitValue::new(0.5),
            ScheduledForStop => UnitValue::new(0.25),
            Recording => UnitValue::new(0.60),
        }
    }
}

#[derive(Debug)]
pub enum ClipChangedEvent {
    PlayState(ClipPlayState),
    ClipVolume(ReaperVolumeValue),
    ClipRepeat(bool),
    ClipPosition(UnitValue),
}
