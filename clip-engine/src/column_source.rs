use crate::{
    clip_timeline, CacheRequest, Clip, ClipChangedEvent, ClipPlayArgs, ClipProcessArgs,
    ClipRecordInput, ClipStopArgs, ClipStopBehavior, RecordBehavior, RecordKind, RecorderRequest,
    Slot, SlotPollArgs, SlotProcessTransportChangeArgs, Timeline, TimelineMoment, TransportChange,
    WriteAudioRequest, WriteMidiRequest,
};
use assert_no_alloc::assert_no_alloc;
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use num_enum::TryFromPrimitive;
use reaper_high::{BorrowedSource, Project};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    reaper_str, BorrowedPcmSource, CustomPcmSource, DurationInBeats, DurationInSeconds,
    ExtendedArgs, GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, OwnedPcmSource,
    OwnedPreviewRegister, PcmSource, PeaksClearArgs, PositionInSeconds, PropertiesWindowArgs,
    ReaperStr, ReaperVolumeValue, SaveStateArgs, SetAvailableArgs, SetFileNameArgs, SetSourceArgs,
};
use std::convert::{TryFrom, TryInto};
use std::error::Error;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::{Arc, LockResult, Mutex, MutexGuard};

#[derive(Clone, Debug)]
pub struct SharedColumnSource(Arc<Mutex<ColumnSource>>);

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
}

#[derive(Debug)]
pub struct ColumnSource {
    mode: ClipColumnMode,
    slots: Vec<Slot>,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
}

impl ColumnSource {
    pub fn new(project: Option<Project>) -> Self {
        Self {
            mode: Default::default(),
            // TODO-high We should probably make this higher so we don't need to allocate in the
            //  audio thread (or block the audio thread through allocation in the main thread).
            //  Or we find a mechanism to return a request for a newly allocated vector, release
            //  the mutex in the meanwhile and try a second time as soon as we have the allocation
            //  ready.
            slots: Vec::with_capacity(8),
            project,
        }
    }

    pub fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        get_slot_mut(&mut self.slots, args.index).fill(args.clip);
    }

    pub fn with_slot<R>(
        &self,
        index: usize,
        f: impl FnOnce(&Slot) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        f(get_slot(&self.slots, index)?)
    }

    pub fn play_clip(&mut self, args: ColumnPlayClipArgs) -> Result<(), &'static str> {
        // TODO-high If column mode Song, suspend all other clips first.
        get_slot_mut(&mut self.slots, args.index).play_clip(args.clip_args)
    }

    pub fn stop_clip(&mut self, args: ColumnStopClipArgs) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, args.index).stop_clip(args.clip_args)
    }

    pub fn set_clip_repeated(
        &mut self,
        args: ColumnSetClipRepeatedArgs,
    ) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, args.index).set_clip_repeated(args.repeated)
    }

    pub fn toggle_clip_repeated(&mut self, index: usize) -> Result<ClipChangedEvent, &'static str> {
        get_slot_mut(&mut self.slots, index).toggle_clip_repeated()
    }

    pub fn record_clip(
        &mut self,
        index: usize,
        behavior: RecordBehavior,
        recorder_request_sender: Sender<RecorderRequest>,
        cache_request_sender: Sender<CacheRequest>,
    ) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, index).record_clip(
            behavior,
            ClipRecordInput::Audio,
            self.project,
            recorder_request_sender,
            cache_request_sender,
        )
    }

    pub fn pause_clip(&mut self, index: usize) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, index).pause_clip()
    }

    pub fn seek_clip(&mut self, index: usize, desired_pos: UnitValue) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, index).seek_clip(desired_pos)
    }

    pub fn clip_record_input(&self, index: usize) -> Option<ClipRecordInput> {
        get_slot(&self.slots, index).ok()?.clip_record_input()
    }

    pub fn write_clip_midi(
        &mut self,
        index: usize,
        request: WriteMidiRequest,
    ) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, index).write_clip_midi(request)
    }

    pub fn write_clip_audio(
        &mut self,
        index: usize,
        request: WriteAudioRequest,
    ) -> Result<(), &'static str> {
        get_slot_mut(&mut self.slots, index).write_clip_audio(request)
    }

    pub fn set_clip_volume(
        &mut self,
        index: usize,
        volume: ReaperVolumeValue,
    ) -> Result<ClipChangedEvent, &'static str> {
        get_slot_mut(&mut self.slots, index).set_clip_volume(volume)
    }

    pub fn poll_slot(&mut self, args: ColumnPollSlotArgs) -> Option<ClipChangedEvent> {
        let slot = get_slot_mut(&mut self.slots, args.index);
        slot.poll(args.slot_args)
    }

    pub fn process_transport_change(&mut self, args: &SlotProcessTransportChangeArgs) {
        for slot in &mut self.slots {
            slot.process_transport_change(args);
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

    fn get_samples(&mut self, mut args: GetSamplesArgs) {
        assert_no_alloc(|| {
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
            // println!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
            //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
            for slot in &mut self.slots {
                let mut inner_args = ClipProcessArgs {
                    block: args.block,
                    timeline: &timeline,
                    timeline_cursor_pos,
                    timeline_tempo,
                };
                // TODO-high Take care of mixing as soon as we implement Free mode.
                let _ = slot.process(&mut inner_args);
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

pub struct ColumnFillSlotArgs {
    pub index: usize,
    pub clip: Clip,
}

pub struct ColumnPlayClipArgs {
    pub index: usize,
    pub clip_args: ClipPlayArgs,
}

pub struct ColumnStopClipArgs<'a> {
    pub index: usize,
    pub clip_args: ClipStopArgs<'a>,
}

pub struct ColumnSetClipRepeatedArgs {
    pub index: usize,
    pub repeated: bool,
}

pub struct ColumnPollSlotArgs {
    pub index: usize,
    pub slot_args: SlotPollArgs,
}

pub struct ColumnWithSlotArgs<'a> {
    pub index: usize,
    pub use_slot: &'a dyn Fn(),
}

fn get_slot(slots: &Vec<Slot>, index: usize) -> Result<&Slot, &'static str> {
    slots.get(index).ok_or("slot doesn't exist")
}

fn get_slot_mut(slots: &mut Vec<Slot>, index: usize) -> &mut Slot {
    if index >= slots.len() {
        slots.resize_with(index + 1, Default::default);
    }
    slots.get_mut(index).unwrap()
}
