use crate::{
    clip_timeline, keep_processing_recorder_requests, keep_stretching, Clip, ClipChangedEvent,
    ClipContent, ClipPlayArgs, ClipStopArgs, ClipStopBehavior, Column, ColumnFillSlotArgs,
    ColumnPlayClipArgs, ColumnPollSlotArgs, ColumnSetClipRepeatedArgs, ColumnStopClipArgs,
    LegacyClip, RecordBehavior, RecordTiming, RecorderRequest, SharedColumnSource, Slot,
    SlotPollArgs, SlotProcessTransportChangeArgs, StretchWorkerRequest, Timeline, TransportChange,
};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use reaper_high::{Guid, Item, Project, Reaper, Track};
use reaper_medium::{Bpm, PositionInSeconds, ReaperVolumeValue};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::thread;

#[derive(Debug)]
pub struct ClipMatrix<H> {
    handler: H,
    /// To communicate with the time stretching worker.
    stretch_worker_sender: Sender<StretchWorkerRequest>,
    recorder_request_sender: Sender<RecorderRequest>,
    columns: Vec<Column>,
    containing_track: Option<Track>,
}

impl<H: ClipMatrixHandler> ClipMatrix<H> {
    pub fn new(handler: H, containing_track: Option<Track>) -> Self {
        let (stretch_worker_sender, stretch_worker_receiver) = crossbeam_channel::bounded(500);
        let (recorder_request_sender, recorder_request_receiver) = crossbeam_channel::bounded(500);
        thread::Builder::new()
            .name(String::from("Playtime stretch worker"))
            .spawn(move || {
                keep_stretching(stretch_worker_receiver);
            });
        thread::Builder::new()
            .name(String::from("Playtime record worker"))
            .spawn(move || {
                keep_processing_recorder_requests(recorder_request_receiver);
            });
        Self {
            handler,
            stretch_worker_sender,
            recorder_request_sender,
            columns: vec![],
            containing_track,
        }
    }

    pub fn clear(&mut self) {
        // TODO-medium How about suspension?
        self.columns.clear();
    }

    /// This is for loading slots the legacy way.
    pub fn load_slots_legacy(
        &mut self,
        descriptors: Vec<LegacySlotDescriptor>,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        self.clear();
        for desc in descriptors {
            let content = match desc.clip.content {
                None => continue,
                Some(c) => c,
            };
            let resolved_track = if let Some(track) = self.containing_track.as_ref() {
                desc.output.resolve_track(track.clone())
            } else {
                None
            };
            let mut column = Column::new(resolved_track);
            let row = 0;
            column.fill_slot(ColumnFillSlotArgs {
                index: row,
                clip: {
                    let source = content.create_source(project)?.into_raw();
                    Clip::from_source(source, project, self.recorder_request_sender.clone())
                },
            });
            column.set_clip_repeated(ColumnSetClipRepeatedArgs {
                index: row,
                repeated: desc.clip.repeat,
            });
            self.columns.push(column);
        }
        self.handler.notify_slot_contents_changed();
        Ok(())
    }

    pub fn filled_slot_descriptors_legacy(&self) -> Vec<QualifiedSlotDescriptor> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(i, column)| {
                Some(QualifiedSlotDescriptor {
                    index: i,
                    descriptor: column
                        .with_slot(0, |slot| {
                            Ok(slot
                                .clip()?
                                .descriptor_legacy()
                                .ok_or("clip didn't deliver descriptor")?)
                        })
                        .ok()?,
                })
            })
            .collect()
    }

    pub fn play_clip_legacy(
        &mut self,
        project: Project,
        slot_index: usize,
        track: Option<Track>,
        options: SlotPlayOptions,
    ) -> Result<(), &'static str> {
        let column = get_column_mut(&mut self.columns, slot_index)?;
        let args = ColumnPlayClipArgs {
            index: 0,
            clip_args: ClipPlayArgs {
                from_bar: if options.next_bar {
                    let timeline = clip_timeline(Some(project), false);
                    Some(timeline.next_bar_at(timeline.cursor_pos()))
                } else {
                    None
                },
            },
        };
        column.play_clip(args)?;
        Ok(())
    }

    /// If repeat is not enabled and `immediately` is false, this has essentially no effect.
    pub fn stop_clip_legacy(
        &mut self,
        slot_index: usize,
        stop_behavior: SlotStopBehavior,
        project: Project,
    ) -> Result<(), &'static str> {
        let column = get_column_mut(&mut self.columns, slot_index)?;
        let timeline = clip_timeline(Some(project), false);
        let args = ColumnStopClipArgs {
            index: 0,
            clip_args: ClipStopArgs {
                stop_behavior: match stop_behavior {
                    SlotStopBehavior::Immediately => ClipStopBehavior::Immediately,
                    SlotStopBehavior::EndOfClip => ClipStopBehavior::EndOfClip,
                },
                timeline_cursor_pos: timeline.cursor_pos(),
                timeline: &timeline,
            },
        };
        column.stop_clip(args)?;
        Ok(())
    }

    /// Detects clips that are finished playing and invokes a stop feedback event if not looped.
    pub fn poll_slot_legacy(
        &mut self,
        slot_index: usize,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
    ) -> Option<ClipChangedEvent> {
        let args = ColumnPollSlotArgs {
            index: 0,
            slot_args: SlotPollArgs { timeline_tempo },
        };
        get_column_mut(&mut self.columns, slot_index)
            .ok()?
            .poll_slot(args)
    }

    pub fn toggle_repeat_legacy(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let event = get_column_mut(&mut self.columns, slot_index)?.toggle_clip_repeated(0)?;
        self.handler.notify_clip_changed(slot_index, event);
        Ok(())
    }

    pub fn with_slot_legacy<R>(
        &self,
        slot_index: usize,
        f: impl FnOnce(&Slot) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        get_column(&self.columns, slot_index)?.with_slot(0, f)
    }

    pub fn process_transport_change(&mut self, change: TransportChange, project: Option<Project>) {
        let timeline = clip_timeline(project, true);
        let moment = timeline.capture_moment();
        let args = SlotProcessTransportChangeArgs {
            change,
            moment,
            timeline: &timeline,
        };
        for column in &mut self.columns {
            column.process_transport_change(&args);
        }
    }

    pub fn record_clip_legacy(
        &mut self,
        slot_index: usize,
        project: Project,
        args: RecordArgs,
    ) -> Result<(), &'static str> {
        let behavior = match args.kind {
            RecordKind::Normal {
                play_after,
                timing,
                detect_downbeat,
            } => RecordBehavior::Normal {
                play_after,
                timing: match timing {
                    ClipRecordTiming::StartImmediatelyStopOnDemand => RecordTiming::Unsynced,
                    ClipRecordTiming::StartOnBarStopOnDemand { start_bar } => {
                        RecordTiming::Synced {
                            start_bar,
                            end_bar: None,
                        }
                    }
                    ClipRecordTiming::StartOnBarStopOnBar {
                        start_bar,
                        bar_count,
                    } => RecordTiming::Synced {
                        start_bar,
                        end_bar: Some(start_bar + bar_count as i32),
                    },
                },
                detect_downbeat,
            },
            RecordKind::MidiOverdub => RecordBehavior::MidiOverdub,
        };
        let task = get_column_mut(&mut self.columns, slot_index)?.record_clip(
            0,
            behavior,
            self.recorder_request_sender.clone(),
        )?;
        self.handler.request_recording_input(task);
        Ok(())
    }

    pub fn pause_clip_legacy(&mut self, slot_index: usize) -> Result<(), &'static str> {
        get_column_mut(&mut self.columns, slot_index)?.pause_clip(0)
    }

    pub fn seek_clip_legacy(
        &mut self,
        slot_index: usize,
        position: UnitValue,
    ) -> Result<(), &'static str> {
        get_column_mut(&mut self.columns, slot_index)?.seek_clip(0, position)
    }

    pub fn set_clip_volume_legacy(
        &mut self,
        slot_index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<(), &'static str> {
        let event = get_column_mut(&mut self.columns, slot_index)?.set_clip_volume(0, volume)?;
        self.handler.notify_clip_changed(slot_index, event);
        Ok(())
    }

    pub fn fill_slot_with_item_source(
        &mut self,
        slot_index: usize,
        item: Item,
    ) -> Result<(), Box<dyn Error>> {
        // let slot = get_slot_mut(&mut self.clip_slots, slot_index)?;
        // let content = ClipContent::from_item(item, false)?;
        // slot.fill_by_user(content, item.project(), &self.stretch_worker_sender)?;
        // self.handler.notify_slot_contents_changed();
        let content = ClipContent::from_item(item, false)?;
        dbg!(content);
        Ok(())
    }
}

fn get_column(columns: &[Column], index: usize) -> Result<&Column, &'static str> {
    columns.get(index).ok_or(NO_SUCH_COLUMN)
}

fn get_column_mut(columns: &mut [Column], index: usize) -> Result<&mut Column, &'static str> {
    columns.get_mut(index).ok_or(NO_SUCH_COLUMN)
}

const NO_SUCH_COLUMN: &str = "no such column";

pub struct LegacySlotDescriptor {
    pub output: LegacyClipOutput,
    pub index: usize,
    pub clip: LegacyClip,
}

pub enum LegacyClipOutput {
    MasterTrack,
    ThisTrack,
    TrackById(Guid),
    TrackByIndex(u32),
    TrackByName(String),
    HardwareOutput,
}

impl LegacyClipOutput {
    fn resolve_track(&self, containing_track: Track) -> Option<Track> {
        use LegacyClipOutput::*;
        match self {
            MasterTrack => Some(containing_track.project().master_track()),
            ThisTrack => Some(containing_track),
            TrackById(id) => {
                let track = containing_track.project().track_by_guid(id);
                if track.is_available() {
                    Some(track)
                } else {
                    None
                }
            }
            TrackByIndex(index) => containing_track.project().track_by_index(*index),
            TrackByName(name) => containing_track
                .project()
                .tracks()
                .find(|t| t.name().map(|n| n.to_str() == name).unwrap_or(false)),
            HardwareOutput => None,
        }
    }
}

#[derive(Debug)]
pub struct ClipRecordTask {
    pub column_source: SharedColumnSource,
    pub slot_index: usize,
}

pub trait ClipMatrixHandler {
    fn request_recording_input(&self, task: ClipRecordTask);
    fn notify_slot_contents_changed(&mut self);
    fn notify_clip_changed(&self, slot_index: usize, event: ClipChangedEvent);
}

#[derive(Copy, Clone, Debug)]
pub enum ClipRecordTiming {
    StartImmediatelyStopOnDemand,
    StartOnBarStopOnDemand { start_bar: i32 },
    StartOnBarStopOnBar { start_bar: i32, bar_count: u32 },
}

// TODO-medium Evolved into a perfect duplicate of ClipStopTime
/// Defines how to stop the clip.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SlotStopBehavior {
    Immediately,
    EndOfClip,
}
/// Contains instructions how to play a clip.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SlotPlayOptions {
    /// Syncs with timeline.
    pub next_bar: bool,
    pub buffered: bool,
}

#[derive(Copy, Clone)]
pub struct RecordArgs {
    pub kind: RecordKind,
}

#[derive(Copy, Clone)]
pub enum RecordKind {
    Normal {
        play_after: bool,
        timing: ClipRecordTiming,
        detect_downbeat: bool,
    },
    MidiOverdub,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QualifiedSlotDescriptor {
    #[serde(rename = "index")]
    pub index: usize,
    #[serde(flatten)]
    pub descriptor: LegacyClip,
}
