use crate::{
    clip_timeline, ClipChangedEvent, ClipPlayArgs, ClipProcessArgs, ClipStopArgs, ClipStopBehavior,
    NewClip, Slot, SlotPollArgs, Timeline,
};
use assert_no_alloc::assert_no_alloc;
use num_enum::TryFromPrimitive;
use reaper_high::{BorrowedSource, Project};
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    reaper_str, BorrowedPcmSource, CustomPcmSource, DurationInBeats, DurationInSeconds,
    ExtendedArgs, GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, OwnedPcmSource,
    OwnedPreviewRegister, PcmSource, PeaksClearArgs, PositionInSeconds, PropertiesWindowArgs,
    ReaperStr, SaveStateArgs, SetAvailableArgs, SetFileNameArgs, SetSourceArgs,
};
use std::convert::{TryFrom, TryInto};
use std::error::Error;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::Arc;

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

impl CustomPcmSource for ColumnSource {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        unimplemented!()
    }

    fn is_available(&mut self) -> bool {
        unimplemented!()
    }

    fn set_available(&mut self, args: SetAvailableArgs) {
        unimplemented!()
    }

    fn get_type(&mut self) -> &ReaperStr {
        reaper_str!("WAVE")
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unimplemented!()
    }

    fn set_file_name(&mut self, args: SetFileNameArgs) -> bool {
        unimplemented!()
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        unimplemented!()
    }

    fn set_source(&mut self, args: SetSourceArgs) {
        unimplemented!()
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        // TODO-high We should return the maximum channel count over all clips.
        //  In get_samples(), we should convert interleaved buffer accordingly.
        //  Probably a good idea to cache the max channel count.
        Some(2)
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        unimplemented!()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        DurationInSeconds::MAX
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

    fn properties_window(&mut self, args: PropertiesWindowArgs) -> i32 {
        unimplemented!()
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
                let inner_args = ClipProcessArgs {
                    block: args.block,
                    timeline: &timeline,
                    timeline_cursor_pos,
                    timeline_tempo,
                };
                // TODO-high Take care of mixing as soon as we implement Free mode.
                let _ = slot.process(inner_args);
            }
        });
        debug_assert_eq!(args.block.samples_out(), args.block.length());
    }

    fn get_peak_info(&mut self, args: GetPeakInfoArgs) {
        unimplemented!()
    }

    fn save_state(&mut self, args: SaveStateArgs) {
        unimplemented!()
    }

    fn load_state(&mut self, args: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unimplemented!()
    }

    fn peaks_clear(&mut self, args: PeaksClearArgs) {
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
        if let Ok(op_code) = OpCode::try_from(args.call) {
            use OpCode::*;
            match op_code {
                FillSlot => forward_ext(&args, |a| self.fill_slot(a)),
                PlayClip => forward_ext(&args, |a| self.play_clip(a)),
                StopClip => forward_ext(&args, |a| self.stop_clip(a)),
                SetClipRepeated => forward_ext(&args, |a| self.set_clip_repeated(a)),
                ToggleClipRepeated => forward_ext(&args, |a| self.toggle_clip_repeated(a)),
                PollSlot => forward_ext(&args, |a| self.poll_slot(a)),
            }
        } else {
            // TODO-medium Maybe implement PCM_SOURCE_EXT_NOTIFYPREVIEWPLAYPOS. This is the only
            //  extended call done by the preview register, at least for type WAVE.
            0
        }
    }
}

// TODO-low Using this extended() mechanism is not very Rusty. The reason why we do it at the
//  moment is that we acquire access to the source by accessing the `source` attribute of the
//  preview register data structure. First, this can be *any* source in general, it's not
//  necessarily a PCM source for clips. Okay, this is not the primary issue. In practice we make
//  sure that it's only ever a PCM source for clips, so we could just do some explicit casting,
//  right? No. The thing which we get back there is not a reference to our ClipPcmSource struct.
//  It's the reaper-rs C++ PCM source, the one that delegates to our Rust struct. This C++ PCM
//  source implements the C++ virtual base class that REAPER API requires and it owns our Rust
//  struct. So if we really want to get rid of the extended() mechanism, we would have to access the
//  ClipPcmSource directly, without taking the C++ detour. And how is this possible in a safe Rusty
//  way that guarantees us that no one else is mutably accessing the source at the same time? By
//  wrapping the source in a mutex. However, this would mean that all calls to that source, even
//  the ones from REAPER would have to unlock the mutex first. For each source operation. That
//  sounds like a bad idea (or is it not because happy path is fast)? Well, but the point is, we
//  already have a mutex. The one around the preview register. This one is strictly necessary,
//  even the REAPER API requires it. As long as we have that outer mutex locked, we should in theory
//  be able to safely interact with our source directly from Rust. So in order to get rid of the
//  extended() mechanism, we would have to provide a way to get a correctly typed reference to our
//  original Rust struct. This itself is maybe possible by using some unsafe code, not sure.
#[derive(Copy, Clone, TryFromPrimitive)]
#[repr(i32)]
enum OpCode {
    FillSlot = 2359769,
    PlayClip,
    StopClip,
    ToggleClipRepeated,
    SetClipRepeated,
    PollSlot,
}

pub trait ColumnSourceSkills {
    fn fill_slot(&mut self, args: ColumnFillSlotArgs);
    fn play_clip(&mut self, args: ColumnPlayClipArgs) -> Result<(), &'static str>;
    fn stop_clip(&mut self, args: ColumnStopClipArgs) -> Result<(), &'static str>;
    fn set_clip_repeated(&mut self, args: ColumnSetClipRepeatedArgs) -> Result<(), &'static str>;
    fn toggle_clip_repeated(&mut self, index: usize) -> Result<ClipChangedEvent, &'static str>;
    fn poll_slot(&mut self, args: ColumnPollSlotArgs) -> Option<ClipChangedEvent>;
}

pub struct ColumnFillSlotArgs {
    pub index: usize,
    pub clip: NewClip,
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

impl ColumnSourceSkills for BorrowedPcmSource {
    fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        ext_in(self, OpCode::FillSlot, args);
    }

    fn play_clip(&mut self, args: ColumnPlayClipArgs) -> Result<(), &'static str> {
        ext_in_out_result(self, OpCode::PlayClip, args)
    }

    fn stop_clip(&mut self, args: ColumnStopClipArgs) -> Result<(), &'static str> {
        ext_in_out_result(self, OpCode::StopClip, args)
    }

    fn set_clip_repeated(&mut self, args: ColumnSetClipRepeatedArgs) -> Result<(), &'static str> {
        ext_in_out_result(self, OpCode::SetClipRepeated, args)
    }

    fn toggle_clip_repeated(&mut self, index: usize) -> Result<ClipChangedEvent, &'static str> {
        ext_in_out_result(self, OpCode::ToggleClipRepeated, index)
    }

    fn poll_slot(&mut self, args: ColumnPollSlotArgs) -> Option<ClipChangedEvent> {
        ext_in_out_default(self, OpCode::PollSlot, args)
    }
}

impl ColumnSourceSkills for ColumnSource {
    fn fill_slot(&mut self, args: ColumnFillSlotArgs) {
        let slot = get_slot_mut(&mut self.slots, args.index);
        slot.fill(args.clip);
    }

    fn play_clip(&mut self, args: ColumnPlayClipArgs) -> Result<(), &'static str> {
        let slot = get_slot_mut(&mut self.slots, args.index);
        // TODO-high If column mode Song, suspend all other clips first.
        slot.play_clip(args.clip_args)
    }

    fn stop_clip(&mut self, args: ColumnStopClipArgs) -> Result<(), &'static str> {
        let slot = get_slot_mut(&mut self.slots, args.index);
        slot.stop_clip(args.clip_args)
    }

    fn set_clip_repeated(&mut self, args: ColumnSetClipRepeatedArgs) -> Result<(), &'static str> {
        let slot = get_slot_mut(&mut self.slots, args.index);
        slot.set_clip_repeated(args.repeated)
    }

    fn toggle_clip_repeated(&mut self, index: usize) -> Result<ClipChangedEvent, &'static str> {
        let slot = get_slot_mut(&mut self.slots, index);
        slot.toggle_clip_repeated()
    }

    fn poll_slot(&mut self, args: ColumnPollSlotArgs) -> Option<ClipChangedEvent> {
        let slot = get_slot_mut(&mut self.slots, args.index);
        slot.poll(args.slot_args)
    }
}

fn get_slot_mut(slots: &mut Vec<Slot>, index: usize) -> &mut Slot {
    if index >= slots.len() {
        slots.resize_with(index + 1, Default::default);
    }
    slots.get_mut(index).unwrap()
}

fn ext_in<T>(source: &BorrowedPcmSource, op_code: OpCode, mut args: T) {
    ext_in_out_default::<T, ()>(source, op_code, args);
}

fn ext_in_out_default<T, R: Default>(
    source: &BorrowedPcmSource,
    op_code: OpCode,
    mut args: T,
) -> R {
    ext_in_out(source, op_code, args, R::default())
}

fn ext_in_out_result<T, R>(
    source: &BorrowedPcmSource,
    op_code: OpCode,
    mut args: T,
) -> Result<R, &'static str> {
    ext_in_out(source, op_code, args, Err("column source ext error"))
}

fn ext_in_out<T, R>(source: &BorrowedPcmSource, op_code: OpCode, args: T, default: R) -> R {
    use std::ptr::null_mut;
    let mut return_value = default;
    // We want to "move" the arguments across the FFI boundary in order to be able to pass non-Copy
    // structs by value. Moving means that ownership is passed to the callee and thus this function
    // is not responsible for dropping the value anymore.
    // Attention: If extended() doesn't pick the value up, we have a memory leak!
    let mut args = ManuallyDrop::new(args);
    unsafe {
        source.extended(
            op_code as _,
            // Pass pointer to stack value
            &mut args as *mut _ as _,
            &mut return_value as *mut _ as _,
            null_mut(),
        );
    }
    return_value
}

unsafe fn forward_ext<T, R>(args: &ExtendedArgs, f: impl FnOnce(T) -> R) -> i32 {
    let inner_args = ptr::read(args.parm_1 as *mut T);
    *(args.parm_2 as *mut _) = f(inner_args);
    1
}
