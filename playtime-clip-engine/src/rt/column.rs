use crate::mutex_util::non_blocking_lock;
use crate::rt::supplier::{
    ChainPreBufferRequest, MaterialInfo, PreBufferRequest, RecorderEquipment, WriteAudioRequest,
    WriteMidiRequest,
};
use crate::rt::{
    AudioBufMut, Clip, ClipPlayArgs, ClipPlayState, ClipProcessArgs, ClipRecordInput, ClipStopArgs,
    OwnedAudioBuffer, RecordBehavior, Slot, SlotProcessTransportChangeArgs,
};
use crate::timeline::{clip_timeline, HybridTimeline, Timeline};
use crate::ClipEngineResult;
use assert_no_alloc::assert_no_alloc;
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use playtime_api::{
    AudioTimeStretchMode, ClipPlayStartTiming, ClipPlayStopTiming, ColumnPlayMode, Db,
    VirtualResampleMode,
};
use reaper_high::Project;
use reaper_medium::{
    reaper_str, CustomPcmSource, DurationInBeats, DurationInSeconds, ExtendedArgs, GetPeakInfoArgs,
    GetSamplesArgs, Hz, LoadStateArgs, OwnedPcmSource, PcmSource, PeaksClearArgs,
    PositionInSeconds, PropertiesWindowArgs, ReaperStr, SaveStateArgs, SetAvailableArgs,
    SetFileNameArgs, SetSourceArgs,
};
use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// Only such methods are public which are allowed to use from real-time threads. Other ones
/// are private and called from the method that processes the incoming commands.
#[derive(Debug)]
pub struct Column {
    settings: ColumnSettings,
    slots: Vec<Slot>,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
    command_receiver: Receiver<ColumnCommand>,
    event_sender: Sender<ColumnEvent>,
    /// Enough reserved memory to hold one audio block of an arbitrary size.
    mix_buffer_chunk: Vec<f64>,
    timeline_was_paused_in_last_block: bool,
}

#[derive(Clone, Debug)]
pub struct SharedColumn(Arc<Mutex<Column>>);

#[derive(Clone, Debug)]
pub struct WeakColumn(Weak<Mutex<Column>>);

impl SharedColumn {
    pub fn new(column_source: Column) -> Self {
        Self(Arc::new(Mutex::new(column_source)))
    }

    pub fn lock(&self) -> MutexGuard<Column> {
        non_blocking_lock(&self.0, "real-time column")
    }

    pub fn downgrade(&self) -> WeakColumn {
        WeakColumn(Arc::downgrade(&self.0))
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl WeakColumn {
    pub fn upgrade(&self) -> Option<SharedColumn> {
        self.0.upgrade().map(SharedColumn)
    }
}

#[derive(Clone, Debug)]
pub struct ColumnCommandSender {
    command_sender: Sender<ColumnCommand>,
}

impl ColumnCommandSender {
    pub fn new(command_sender: Sender<ColumnCommand>) -> Self {
        Self { command_sender }
    }

    pub fn clear_slots(&self) {
        self.send_source_task(ColumnCommand::ClearSlots);
    }

    pub fn update_settings(&self, settings: ColumnSettings) {
        self.send_source_task(ColumnCommand::UpdateSettings(settings));
    }

    pub fn fill_slot(&self, args: Box<Option<ColumnFillSlotArgs>>) {
        self.send_source_task(ColumnCommand::FillSlot(args));
    }

    pub fn set_clip_audio_resample_mode(&self, args: ColumnSetClipAudioResampleModeArgs) {
        self.send_source_task(ColumnCommand::SetClipAudioResampleMode(args));
    }

    pub fn set_clip_audio_time_stretch_mode(&self, args: ColumnSetClipAudioTimeStretchModeArgs) {
        self.send_source_task(ColumnCommand::SetClipAudioTimeStretchMode(args));
    }

    pub fn play_clip(&self, args: ColumnPlayClipArgs) {
        self.send_source_task(ColumnCommand::PlayClip(args));
    }

    pub fn stop_clip(&self, args: ColumnStopClipArgs) {
        self.send_source_task(ColumnCommand::StopClip(args));
    }

    pub fn set_clip_looped(&self, args: ColumnSetClipLoopedArgs) {
        self.send_source_task(ColumnCommand::SetClipLooped(args));
    }

    pub fn pause_clip(&self, index: usize) {
        let args = ColumnPauseClipArgs { index };
        self.send_source_task(ColumnCommand::PauseClip(args));
    }

    pub fn seek_clip(&self, index: usize, desired_pos: UnitValue) {
        let args = ColumnSeekClipArgs { index, desired_pos };
        self.send_source_task(ColumnCommand::SeekClip(args));
    }

    pub fn set_clip_volume(&self, slot_index: usize, volume: Db) {
        let args = ColumnSetClipVolumeArgs { slot_index, volume };
        self.send_source_task(ColumnCommand::SetClipVolume(args));
    }

    fn send_source_task(&self, task: ColumnCommand) {
        self.command_sender.try_send(task).unwrap();
    }
}

#[derive(Debug)]
pub enum ColumnCommand {
    ClearSlots,
    UpdateSettings(ColumnSettings),
    // Boxed because comparatively large.
    FillSlot(Box<Option<ColumnFillSlotArgs>>),
    SetClipAudioResampleMode(ColumnSetClipAudioResampleModeArgs),
    SetClipAudioTimeStretchMode(ColumnSetClipAudioTimeStretchModeArgs),
    PlayClip(ColumnPlayClipArgs),
    StopClip(ColumnStopClipArgs),
    PauseClip(ColumnPauseClipArgs),
    SeekClip(ColumnSeekClipArgs),
    SetClipVolume(ColumnSetClipVolumeArgs),
    SetClipLooped(ColumnSetClipLoopedArgs),
}

trait EventSender {
    fn clip_play_state_changed(&self, slot_index: usize, play_state: ClipPlayState);

    fn clip_material_info_changed(&self, slot_index: usize, material_info: MaterialInfo);

    fn dispose(&self, garbage: ColumnGarbage);

    fn send_event(&self, event: ColumnEvent);
}

impl EventSender for Sender<ColumnEvent> {
    fn clip_play_state_changed(&self, slot_index: usize, play_state: ClipPlayState) {
        let event = ColumnEvent::ClipPlayStateChanged {
            slot_index,
            play_state,
        };
        self.send_event(event);
    }

    fn clip_material_info_changed(&self, slot_index: usize, material_info: MaterialInfo) {
        let event = ColumnEvent::ClipMaterialInfoChanged {
            slot_index,
            material_info,
        };
        self.send_event(event);
    }

    fn dispose(&self, garbage: ColumnGarbage) {
        self.send_event(ColumnEvent::Dispose(garbage));
    }

    fn send_event(&self, event: ColumnEvent) {
        self.try_send(event).unwrap();
    }
}

#[derive(Clone, Debug, Default)]
pub struct ColumnSettings {
    // TODO-low We could maybe also treat this like e.g. time stretch mode. Something that's
    //  not always passed through the functions but updated whenever changed and propagated as effective
    //  timing to the clip. Let's see what turns out to be the more practical design.
    pub clip_play_start_timing: Option<ClipPlayStartTiming>,
    pub clip_play_stop_timing: Option<ClipPlayStopTiming>,
    pub play_mode: ColumnPlayMode,
}

const MAX_CHANNEL_COUNT: usize = 64;
const MAX_BLOCK_SIZE: usize = 2048;

/// At the time of this writing, a slot is just around 900 byte, so 100 slots take roughly 90 kB.
const MAX_SLOT_COUNT_WITHOUT_REALLOCATION: usize = 100;

impl Column {
    pub fn new(
        permanent_project: Option<Project>,
        command_receiver: Receiver<ColumnCommand>,
        event_sender: Sender<ColumnEvent>,
    ) -> Self {
        debug!("Slot size: {}", std::mem::size_of::<Slot>());
        Self {
            settings: Default::default(),
            slots: Vec::with_capacity(MAX_SLOT_COUNT_WITHOUT_REALLOCATION),
            project: permanent_project,
            command_receiver,
            event_sender,
            // Sized to hold pretty any audio block imaginable. Vastly oversized for the majority
            // of use cases but 1 MB memory per column ... okay for now, on the safe side.
            mix_buffer_chunk: OwnedAudioBuffer::new(MAX_CHANNEL_COUNT, MAX_BLOCK_SIZE).into_inner(),
            timeline_was_paused_in_last_block: false,
        }
    }

    fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        let material_info = args.clip.material_info().unwrap();
        get_slot_mut_insert(&mut self.slots, args.slot_index).fill(args.clip);
        self.event_sender
            .clip_material_info_changed(args.slot_index, material_info);
    }

    pub fn slot(&self, index: usize) -> ClipEngineResult<&Slot> {
        get_slot(&self.slots, index)
    }

    pub fn slot_mut(&mut self, index: usize) -> ClipEngineResult<&mut Slot> {
        self.slots.get_mut(index).ok_or(SLOT_DOESNT_EXIST)
    }

    fn set_clip_audio_resample_mode(
        &mut self,
        slot_index: usize,
        mode: VirtualResampleMode,
    ) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, slot_index)?.set_clip_audio_resample_mode(mode)
    }

    fn set_clip_audio_time_stretch_mode(
        &mut self,
        slot_index: usize,
        mode: AudioTimeStretchMode,
    ) -> ClipEngineResult<()> {
        get_slot_mut(&mut self.slots, slot_index)?.set_clip_audio_time_stretch_mode(mode)
    }

    pub fn play_clip(&mut self, args: ColumnPlayClipArgs) -> ClipEngineResult<()> {
        let parent_start_timing = self
            .settings
            .clip_play_start_timing
            .unwrap_or(args.parent_start_timing);
        let parent_stop_timing = self
            .settings
            .clip_play_stop_timing
            .unwrap_or(args.parent_stop_timing);
        let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
        if self.settings.play_mode.is_exclusive() {
            for (_, slot) in self
                .slots
                .iter_mut()
                .enumerate()
                .filter(|(i, _)| *i != args.slot_index)
            {
                let stop_args = ClipStopArgs {
                    parent_start_timing,
                    parent_stop_timing,
                    stop_timing: None,
                    timeline: &args.timeline,
                    ref_pos: Some(ref_pos),
                };
                let _ = slot.stop_clip(stop_args);
            }
        }
        let clip_args = ClipPlayArgs {
            parent_start_timing,
            timeline: &args.timeline,
            ref_pos: Some(ref_pos),
        };
        get_slot_mut(&mut self.slots, args.slot_index)?.play_clip(clip_args)
    }

    pub fn stop_clip(&mut self, args: ColumnStopClipArgs) -> ClipEngineResult<()> {
        let clip_args = ClipStopArgs {
            parent_start_timing: self
                .settings
                .clip_play_start_timing
                .unwrap_or(args.parent_start_timing),
            parent_stop_timing: self
                .settings
                .clip_play_stop_timing
                .unwrap_or(args.parent_stop_timing),
            stop_timing: None,
            timeline: &args.timeline,
            ref_pos: args.ref_pos,
        };
        get_slot_mut(&mut self.slots, args.slot_index)?.stop_clip(clip_args)
    }

    pub fn set_clip_looped(&mut self, args: ColumnSetClipLoopedArgs) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, args.slot_index).set_clip_looped(args.looped)
    }

    pub fn clip_play_state(&self, index: usize) -> ClipEngineResult<ClipPlayState> {
        Ok(get_slot(&self.slots, index)?.clip()?.play_state())
    }

    pub fn record_clip(
        &mut self,
        index: usize,
        behavior: RecordBehavior,
        equipment: &RecorderEquipment,
        pre_buffer_request_sender: &Sender<ChainPreBufferRequest>,
    ) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, index).record_clip(
            behavior,
            ClipRecordInput::Audio,
            self.project,
            equipment,
            pre_buffer_request_sender,
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

    fn set_clip_volume(&mut self, slot_index: usize, volume: Db) -> ClipEngineResult<()> {
        get_slot_mut_insert(&mut self.slots, slot_index).set_clip_volume(volume)
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

    fn duration(&self) -> DurationInSeconds {
        DurationInSeconds::MAX
    }

    fn process_commands(&mut self) {
        while let Ok(task) = self.command_receiver.try_recv() {
            use ColumnCommand::*;
            match task {
                ClearSlots => {
                    self.slots.clear();
                }
                UpdateSettings(s) => {
                    self.settings = s;
                }
                FillSlot(mut boxed_args) => {
                    let args = boxed_args.take().unwrap();
                    self.fill_slot(args);
                    self.event_sender
                        .dispose(ColumnGarbage::FillSlotArgs(boxed_args))
                }
                SetClipAudioResampleMode(args) => {
                    let _ = self.set_clip_audio_resample_mode(args.slot_index, args.mode);
                }
                SetClipAudioTimeStretchMode(args) => {
                    let _ = self.set_clip_audio_time_stretch_mode(args.slot_index, args.mode);
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
                    let _ = self.set_clip_volume(args.slot_index, args.volume);
                }
                SeekClip(args) => {
                    let _ = self.seek_clip(args.index, args.desired_pos);
                }
                SetClipLooped(args) => {
                    let _ = self.set_clip_looped(args);
                }
            }
        }
    }

    fn get_samples(&mut self, args: GetSamplesArgs) {
        // We have code, e.g. triggered by crossbeam_channel that requests the ID of the
        // current thread. This operation needs an allocation at the first time it's executed
        // on a specific thread. If Live FX multi-processing is enabled, get_samples() will be
        // called from a non-sticky worker thread instead of the audio interface thread (for which
        // we already do this), so we execute this here again in order to initialize the current
        // thread outside of assert_no_alloc.
        let _ = std::thread::current().id();
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
            // Handle sync to project pause
            if !timeline.is_running() {
                // Main timeline is paused.
                self.timeline_was_paused_in_last_block = true;
                return;
            }
            let resync = if self.timeline_was_paused_in_last_block {
                self.timeline_was_paused_in_last_block = false;
                true
            } else {
                false
            };
            // Get samples
            let timeline_cursor_pos = timeline.cursor_pos();
            let timeline_tempo = timeline.tempo_at(timeline_cursor_pos);
            let output_channel_count = args.block.nch() as usize;
            let output_frame_count = args.block.length() as usize;
            let mut output_buffer = unsafe {
                AudioBufMut::from_raw(
                    args.block.samples(),
                    output_channel_count,
                    output_frame_count,
                )
            };
            // rt_debug!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
            //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
            for (row, slot) in self.slots.iter_mut().enumerate() {
                // Our strategy is to always write all available source channels into the mix
                // buffer. From a performance perspective, it would actually be enough to take
                // only as many channels as we need (= track channel count). However, always using
                // the source channel count as reference is much simpler, in particular when it
                // comes to caching and pre-buffering. Also, in practice this is rarely an issue.
                // Most samples out there used in typical stereo track setups have no more than 2
                // channels. And if they do, the user can always down-mix to the desired channel
                // count up-front.
                let clip_material_info = match slot.material_info() {
                    Ok(i) => i,
                    Err(_) => continue,
                };
                let clip_channel_count = clip_material_info.channel_count();
                let mut mix_buffer = AudioBufMut::from_slice(
                    &mut self.mix_buffer_chunk,
                    clip_channel_count,
                    output_frame_count,
                )
                .unwrap();
                let mut inner_args = ClipProcessArgs {
                    dest_buffer: &mut mix_buffer,
                    dest_sample_rate: args.block.sample_rate(),
                    midi_event_list: args
                        .block
                        .midi_event_list_mut()
                        .expect("no MIDI event list available"),
                    timeline: &timeline,
                    timeline_cursor_pos,
                    timeline_tempo,
                    resync,
                };
                if let Ok(outcome) = slot.process(&mut inner_args) {
                    if outcome.num_audio_frames_written > 0 {
                        output_buffer
                            .slice_mut(0..outcome.num_audio_frames_written)
                            .modify_frames(|sample| {
                                // TODO-high-performance This is a hot code path. We might want to skip bound checks
                                //  in sample_value_at().
                                if sample.index.channel < clip_channel_count {
                                    sample.value + mix_buffer.sample_value_at(sample.index).unwrap()
                                } else {
                                    // Clip doesn't have material on this channel.
                                    0.0
                                }
                            })
                    }
                    if let Some(changed_play_state) = outcome.changed_play_state {
                        self.event_sender
                            .clip_play_state_changed(row, changed_play_state);
                    }
                }
            }
        });
        debug_assert_eq!(args.block.samples_out(), args.block.length());
    }

    fn extended(&mut self, _args: ExtendedArgs) -> i32 {
        // TODO-medium Maybe implement PCM_SOURCE_EXT_NOTIFYPREVIEWPLAYPOS. This is the only
        //  extended call done by the preview register, at least for type WAVE.
        0
    }
}

impl CustomPcmSource for SharedColumn {
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
        unimplemented!()
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

    fn get_samples(&mut self, args: GetSamplesArgs) {
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
    pub slot_index: usize,
    pub clip: Clip,
}

#[derive(Debug)]
pub struct ColumnSetClipAudioResampleModeArgs {
    pub slot_index: usize,
    pub mode: VirtualResampleMode,
}

#[derive(Debug)]
pub struct ColumnSetClipAudioTimeStretchModeArgs {
    pub slot_index: usize,
    pub mode: AudioTimeStretchMode,
}

#[derive(Debug)]
pub struct ColumnPlayClipArgs {
    pub slot_index: usize,
    pub parent_start_timing: ClipPlayStartTiming,
    pub parent_stop_timing: ClipPlayStopTiming,
    pub timeline: HybridTimeline,
    /// Set this if you already have the current timeline position or want to play a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
}

#[derive(Debug)]
pub struct ColumnStopClipArgs {
    pub slot_index: usize,
    pub parent_start_timing: ClipPlayStartTiming,
    pub parent_stop_timing: ClipPlayStopTiming,
    pub timeline: HybridTimeline,
    /// Set this if you already have the current timeline position or want to stop a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
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
    pub slot_index: usize,
    pub volume: Db,
}

#[derive(Debug)]
pub struct ColumnSetClipLoopedArgs {
    pub slot_index: usize,
    pub looped: bool,
}

pub struct ColumnWithSlotArgs<'a> {
    pub index: usize,
    pub use_slot: &'a dyn Fn(),
}

fn get_slot(slots: &[Slot], index: usize) -> ClipEngineResult<&Slot> {
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

#[derive(Debug)]
pub enum ColumnEvent {
    ClipPlayStateChanged {
        slot_index: usize,
        play_state: ClipPlayState,
    },
    ClipMaterialInfoChanged {
        slot_index: usize,
        material_info: MaterialInfo,
    },
    Dispose(ColumnGarbage),
}

#[derive(Debug)]
pub enum ColumnGarbage {
    FillSlotArgs(Box<Option<ColumnFillSlotArgs>>),
}
