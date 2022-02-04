use crate::{
    clip_timeline, keep_stretching, ClipChangedEvent, ClipContent, ClipPlayArgs, ClipSlot,
    ClipStopArgs, ClipStopBehavior, Column, ColumnFillSlotArgs, ColumnPlayClipArgs,
    ColumnPollSlotArgs, ColumnSetClipRepeatedArgs, ColumnStopClipArgs, LegacyClip, NewClip,
    RecordArgs, SharedRegister, Slot, SlotPlayOptions, SlotPollArgs, SlotStopBehavior,
    StretchWorkerRequest, Timeline, TransportChange,
};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use reaper_high::{Guid, Item, Project, Reaper, Track};
use reaper_medium::{Bpm, PositionInSeconds, ReaperVolumeValue};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug)]
pub struct ClipMatrix<H> {
    handler: H,
    clip_slots: Vec<ClipSlot>,
    /// To communicate with the time stretching worker.
    stretch_worker_sender: Sender<StretchWorkerRequest>,
    columns: Vec<Column>,
    containing_track: Option<Track>,
}

impl<H: ClipMatrixHandler> ClipMatrix<H> {
    pub fn new(handler: H, containing_track: Option<Track>) -> Self {
        let (stretch_worker_sender, stretch_worker_receiver) = crossbeam_channel::bounded(500);
        std::thread::spawn(move || {
            keep_stretching(stretch_worker_receiver);
        });
        Self {
            handler,
            clip_slots: (0..8).map(ClipSlot::new).collect(),
            stretch_worker_sender,
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
                    NewClip::new(source, project)
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
        let column = get_column_mut(&mut self.columns, slot_index).ok()?;
        let args = ColumnPollSlotArgs {
            index: 0,
            slot_args: SlotPollArgs { timeline_tempo },
        };
        column.poll_slot(args)
    }

    pub fn toggle_repeat_legacy(&mut self, slot_index: usize) -> Result<(), &'static str> {
        let column = get_column_mut(&mut self.columns, slot_index)?;
        let event = column.toggle_clip_repeated(0)?;
        self.handler.notify_clip_changed(slot_index, event);
        Ok(())
    }

    pub fn with_slot_legacy<R>(
        &self,
        slot_index: usize,
        f: impl FnOnce(&Slot) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        let column = get_column(&self.columns, slot_index)?;
        column.with_slot(0, f)
    }

    pub fn process_transport_change(&mut self, change: TransportChange, project: Option<Project>) {
        let timeline = clip_timeline(project, true);
        let moment = timeline.capture_moment();
        for slot in self.clip_slots.iter_mut() {
            slot.process_transport_change(change, moment, &timeline)
                .unwrap();
        }
    }

    pub fn fill_slot_by_user(
        &mut self,
        slot_index: usize,
        content: ClipContent,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        get_slot_mut(&mut self.clip_slots, slot_index)?.fill_by_user(
            content,
            project,
            &self.stretch_worker_sender,
        )?;
        self.handler.notify_slot_contents_changed();
        Ok(())
    }

    pub fn fill_slot_with_item_source(
        &mut self,
        slot_index: usize,
        item: Item,
    ) -> Result<(), Box<dyn Error>> {
        let slot = get_slot_mut(&mut self.clip_slots, slot_index)?;
        let content = ClipContent::from_item(item, false)?;
        slot.fill_by_user(content, item.project(), &self.stretch_worker_sender)?;
        self.handler.notify_slot_contents_changed();
        Ok(())
    }

    pub fn record_clip(
        &mut self,
        slot_index: usize,
        project: Project,
        args: RecordArgs,
    ) -> Result<(), &'static str> {
        let slot = get_slot_mut(&mut self.clip_slots, slot_index)?;
        let register = slot.record(project, &self.stretch_worker_sender, args)?;
        let task = ClipRecordTask { register, project };
        self.handler.request_recording_input(task);
        Ok(())
    }

    pub fn pause_clip(&mut self, slot_index: usize) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?.pause()
    }

    pub fn seek_slot(
        &mut self,
        slot_index: usize,
        position: UnitValue,
    ) -> Result<(), &'static str> {
        let event = self
            .get_slot_mut(slot_index)?
            .set_proportional_position(position)?;
        if let Some(event) = event {
            self.handler.notify_clip_changed(slot_index, event);
        }
        Ok(())
    }

    pub fn set_volume(
        &mut self,
        slot_index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<(), &'static str> {
        let event = self.get_slot_mut(slot_index)?.set_volume(volume);
        self.handler.notify_clip_changed(slot_index, event);
        Ok(())
    }

    pub fn set_clip_tempo_factor(
        &mut self,
        slot_index: usize,
        tempo_factor: f64,
    ) -> Result<(), &'static str> {
        self.get_slot_mut(slot_index)?
            .set_tempo_factor(tempo_factor);
        Ok(())
    }

    fn get_slot_mut(&mut self, slot_index: usize) -> Result<&mut ClipSlot, &'static str> {
        self.clip_slots.get_mut(slot_index).ok_or("no such slot")
    }
}

fn get_slot_mut(slots: &mut [ClipSlot], index: usize) -> Result<&mut ClipSlot, &'static str> {
    slots.get_mut(index).ok_or("no such slot")
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
    pub register: SharedRegister,
    pub project: Project,
}

pub trait ClipMatrixHandler {
    fn request_recording_input(&self, task: ClipRecordTask);
    fn notify_slot_contents_changed(&mut self);
    fn notify_clip_changed(&self, slot_index: usize, event: ClipChangedEvent);
}
