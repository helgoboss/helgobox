use crate::rt::supplier::{RecorderEquipment, WriteAudioRequest, WriteMidiRequest};
use crate::rt::{
    Clip, ClipChangedEvent, ClipPlayArgs, ClipPlayState, ClipProcessArgs, ClipRecordInput,
    ClipStopArgs, RecordBehavior, RelevantPlayStateChange, Slot, SlotProcessTransportChangeArgs,
    TransportChange,
};
use crate::timeline::{clip_timeline, HybridTimeline, Timeline, TimelineMoment};
use crate::ClipEngineResult;
use assert_no_alloc::assert_no_alloc;
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use playtime_api::ClipPlayStartTiming;
use reaper_high::{Project, Reaper};
use reaper_medium::{
    reaper_str, CustomPcmSource, DurationInBeats, DurationInSeconds, ExtendedArgs, GetPeakInfoArgs,
    GetSamplesArgs, Hz, LoadStateArgs, OwnedPcmSource, PcmSource, PeaksClearArgs, PlayState,
    PositionInSeconds, ProjectContext, PropertiesWindowArgs, ReaperStr, ReaperVolumeValue,
    SaveStateArgs, SetAvailableArgs, SetFileNameArgs, SetSourceArgs,
};
use std::error::Error;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

#[derive(Clone, Debug)]
pub struct SharedColumnSource(Arc<Mutex<ColumnSource>>);

#[derive(Clone, Debug)]
pub struct WeakColumnSource(Weak<Mutex<ColumnSource>>);

impl SharedColumnSource {
    pub fn new(column_source: ColumnSource) -> Self {
        Self(Arc::new(Mutex::new(column_source)))
    }

    pub fn lock(&self) -> MutexGuard<ColumnSource> {
        match self.0.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        }
    }

    pub fn downgrade(&self) -> WeakColumnSource {
        WeakColumnSource(Arc::downgrade(&self.0))
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl WeakColumnSource {
    pub fn upgrade(&self) -> Option<SharedColumnSource> {
        self.0.upgrade().map(SharedColumnSource)
    }
}

#[derive(Clone, Debug)]
pub struct ColumnSourceCommandSender {
    command_sender: Sender<ColumnSourceCommand>,
}

impl ColumnSourceCommandSender {
    pub fn new(command_sender: Sender<ColumnSourceCommand>) -> Self {
        Self { command_sender }
    }

    pub fn update_settings(&self, settings: ColumnSettings) {
        self.send_source_task(ColumnSourceCommand::UpdateSettings(settings));
    }

    pub fn fill_slot(&self, args: ColumnFillSlotArgs) {
        self.send_source_task(ColumnSourceCommand::FillSlot(args));
    }

    pub fn play_clip(&self, args: ColumnPlayClipArgs) {
        self.send_source_task(ColumnSourceCommand::PlayClip(args));
    }

    pub fn stop_clip(&self, args: ColumnStopClipArgs) {
        self.send_source_task(ColumnSourceCommand::StopClip(args));
    }

    pub fn set_clip_repeated(&self, args: ColumnSetClipRepeatedArgs) {
        self.send_source_task(ColumnSourceCommand::SetClipRepeated(args));
    }

    pub fn pause_clip(&self, index: usize) {
        let args = ColumnPauseClipArgs { index };
        self.send_source_task(ColumnSourceCommand::PauseClip(args));
    }

    pub fn seek_clip(&self, index: usize, desired_pos: UnitValue) {
        let args = ColumnSeekClipArgs { index, desired_pos };
        self.send_source_task(ColumnSourceCommand::SeekClip(args));
    }

    pub fn set_clip_volume(&self, index: usize, volume: ReaperVolumeValue) {
        let args = ColumnSetClipVolumeArgs { index, volume };
        self.send_source_task(ColumnSourceCommand::SetClipVolume(args));
    }

    fn send_source_task(&self, task: ColumnSourceCommand) {
        self.command_sender.try_send(task).unwrap();
    }
}

#[derive(Debug)]
pub enum ColumnSourceCommand {
    UpdateSettings(ColumnSettings),
    FillSlot(ColumnFillSlotArgs),
    PlayClip(ColumnPlayClipArgs),
    StopClip(ColumnStopClipArgs),
    PauseClip(ColumnPauseClipArgs),
    SeekClip(ColumnSeekClipArgs),
    SetClipVolume(ColumnSetClipVolumeArgs),
    SetClipRepeated(ColumnSetClipRepeatedArgs),
}

#[derive(Debug)]
pub struct ColumnSource {
    settings: ColumnSettings,
    // TODO-high Integrate into API
    mode: ClipColumnMode,
    slots: Vec<Slot>,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
    command_receiver: Receiver<ColumnSourceCommand>,
    event_sender: Sender<ColumnSourceEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    pub clip_play_start_timing: Option<ClipPlayStartTiming>,
}

impl ColumnSource {
    pub fn new(
        project: Option<Project>,
        command_receiver: Receiver<ColumnSourceCommand>,
        event_sender: Sender<ColumnSourceEvent>,
    ) -> Self {
        Self {
            settings: Default::default(),
            mode: Default::default(),
            // TODO-high We should probably make this higher so we don't need to allocate in the
            //  audio thread (or block the audio thread through allocation in the main thread).
            //  Or we find a mechanism to return a request for a newly allocated vector, release
            //  the mutex in the meanwhile and try a second time as soon as we have the allocation
            //  ready.
            slots: Vec::with_capacity(8),
            project,
            command_receiver,
            event_sender,
        }
    }

    fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        get_slot_mut_insert(&mut self.slots, args.index).fill(args.clip);
    }

    pub fn slot(&self, index: usize) -> ClipEngineResult<&Slot> {
        get_slot(&self.slots, index)
    }

    pub fn slot_mut(&mut self, index: usize) -> ClipEngineResult<&mut Slot> {
        self.slots.get_mut(index).ok_or(SLOT_DOESNT_EXIST)
    }

    pub fn play_clip(&mut self, args: ColumnPlayClipArgs) -> ClipEngineResult<()> {
        // TODO-high If column mode Song, suspend all other clips first.
        let clip_args = ClipPlayArgs {
            parent_start_timing: self
                .settings
                .clip_play_start_timing
                .unwrap_or(args.parent_start_timing),
            timeline: &args.timeline,
            ref_pos: args.ref_pos,
        };
        get_slot_mut(&mut self.slots, args.slot_index)?.play_clip(clip_args)
    }

    pub fn stop_clip(&mut self, args: ColumnStopClipArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.slot_index).stop_clip(args.clip_args)
    }

    pub fn set_clip_repeated(&mut self, args: ColumnSetClipRepeatedArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.index).set_clip_repeated(args.repeated)
    }

    pub fn clip_play_state(&self, index: usize) -> ClipEngineResult<ClipPlayState> {
        Ok(get_slot(&self.slots, index)?.clip()?.play_state())
    }

    pub fn record_clip(
        &mut self,
        index: usize,
        behavior: RecordBehavior,
        equipment: RecorderEquipment,
    ) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).record_clip(
            behavior,
            ClipRecordInput::Audio,
            self.project,
            equipment,
        )
    }

    pub fn pause_clip(&mut self, index: usize) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).pause_clip()
    }

    fn seek_clip(&mut self, index: usize, desired_pos: UnitValue) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).seek_clip(desired_pos)
    }

    pub fn clip_record_input(&self, index: usize) -> Option<ClipRecordInput> {
        get_slot(&self.slots, index).ok()?.clip_record_input()
    }

    pub fn write_clip_midi(
        &mut self,
        index: usize,
        request: WriteMidiRequest,
    ) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).write_clip_midi(request)
    }

    pub fn write_clip_audio(
        &mut self,
        index: usize,
        request: WriteAudioRequest,
    ) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).write_clip_audio(request)
    }

    fn set_clip_volume(
        &mut self,
        index: usize,
        volume: ReaperVolumeValue,
    ) -> ClipEngineResult<ClipChangedEvent> {
        get_slot_mut_insert(&mut self.slots, index).set_clip_volume(volume)
    }

    pub fn process_transport_change(&mut self, args: SlotProcessTransportChangeArgs) {
        let args = SlotProcessTransportChangeArgs {
            parent_clip_play_start_timing: self
                .settings
                .clip_play_start_timing
                .unwrap_or(args.parent_clip_play_start_timing),
            ..args
        };
        for slot in &mut self.slots {
            slot.process_transport_change(&args);
        }
    }

    fn get_num_channels(&self) -> Option<u32> {
        // TODO-high We should return the maximum channel count over all clips.
        //  In get_samples(), we should convert interleaved buffer accordingly.
        //  Probably a good idea to cache the max channel count.
        Some(2)
    }

    fn duration(&self) -> DurationInSeconds {
        DurationInSeconds::MAX
    }

    fn process_commands(&mut self) {
        while let Ok(task) = self.command_receiver.try_recv() {
            use ColumnSourceCommand::*;
            match task {
                UpdateSettings(s) => {
                    self.settings = s;
                }
                FillSlot(args) => {
                    self.fill_slot(args);
                }
                PlayClip(args) => {
                    let _ = self.play_clip(args);
                }
                StopClip(args) => {
                    let _ = self.stop_clip(args);
                }
                PauseClip(args) => {
                    let _ = self.pause_clip(args.index);
                }
                SetClipVolume(args) => {
                    let _ = self.set_clip_volume(args.index, args.volume);
                }
                SeekClip(args) => {
                    let _ = self.seek_clip(args.index, args.desired_pos);
                }
                SetClipRepeated(args) => {
                    let _ = self.set_clip_repeated(args);
                }
            }
        }
    }

    fn get_samples(&mut self, mut args: GetSamplesArgs) {
        assert_no_alloc(|| {
            self.process_commands();
            // Make sure that in any case, we are only queried once per time, without retries.
            // TODO-medium This mechanism of advancing the position on every call by
            //  the block duration relies on the fact that the preview
            //  register timeline calls us continuously and never twice per block.
            //  It would be better not to make that assumption and make this more
            //  stable by actually looking at the diff between the currently requested
            //  time_s and the previously requested time_s. If this diff is zero or
            //  doesn't correspond to the non-tempo-adjusted block duration, we know
            //  something is wrong.
            unsafe {
                args.block.set_samples_out(args.block.length());
            }
            // Get main timeline info
            let timeline = clip_timeline(self.project, false);
            if !timeline.is_running() {
                // Main timeline is paused. Don't play, we don't want to play the same buffer
                // repeatedly!
                // TODO-high Pausing main transport and continuing has timing issues.
                return;
            }
            // Get samples
            let timeline_cursor_pos = timeline.cursor_pos();
            let timeline_tempo = timeline.tempo_at(timeline_cursor_pos);
            // rt_debug!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
            //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
            for (row, slot) in self.slots.iter_mut().enumerate() {
                let mut inner_args = ClipProcessArgs {
                    block: args.block,
                    timeline: &timeline,
                    timeline_cursor_pos,
                    timeline_tempo,
                };
                // TODO-high Take care of mixing as soon as we implement Free mode.
                if let Ok(Some(changed_play_state)) = slot.process(&mut inner_args) {
                    let event = ColumnSourceEvent::ClipPlayStateChanged {
                        index: row,
                        play_state: changed_play_state,
                    };
                    self.event_sender.try_send(event).unwrap();
                }
            }
        });
        debug_assert_eq!(args.block.samples_out(), args.block.length());
    }

    fn extended(&mut self, args: ExtendedArgs) -> i32 {
        // TODO-medium Maybe implement PCM_SOURCE_EXT_NOTIFYPREVIEWPLAYPOS. This is the only
        //  extended call done by the preview register, at least for type WAVE.
        0
    }
}

#[derive(Debug)]
pub enum ClipColumnMode {
    /// Song mode.
    ///
    /// - Only one clip in the column can play at a certain point in time.
    /// - Clips are started/stopped if the corresponding scene is started/stopped.
    Song,
    /// Free mode.
    ///
    /// - Multiple clips can play simultaneously.
    /// - Clips are not started/stopped if the corresponding scene is started/stopped.
    Free,
}

impl Default for ClipColumnMode {
    fn default() -> Self {
        Self::Song
    }
}

impl CustomPcmSource for SharedColumnSource {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        unimplemented!()
    }

    fn is_available(&mut self) -> bool {
        unimplemented!()
    }

    fn set_available(&mut self, _: SetAvailableArgs) {
        unimplemented!()
    }

    fn get_type(&mut self) -> &ReaperStr {
        // This is not relevant for usage in preview registers, but it will be called.
        // TODO-medium Return something less misleading here.
        reaper_str!("WAVE")
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unimplemented!()
    }

    fn set_file_name(&mut self, _: SetFileNameArgs) -> bool {
        unimplemented!()
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        unimplemented!()
    }

    fn set_source(&mut self, _: SetSourceArgs) {
        unimplemented!()
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        self.lock().get_num_channels()
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        unimplemented!()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        self.lock().duration()
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        unimplemented!()
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        unimplemented!()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        unimplemented!()
    }

    fn properties_window(&mut self, _: PropertiesWindowArgs) -> i32 {
        unimplemented!()
    }

    fn get_samples(&mut self, mut args: GetSamplesArgs) {
        self.lock().get_samples(args)
    }

    fn get_peak_info(&mut self, _: GetPeakInfoArgs) {
        unimplemented!()
    }

    fn save_state(&mut self, _: SaveStateArgs) {
        unimplemented!()
    }

    fn load_state(&mut self, _: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unimplemented!()
    }

    fn peaks_clear(&mut self, _: PeaksClearArgs) {
        unimplemented!()
    }

    fn peaks_build_begin(&mut self) -> bool {
        unimplemented!()
    }

    fn peaks_build_run(&mut self) -> bool {
        unimplemented!()
    }

    fn peaks_build_finish(&mut self) {
        unimplemented!()
    }

    unsafe fn extended(&mut self, args: ExtendedArgs) -> i32 {
        self.lock().extended(args)
    }
}

#[derive(Debug)]
pub struct ColumnFillSlotArgs {
    pub index: usize,
    pub clip: Clip,
}

#[derive(Debug)]
pub struct ColumnPlayClipArgs {
    pub slot_index: usize,
    pub parent_start_timing: ClipPlayStartTiming,
    pub timeline: HybridTimeline,
    /// Set this if you already have the current timeline position or want to play a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
}

#[derive(Debug)]
pub struct ColumnStopClipArgs {
    pub slot_index: usize,
    pub clip_args: ClipStopArgs,
}

#[derive(Debug)]
pub struct ColumnPauseClipArgs {
    pub index: usize,
}

#[derive(Debug)]
pub struct ColumnSeekClipArgs {
    pub index: usize,
    pub desired_pos: UnitValue,
}

#[derive(Debug)]
pub struct ColumnSetClipVolumeArgs {
    pub index: usize,
    pub volume: ReaperVolumeValue,
}

#[derive(Debug)]
pub struct ColumnSetClipRepeatedArgs {
    pub index: usize,
    pub repeated: bool,
}

pub struct ColumnWithSlotArgs<'a> {
    pub index: usize,
    pub use_slot: &'a dyn Fn(),
}

fn get_slot(slots: &Vec<Slot>, index: usize) -> ClipEngineResult<&Slot> {
    slots.get(index).ok_or(SLOT_DOESNT_EXIST)
}

fn get_slot_mut(slots: &mut Vec<Slot>, index: usize) -> ClipEngineResult<&mut Slot> {
    slots.get_mut(index).ok_or(SLOT_DOESNT_EXIST)
}

const SLOT_DOESNT_EXIST: &str = "slot doesn't exist";

fn get_slot_mut_insert(slots: &mut Vec<Slot>, index: usize) -> &mut Slot {
    if index >= slots.len() {
        slots.resize_with(index + 1, Default::default);
    }
    slots.get_mut(index).unwrap()
}

#[derive(Clone, Debug)]
pub enum ColumnSourceEvent {
    ClipPlayStateChanged {
        index: usize,
        play_state: ClipPlayState,
    },
}

#[deprecated]
pub const FAKE_ROW_INDEX: usize = 0;
