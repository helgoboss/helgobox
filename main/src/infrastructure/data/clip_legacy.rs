use crate::application::{TargetCategory, VirtualTrackType};
use crate::base::notification;
use crate::domain::{ReaperTargetType, TransportAction};
use crate::infrastructure::data::{deserialize_track, MappingModelData, TrackDeserializationInput};
use base::default_util::is_default;
use playtime_api::persistence::SourceOrigin;
use reaper_high::{Guid, Track};
use reaper_medium::ReaperVolumeValue;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(super) fn create_clip_matrix_from_legacy_slots(
    slots: &[QualifiedSlotDescriptor],
    main_mappings: &[MappingModelData],
    controller_mappings: &[MappingModelData],
    containing_track: Option<&Track>,
) -> Result<playtime_api::persistence::Matrix, &'static str> {
    use playtime_api::persistence as api;
    let matrix = api::Matrix {
        columns: {
            let api_columns: Result<Vec<_>, &'static str> = slots
                .iter()
                .map(|desc| {
                    let output =
                        determine_legacy_clip_track(desc.index, main_mappings, controller_mappings);
                    let api_column = api::Column {
                        id: Default::default(),
                        name: None,
                        clip_play_settings: api::ColumnClipPlaySettings {
                            track: output
                                .resolve_track(containing_track.cloned())?
                                .map(|t| api::TrackId::new(t.guid().to_string_without_braces())),
                            ..Default::default()
                        },
                        clip_record_settings: Default::default(),
                        slots: {
                            let api_clip = api::Clip {
                                id: Default::default(),
                                name: None,
                                source: match desc.descriptor.content.clone() {
                                    ClipContent::File { file } => {
                                        api::Source::File(api::FileSource { path: file })
                                    }
                                    ClipContent::MidiChunk { chunk } => {
                                        api::Source::MidiChunk(api::MidiChunkSource { chunk })
                                    }
                                },
                                frozen_source: None,
                                active_source: SourceOrigin::Normal,
                                time_base: api::ClipTimeBase::Time,
                                start_timing: None,
                                stop_timing: None,
                                looped: desc.descriptor.repeat,
                                volume: api::Db::new(0.0).unwrap(),
                                color: api::ClipColor::PlayTrackColor,
                                dynamic_section: Default::default(),
                                fixed_section: Default::default(),
                                audio_settings: Default::default(),
                                midi_settings: Default::default(),
                            };
                            let api_slot = api::Slot {
                                id: Default::default(),
                                // In the previous clip system, we had only one dimension.
                                row: 0,
                                clip_old: None,
                                clips: Some(vec![api_clip]),
                            };
                            Some(vec![api_slot])
                        },
                    };
                    Ok(api_column)
                })
                .collect();
            Some(api_columns?)
        },
        ..Default::default()
    };
    Ok(matrix)
}

fn determine_legacy_clip_track(
    slot_index: usize,
    main_mappings: &[MappingModelData],
    controller_mappings: &[MappingModelData],
) -> LegacyClipOutput {
    let mut candidates: Vec<_> = main_mappings
        .iter()
        .chain(controller_mappings.iter())
        .filter_map(|m| {
            if m.target.category == TargetCategory::Reaper
                && m.target.r#type == ReaperTargetType::ClipTransport
                && matches!(m.target.transport_action, TransportAction::PlayPause | TransportAction::PlayStop)
                && m.target.slot_index == slot_index
            {
                let input = TrackDeserializationInput {
                    track_data: &m.target.track_data,
                    clip_column: &m.target.clip_column,
                };
                let prop_values = deserialize_track(input);
                use VirtualTrackType::*;
                let t = match prop_values.r#type {
                    This => LegacyClipOutput::ThisTrack,
                    Selected | AllSelected | Dynamic => {
                        warn_about_legacy_clip_loss(slot_index, "The clip play target used track \"Selected\", \"All selected\" or \"Dynamic\" which is not supported anymore. Falling back to playing slot on \"This\" track.");
                        LegacyClipOutput::ThisTrack
                    },
                    Master => LegacyClipOutput::MasterTrack,
                    Unit => return None,
                    ById => if let Some(id) = prop_values.id { LegacyClipOutput::TrackById(id) } else {
                        LegacyClipOutput::ThisTrack
                    },
                    ByName => LegacyClipOutput::TrackByName(prop_values.name),
                    AllByName => {
                        warn_about_legacy_clip_loss(slot_index, "The clip play target used track \"All named\" which is not supported anymore. Falling back to identifying track by name.");
                        LegacyClipOutput::TrackByName(prop_values.name)
                    },
                    ByIndex => LegacyClipOutput::TrackByIndex(prop_values.index),
                    ByIdOrName => if let Some(id) = prop_values.id {
                        LegacyClipOutput::TrackById(id)
                    } else {
                        LegacyClipOutput::TrackByName(prop_values.name)
                    },
                    // We didn't have this before.
                    FromClipColumn | ByIndexTcp | ByIndexMcp | DynamicTcp | DynamicMcp=> return None
                };
                Some(t)
            } else {
                None
            }
        }).collect();
    if candidates.len() > 1 {
        warn_about_legacy_clip_loss(slot_index, "This clip was referred to by multiple clip play targets. Only the first one will be taken into account.");
    }
    let res = if let Some(first) = candidates.drain(..).next() {
        first
    } else {
        warn_about_legacy_clip_loss(slot_index, "There was no corresponding play target for this clip. Clip will be played on the master track instead.");
        LegacyClipOutput::MasterTrack
    };
    res
}

enum LegacyClipOutput {
    MasterTrack,
    ThisTrack,
    TrackById(Guid),
    TrackByIndex(u32),
    TrackByName(String),
}

impl LegacyClipOutput {
    fn resolve_track(
        &self,
        containing_track: Option<Track>,
    ) -> Result<Option<Track>, &'static str> {
        use LegacyClipOutput::*;
        let containing_track = containing_track.ok_or(
            "track-based columns are not supported when clip engine runs in monitoring FX chain",
        );
        let track = match self {
            MasterTrack => Some(containing_track?.project().master_track()?),
            ThisTrack => Some(containing_track?),
            TrackById(id) => {
                let track = containing_track?.project().track_by_guid(id)?;
                if track.is_available() {
                    Some(track)
                } else {
                    None
                }
            }
            TrackByIndex(index) => containing_track?.project().track_by_index(*index),
            TrackByName(name) => containing_track?
                .project()
                .tracks()
                .find(|t| t.name().map(|n| n.to_str() == name).unwrap_or(false)),
        };
        Ok(track)
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub(super) struct QualifiedSlotDescriptor {
    #[serde(rename = "index")]
    pub index: usize,
    #[serde(flatten)]
    pub descriptor: ClipData,
}

/// Describes settings and contents of one clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub(super) struct ClipData {
    #[serde(rename = "volume", default, skip_serializing_if = "is_default")]
    pub volume: ReaperVolumeValue,
    #[serde(rename = "repeat", default, skip_serializing_if = "is_default")]
    pub repeat: bool,
    #[serde(rename = "content")]
    pub content: ClipContent,
}

/// Describes the content of a clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ClipContent {
    File { file: PathBuf },
    MidiChunk { chunk: String },
}

fn warn_about_legacy_clip_loss(slot_index: usize, msg: &str) {
    notification::warn(format!(
        "\
        You have loaded a preset that makes use of the experimental clip engine contained in \
        older ReaLearn versions. This engine is now legacy and replaced by a new engine. The new \
        engine is better in many ways but doesn't support some of the more exotic features of the \
        old engine. In particular, clip {} will probably behave differently now. Details: {}
        ",
        slot_index + 1,
        msg
    ))
}
