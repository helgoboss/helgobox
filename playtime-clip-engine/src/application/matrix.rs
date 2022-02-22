use crate::application::{ClipContent, ClipData, Column};
use crate::processing::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, keep_stretching, RecorderEquipment, StretchWorkerRequest,
};
use crate::processing::{
    Clip, ClipChangedEvent, ClipInfo, ClipPlayArgs, ClipPlayState, ClipStopArgs, ClipStopBehavior,
    ColumnFillSlotArgs, ColumnPlayClipArgs, ColumnSetClipRepeatedArgs, ColumnStopClipArgs,
    RealTimeClipMatrix, RealTimeClipMatrixCommand, RecordBehavior, RecordTiming,
    SharedColumnSource, SlotProcessTransportChangeArgs, TransportChange,
};
use crate::timeline::{clip_timeline, Timeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use reaper_high::{Guid, Item, Project, Track};
use reaper_medium::{Bpm, PositionInSeconds, ReaperVolumeValue};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::thread;
use std::thread::JoinHandle;

#[derive(Debug)]
pub struct Matrix<H> {
    handler: H,
    stretch_worker_sender: Sender<StretchWorkerRequest>,
    recorder_equipment: RecorderEquipment,
    columns: Vec<Column>,
    containing_track: Option<Track>,
    real_time_command_sender: Sender<RealTimeClipMatrixCommand>,
    worker_pool: WorkerPool,
}

#[derive(Debug, Default)]
struct WorkerPool {
    workers: Vec<Worker>,
}

#[derive(Debug)]
struct Worker {
    join_handle: Option<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn add_worker(&mut self, name: &str, f: impl FnOnce() + Send + 'static) {
        let join_handle = thread::Builder::new()
            .name(String::from(name))
            .spawn(f)
            .unwrap();
        let worker = Worker {
            join_handle: Some(join_handle),
        };
        self.workers.push(worker);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(join_handle) = self.join_handle.take() {
            let name = join_handle.thread().name().unwrap();
            debug!("Shutting down clip matrix worker \"{}\"...", name);
            join_handle.join().unwrap();
        }
    }
}

impl<H: ClipMatrixHandler> Matrix<H> {
    pub fn new(handler: H, containing_track: Option<Track>) -> (Self, RealTimeClipMatrix) {
        let (stretch_worker_sender, stretch_worker_receiver) = crossbeam_channel::bounded(500);
        let (recorder_request_sender, recorder_request_receiver) = crossbeam_channel::bounded(500);
        let (cache_request_sender, cache_request_receiver) = crossbeam_channel::bounded(500);
        let (pre_buffer_request_sender, pre_buffer_request_receiver) =
            crossbeam_channel::bounded(500);
        let (real_time_command_sender, real_time_command_receiver) =
            crossbeam_channel::bounded(500);
        let mut worker_pool = WorkerPool::default();
        worker_pool.add_worker("Playtime stretch worker", move || {
            keep_stretching(stretch_worker_receiver);
        });
        worker_pool.add_worker("Playtime recording worker", move || {
            keep_processing_recorder_requests(recorder_request_receiver);
        });
        worker_pool.add_worker("Playtime cache worker", move || {
            keep_processing_cache_requests(cache_request_receiver);
        });
        worker_pool.add_worker("Playtime pre-buffer worker", move || {
            keep_processing_pre_buffer_requests(pre_buffer_request_receiver);
        });
        let matrix = Self {
            handler,
            stretch_worker_sender,
            recorder_equipment: RecorderEquipment {
                recorder_request_sender,
                cache_request_sender,
                pre_buffer_request_sender,
            },
            columns: vec![],
            containing_track,
            real_time_command_sender,
            worker_pool,
        };
        let real_time_matrix = RealTimeClipMatrix::new(real_time_command_receiver);
        (matrix, real_time_matrix)
    }

    pub fn clear(&mut self) {
        // TODO-medium How about suspension?
        self.columns.clear();
        self.send_command_to_real_time_matrix(RealTimeClipMatrixCommand::Clear);
    }

    fn send_command_to_real_time_matrix(&self, command: RealTimeClipMatrixCommand) {
        self.real_time_command_sender.try_send(command).unwrap();
    }

    /// This is for loading slots the legacy way.
    pub fn load_slots_legacy(
        &mut self,
        descriptors: Vec<LegacySlotDescriptor>,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        self.clear();
        for desc in descriptors {
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
                    let source = desc.clip.content.create_source(project)?.into_raw();
                    Clip::from_source(source, project, self.recorder_equipment.clone())
                },
            });
            column.set_clip_repeated(ColumnSetClipRepeatedArgs {
                index: row,
                repeated: desc.clip.repeat,
            });
            self.send_command_to_real_time_matrix(RealTimeClipMatrixCommand::InsertColumn(
                desc.index,
                column.source(),
            ));
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
                    descriptor: column.clip_data(0)?,
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
        column.play_clip(args);
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
                timeline,
            },
        };
        column.stop_clip(args);
        Ok(())
    }

    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<(ClipLocation, ClipChangedEvent)> {
        self.columns
            .iter_mut()
            .enumerate()
            .flat_map(|(i, column)| {
                column
                    .poll(timeline_tempo)
                    .into_iter()
                    .map(move |(row, event)| (ClipLocation::new(i, row), event))
            })
            .collect()
    }

    pub fn toggle_repeat_legacy(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let event = get_column_mut(&mut self.columns, slot_index)?.toggle_clip_repeated(0)?;
        self.handler.notify_clip_changed(slot_index, event);
        Ok(())
    }

    pub fn clip_position_in_seconds(
        &self,
        slot_index: usize,
        timeline_tempo: Bpm,
    ) -> Option<PositionInSeconds> {
        get_column(&self.columns, slot_index)
            .ok()?
            .clip_position_in_seconds(0, timeline_tempo)
    }

    pub fn clip_play_state(&self, slot_index: usize) -> Option<ClipPlayState> {
        get_column(&self.columns, slot_index)
            .ok()?
            .clip_play_state(0)
    }

    pub fn clip_repeated(&self, slot_index: usize) -> Option<bool> {
        get_column(&self.columns, slot_index).ok()?.clip_repeated(0)
    }

    pub fn clip_volume(&self, slot_index: usize) -> Option<ReaperVolumeValue> {
        get_column(&self.columns, slot_index).ok()?.clip_volume(0)
    }

    pub fn clip_data(&self, slot_index: usize) -> Option<ClipData> {
        get_column(&self.columns, slot_index).ok()?.clip_data(0)
    }

    pub fn clip_info(&self, slot_index: usize) -> Option<ClipInfo> {
        get_column(&self.columns, slot_index).ok()?.clip_info(0)
    }

    pub fn process_transport_change(&mut self, change: TransportChange, project: Option<Project>) {
        let timeline = clip_timeline(project, true);
        let moment = timeline.capture_moment();
        let args = SlotProcessTransportChangeArgs {
            change,
            moment,
            timeline,
        };
        for column in &mut self.columns {
            column.process_transport_change(args.clone());
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
            self.recorder_equipment.clone(),
        )?;
        self.handler.request_recording_input(task);
        Ok(())
    }

    pub fn pause_clip_legacy(&mut self, slot_index: usize) -> Result<(), &'static str> {
        get_column_mut(&mut self.columns, slot_index)?.pause_clip(0);
        Ok(())
    }

    pub fn seek_clip_legacy(
        &mut self,
        slot_index: usize,
        position: UnitValue,
    ) -> Result<(), &'static str> {
        get_column_mut(&mut self.columns, slot_index)?.seek_clip(0, position);
        Ok(())
    }

    pub fn set_clip_volume_legacy(
        &mut self,
        slot_index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<(), &'static str> {
        get_column_mut(&mut self.columns, slot_index)?.set_clip_volume(0, volume);
        Ok(())
    }

    pub fn proportional_clip_position_legacy(&self, slot_index: usize) -> Option<UnitValue> {
        get_column(&self.columns, slot_index)
            .ok()?
            .proportional_clip_position(0)
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
    pub clip: ClipData,
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
    pub descriptor: ClipData,
}

pub struct ClipLocation {
    pub column: usize,
    pub row: usize,
}

impl ClipLocation {
    pub fn new(column: usize, row: usize) -> Self {
        Self { column, row }
    }
}
