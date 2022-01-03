use std::error::Error;
use std::path::{Path, PathBuf};

use reaper_high::{Item, OwnedSource, Project, Reaper, ReaperSource};
use reaper_medium::{MidiImportBehavior, ReaperVolumeValue};
use serde::{Deserialize, Serialize};

use crate::base::default_util::is_default;
use helgoboss_learn::UnitValue;

/// Describes settings and contents of one clip slot.
// TODO-high This data is more about the clip than about the slot, so we should call it
//  Clip. And SlotContent should be ClipContent.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SlotDescriptor {
    #[serde(rename = "volume", default, skip_serializing_if = "is_default")]
    pub volume: ReaperVolumeValue,
    #[serde(rename = "repeat", default, skip_serializing_if = "is_default")]
    pub repeat: bool,
    #[serde(rename = "content", default, skip_serializing_if = "is_default")]
    pub content: Option<SlotContent>,
}

impl Default for SlotDescriptor {
    fn default() -> Self {
        Self {
            volume: ReaperVolumeValue::ZERO_DB,
            repeat: false,
            content: None,
        }
    }
}

impl SlotDescriptor {
    pub fn is_filled(&self) -> bool {
        self.content.is_some()
    }
}

/// Describes the content of a clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SlotContent {
    File {
        #[serde(rename = "file")]
        file: PathBuf,
    },
}

impl SlotContent {
    /// Creates slot content based on the audio/MIDI file used by the given item.
    ///
    /// If the item uses pooled MIDI instead of a file, this method exports the MIDI data to a new
    /// file in the recording directory and uses that one.   
    pub fn from_item(item: Item) -> Result<Self, Box<dyn Error>> {
        let active_take = item.active_take().ok_or("item has no active take")?;
        let root_source = active_take
            .source()
            .ok_or("take has no source")?
            .root_source();
        let root_source = ReaperSource::new(root_source);
        let source_type = root_source.r#type();
        let item_project = item.project();
        let file = if let Some(source_file) = root_source.file_name() {
            source_file
        } else if source_type == "MIDI" {
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
            source_file
        } else {
            return Err(format!("item source incompatible (type {})", source_type).into());
        };
        let content = SlotContent::File {
            file: item_project
                .and_then(|p| p.make_path_relative_if_in_project_directory(&file))
                .unwrap_or(file),
        };
        Ok(content)
    }

    /// Returns the path to the file, if the clip slot content is file-based.
    pub fn file(&self) -> Option<&Path> {
        use SlotContent::*;
        match self {
            File { file } => Some(file),
        }
    }

    /// Creates a REAPER PCM source from this content.
    pub fn create_source(&self, project: Option<Project>) -> Result<OwnedSource, &'static str> {
        match self {
            SlotContent::File { file } => {
                let absolute_file = if file.is_relative() {
                    project
                        .ok_or("slot source given as relative file but without project")?
                        .make_path_absolute(file)
                        .ok_or("couldn't make clip source path absolute")?
                } else {
                    file.clone()
                };
                OwnedSource::from_file(&absolute_file, MidiImportBehavior::UsePreference)
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
        }
    }
}
