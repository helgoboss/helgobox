use std::convert::TryInto;
use std::error::Error;
use std::ptr::null_mut;

use helgoboss_learn::BASE_EPSILON;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::Reaper;
use reaper_medium::{
    BorrowedPcmSource, BorrowedPcmSourceTransfer, CustomPcmSource, DurationInBeats,
    DurationInSeconds, ExtendedArgs, GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, MidiEvent,
    OwnedPcmSource, PcmSource, PeaksClearArgs, PositionInSeconds, ProjectContext,
    PropertiesWindowArgs, ReaperStr, SaveStateArgs, SetAvailableArgs, SetFileNameArgs,
    SetSourceArgs,
};

use crate::domain::clip::source_util::pcm_source_is_midi;

// TODO-medium Using this extended() mechanism is not very Rusty. The reason why we do it at the
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
const EXT_QUERY_STATE: i32 = 2359769;
const EXT_SCHEDULE_START: i32 = 2359771;
const EXT_QUERY_INNER_LENGTH: i32 = 2359772;
const EXT_ENABLE_REPEAT: i32 = 2359773;
const EXT_DISABLE_REPEAT: i32 = 2359774;
const EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED: i32 = 2359775;
const EXT_SCHEDULE_STOP: i32 = 2359776;
const EXT_BACKPEDAL_FROM_SCHEDULED_STOP: i32 = 2359777;
const EXT_ADJUST_START_POS_BY: i32 = 2359778;
const EXT_SUSPEND: i32 = 2359779;
const EXT_RETRIGGER: i32 = 2359780;

/// Represents a state of the clip wrapper PCM source.
#[derive(Copy, Clone, Eq, PartialEq, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(i32)]
pub enum ClipPcmSourceState {
    Suspended = 9,
    Playing = 10,
    PrepareSuspension = 11,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ClipStopPosition {
    At(PositionInSeconds),
    AtEndOfClip,
}

/// A PCM source which wraps a native REAPER PCM source and applies all kinds of clip
/// functionality to it.
///
/// For example, it makes sure it starts at the right position on the timeline.
///
/// It's intended to be continuously played by a preview register (immediately, unbuffered,
/// infinitely).
pub struct ClipPcmSource {
    /// This source contains the actual audio/MIDI data.
    inner: OwnedPcmSource,
    state: ClipPcmSourceState,
    /// An ever-increasing counter which is used just for debugging purposes at the moment.
    counter: u64,
    /// If set, the clip is playing or about to play. If not set, the clip is stopped.
    start_pos: Option<PositionInSeconds>,
    /// If set, the clip is about to stop. If not set, the clip is either stopped or playing.
    stop_pos: Option<PositionInSeconds>,
    /// Unused at the moment but intended for persistently setting the start offset.
    offset: PositionInSeconds,
    repeated: bool,
    is_midi: bool,
}

impl ClipPcmSource {
    /// Wraps the given native REAPER PCM source.
    pub fn new(inner: OwnedPcmSource) -> Self {
        let is_midi = pcm_source_is_midi(&inner);
        Self {
            inner,
            state: ClipPcmSourceState::Suspended,
            counter: 0,
            start_pos: None,
            stop_pos: None,
            offset: PositionInSeconds::ZERO,
            repeated: false,
            is_midi,
        }
    }

    fn timeline_cursor_pos(&self) -> PositionInSeconds {
        // TODO-high Save and use actual project in source.
        Reaper::get()
            .medium_reaper()
            .get_play_position_2_ex(ProjectContext::CurrentProject)
    }

    /// Returns the position starting from the time that this source was scheduled for start.
    ///
    /// Returns `None` if not scheduled or if beyond scheduled stop. Returns negative position if
    /// clip not yet playing.
    fn pos_from_start(&self) -> Option<PositionInSeconds> {
        let scheduled_start_pos = self.start_pos?;
        let current_pos = self.timeline_cursor_pos();
        // Return `None` if scheduled stop position is reached.
        if let Some(scheduled_stop_pos) = self.stop_pos {
            if current_pos.has_reached(scheduled_stop_pos) {
                return None;
            }
        }
        Some(current_pos - scheduled_start_pos + self.offset)
    }

    /// Calculates the current position within the clip considering the *repeated* setting.
    ///
    /// Returns negative position if clip not yet playing. Returns `None` if clip length is zero.
    fn calculate_pos_within_clip(
        &self,
        pos_from_start: PositionInSeconds,
    ) -> Option<PositionInSeconds> {
        if pos_from_start < PositionInSeconds::ZERO {
            // Count-in phase. Report negative position.
            Some(pos_from_start)
        } else if self.repeated {
            // Playing and repeating. Report repeated position.
            pos_from_start % self.query_inner_length()
        } else if pos_from_start.get() < self.query_inner_length().get() {
            // Not repeating and still within clip bounds. Just report position.
            Some(pos_from_start)
        } else {
            // Not repeating and clip length exceeded.
            None
        }
    }
}

impl CustomPcmSource for ClipPcmSource {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        // Not correct but probably never used.
        self.inner.duplicate()
    }

    fn is_available(&mut self) -> bool {
        self.inner.is_available()
    }

    fn set_available(&mut self, args: SetAvailableArgs) {
        self.inner.set_available(args.is_available);
    }

    fn get_type(&mut self) -> &ReaperStr {
        unsafe { self.inner.get_type_unchecked() }
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unsafe { self.inner.get_file_name_unchecked() }
    }

    fn set_file_name(&mut self, args: SetFileNameArgs) -> bool {
        self.inner.set_file_name(args.new_file_name)
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        self.inner.get_source()
    }

    fn set_source(&mut self, args: SetSourceArgs) {
        self.inner.set_source(args.source);
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        self.inner.get_num_channels()
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        self.inner.get_sample_rate()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        DurationInSeconds::MAX
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        let _ = self.inner.get_length_beats()?;
        Some(DurationInBeats::MAX)
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        self.inner.get_bits_per_sample()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        self.inner.get_preferred_position()
    }

    fn properties_window(&mut self, args: PropertiesWindowArgs) -> i32 {
        unsafe { self.inner.properties_window(args.parent_window) }
    }

    fn get_samples(&mut self, args: GetSamplesArgs) {
        // Debugging
        // if self.counter % 500 == 0 {
        //     let ptr = args.block.as_ptr();
        //     let raw = unsafe { ptr.as_ref() };
        //     dbg!(raw);
        // }
        self.counter += 1;
        // Actual stuff
        use ClipPcmSourceState::*;
        match self.state {
            Suspended => {}
            Playing => unsafe {
                if let Some(pos) = self.pos_within_clip() {
                    // This means the clip is playing or about o play. At least not stopped.
                    // We want to start playing as soon as we reach the scheduled start position,
                    // that means pos == 0.0. In order to do that, we need to take into account that
                    // the audio buffer start point is not necessarily equal to the measure start
                    // point. If we would naively start playing as soon as pos >= 0.0, we might skip
                    // the first samples/messages! We need to start playing as soon as the end of
                    // the audio block is located on or right to the scheduled start point
                    // (end_pos >= 0.0).
                    let desired_sample_count = args.block.length();
                    let sample_rate = args.block.sample_rate().get();
                    let block_duration = desired_sample_count as f64 / sample_rate;
                    let end_pos = PositionInSeconds::new_unchecked(pos.get() + block_duration);
                    if end_pos < PositionInSeconds::ZERO {
                        return;
                    }
                    if self.is_midi {
                        // MIDI.
                        // For MIDI it seems to be okay to start at a negative position. The source
                        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
                        // offset.
                        args.block.set_time_s(pos);
                        self.inner.get_samples(args.block);
                        let written_sample_count = args.block.samples_out();
                        if written_sample_count < desired_sample_count {
                            // We have reached the end of the clip and it doesn't fill the
                            // complete block.
                            if self.repeated {
                                // Repeat. Fill rest of buffer with beginning of source.
                                // We need to start from negative position so the frame
                                // offset of the *added* MIDI events is correctly written.
                                // The negative position should be as long as the duration of
                                // samples already written.
                                let written_duration = written_sample_count as f64 / sample_rate;
                                let negative_pos =
                                    PositionInSeconds::new_unchecked(-written_duration);
                                args.block.set_time_s(negative_pos);
                                args.block.set_length(desired_sample_count);
                                self.inner.get_samples(args.block);
                            } else {
                                // Let preview register know that complete buffer has been
                                // filled as desired in order to prevent retry (?) queries that
                                // lead to double events.
                                args.block.set_samples_out(desired_sample_count);
                            }
                        }
                    } else {
                        // Audio.
                        if pos < PositionInSeconds::ZERO {
                            // For audio, starting at a negative position leads to weird sounds.
                            // That's why we need to query from 0.0 and
                            // offset the provided sample buffer by that
                            // amount.
                            let sample_offset = (-pos.get() * sample_rate) as i32;
                            args.block.set_time_s(PositionInSeconds::ZERO);
                            with_shifted_samples(args.block, sample_offset, |b| {
                                self.inner.get_samples(b);
                            });
                        } else {
                            args.block.set_time_s(pos);
                            self.inner.get_samples(args.block);
                        }
                        let written_sample_count = args.block.samples_out();
                        if written_sample_count < desired_sample_count {
                            // We have reached the end of the clip and it doesn't fill the
                            // complete block.
                            if self.repeated {
                                // Repeat. Because we assume that the user cuts sources
                                // sample-perfect, we must immediately fill the rest of the
                                // buffer with the very
                                // beginning of the source.
                                // Audio. Start from zero and write just remaining samples.
                                args.block.set_time_s(PositionInSeconds::ZERO);
                                with_shifted_samples(args.block, written_sample_count, |b| {
                                    self.inner.get_samples(b);
                                });
                                // Let preview register know that complete buffer has been filled.
                                args.block.set_samples_out(desired_sample_count);
                            } else {
                                // Let preview register know that complete buffer has been
                                // filled as desired in order to prevent retry (?) queries.
                                args.block.set_samples_out(desired_sample_count);
                            }
                        }
                    }
                }
            },
            PrepareSuspension => {
                if self.is_midi {
                    send_all_notes_off(&args);
                }
                self.state = Suspended;
            }
        }
    }

    fn get_peak_info(&mut self, args: GetPeakInfoArgs) {
        unsafe {
            self.inner.get_peak_info(args.block);
        }
    }

    fn save_state(&mut self, args: SaveStateArgs) {
        unsafe {
            self.inner.save_state(args.context);
        }
    }

    fn load_state(&mut self, args: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unsafe { self.inner.load_state(args.first_line, args.context) }
    }

    fn peaks_clear(&mut self, args: PeaksClearArgs) {
        self.inner.peaks_clear(args.delete_file);
    }

    fn peaks_build_begin(&mut self) -> bool {
        self.inner.peaks_build_begin()
    }

    fn peaks_build_run(&mut self) -> bool {
        self.inner.peaks_build_run()
    }

    fn peaks_build_finish(&mut self) {
        self.inner.peaks_build_finish();
    }

    unsafe fn extended(&mut self, args: ExtendedArgs) -> i32 {
        match args.call {
            EXT_QUERY_STATE => self.query_state().into(),
            EXT_SCHEDULE_START => {
                let pos: PositionInSeconds = *(args.parm_1 as *mut _);
                let pos = if pos.get() < 0.0 { None } else { Some(pos) };
                let repeated: bool = *(args.parm_2 as *mut _);
                self.schedule_start(pos, repeated);
                1
            }
            EXT_RETRIGGER => {
                self.retrigger();
                1
            }
            EXT_SCHEDULE_STOP => {
                let pos: ClipStopPosition = *(args.parm_1 as *mut _);
                self.schedule_stop(pos);
                1
            }
            EXT_SUSPEND => {
                self.suspend();
                1
            }
            EXT_BACKPEDAL_FROM_SCHEDULED_STOP => {
                self.backpedal_from_scheduled_stop();
                1
            }
            EXT_ADJUST_START_POS_BY => {
                let delta: PositionInSeconds = *(args.parm_1 as *mut _);
                self.adjust_start_pos_by(delta);
                1
            }
            EXT_QUERY_INNER_LENGTH => {
                *(args.parm_1 as *mut f64) = self.query_inner_length().get();
                1
            }
            EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED => {
                *(args.parm_1 as *mut f64) = if let Some(pos) = self.pos_within_clip() {
                    pos.get()
                } else {
                    f64::NAN
                };
                1
            }
            EXT_ENABLE_REPEAT => {
                self.set_repeated(true);
                1
            }
            EXT_DISABLE_REPEAT => {
                self.set_repeated(false);
                1
            }
            _ => self
                .inner
                .extended(args.call, args.parm_1, args.parm_2, args.parm_3),
        }
    }
}

fn send_all_notes_off(args: &GetSamplesArgs) {
    for ch in 0..16 {
        let all_notes_off = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_NOTES_OFF,
            U7::MIN,
        );
        let all_sound_off = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_SOUND_OFF,
            U7::MIN,
        );
        add_midi_event(args, all_notes_off);
        add_midi_event(args, all_sound_off);
    }
}

fn add_midi_event(args: &GetSamplesArgs, msg: RawShortMessage) {
    let mut event = MidiEvent::default();
    event.set_message(msg);
    args.block.midi_event_list().add_item(&event);
}

pub trait ClipPcmSourceSkills {
    /// Returns the state of this clip source.
    fn query_state(&self) -> ClipPcmSourceState;

    /// Starts or at schedules playing of the clip.
    ///
    /// If position is set, this method schedules playing of the clip. If not, it starts playing it
    /// immediately.
    // TODO-high Also distinguish between schedule and play immediately.
    fn schedule_start(&mut self, pos: Option<PositionInSeconds>, repeated: bool);

    /// Retriggers the clip (if currently playing).
    fn retrigger(&mut self);

    /// Schedules clip stop.
    fn schedule_stop(&mut self, pos: ClipStopPosition);

    /// Suspends playback, immediately but sending note/sound off before in case of MIDI.
    fn suspend(&mut self);

    /// "Undoes" a scheduled stop if user changes their mind.
    fn backpedal_from_scheduled_stop(&mut self);

    /// Nudges the (scheduled) start position by the given amount.
    ///
    /// Setting negative values is equivalent to moving the play cursor in the right direction.
    /// Setting positive values is equivalent to moving the play cursor in the left direction.
    ///
    /// This only has an effect if the clip is not stopped.
    fn adjust_start_pos_by(&mut self, delta: PositionInSeconds);

    /// Returns the clip length.
    ///
    /// The clip length is different from the clip source length. The clip source length is infinite
    /// because it just acts as a sort of virtual track).
    fn query_inner_length(&self) -> DurationInSeconds;

    /// Changes whether to repeat or not repeat the clip.
    fn set_repeated(&mut self, repeated: bool);

    /// Returns the position within the clip.
    ///
    /// - Considers repeat.
    /// - Returns negative position if clip not yet playing.
    /// - Returns `None` if not scheduled, if beyond scheduled stop or if clip length is zero.
    /// - Returns (hypothetical) position even if not playing!
    fn pos_within_clip(&self) -> Option<PositionInSeconds>;
}

impl ClipPcmSourceSkills for ClipPcmSource {
    fn query_state(&self) -> ClipPcmSourceState {
        self.state
    }

    fn schedule_start(&mut self, pos: Option<PositionInSeconds>, repeated: bool) {
        let resolved_pos = pos.unwrap_or_else(|| self.timeline_cursor_pos());
        self.start_pos = Some(resolved_pos);
        self.stop_pos = None;
        self.repeated = repeated;
        self.state = ClipPcmSourceState::Playing;
    }

    fn retrigger(&mut self) {
        self.start_pos = Some(self.timeline_cursor_pos());
    }

    fn schedule_stop(&mut self, pos: ClipStopPosition) {
        let resolved_stop_pos = match pos {
            ClipStopPosition::At(pos) => pos,
            ClipStopPosition::AtEndOfClip => match self.start_pos {
                None => return,
                Some(scheduled_start_pos) => {
                    let pos = scheduled_start_pos.get()
                        + self.offset.get()
                        + self.query_inner_length().get();
                    PositionInSeconds::new(pos)
                }
            },
        };
        self.stop_pos = Some(resolved_stop_pos);
    }

    fn suspend(&mut self) {
        self.state = ClipPcmSourceState::PrepareSuspension;
    }

    fn backpedal_from_scheduled_stop(&mut self) {
        self.stop_pos = None;
    }

    fn adjust_start_pos_by(&mut self, delta: PositionInSeconds) {
        let current_start_pos = match self.start_pos {
            None => return,
            Some(p) => p,
        };
        self.start_pos = Some(current_start_pos + delta);
    }

    fn query_inner_length(&self) -> DurationInSeconds {
        self.inner.get_length().unwrap_or_default()
    }

    fn set_repeated(&mut self, repeated: bool) {
        self.repeated = repeated;
    }

    fn pos_within_clip(&self) -> Option<PositionInSeconds> {
        let pos_from_start = self.pos_from_start()?;
        self.calculate_pos_within_clip(pos_from_start)
    }
}

impl ClipPcmSourceSkills for BorrowedPcmSource {
    fn query_state(&self) -> ClipPcmSourceState {
        let state = unsafe { self.extended(EXT_QUERY_STATE, null_mut(), null_mut(), null_mut()) };
        state.try_into().expect("invalid state")
    }

    fn schedule_start(&mut self, pos: Option<PositionInSeconds>, mut repeated: bool) {
        let mut raw_pos = if let Some(p) = pos { p.get() } else { -1.0 };
        unsafe {
            self.extended(
                EXT_SCHEDULE_START,
                &mut raw_pos as *mut _ as _,
                &mut repeated as *mut _ as _,
                null_mut(),
            );
        }
    }

    fn retrigger(&mut self) {
        unsafe {
            self.extended(EXT_RETRIGGER, null_mut(), null_mut(), null_mut());
        }
    }

    fn schedule_stop(&mut self, mut pos: ClipStopPosition) {
        unsafe {
            self.extended(
                EXT_SCHEDULE_STOP,
                &mut pos as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn suspend(&mut self) {
        unsafe {
            self.extended(EXT_SUSPEND, null_mut(), null_mut(), null_mut());
        }
    }

    fn backpedal_from_scheduled_stop(&mut self) {
        unsafe {
            self.extended(
                EXT_BACKPEDAL_FROM_SCHEDULED_STOP,
                null_mut(),
                null_mut(),
                null_mut(),
            );
        }
    }

    fn adjust_start_pos_by(&mut self, mut delta: PositionInSeconds) {
        unsafe {
            self.extended(
                EXT_ADJUST_START_POS_BY,
                &mut delta as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn query_inner_length(&self) -> DurationInSeconds {
        let mut l = 0.0;
        unsafe {
            self.extended(
                EXT_QUERY_INNER_LENGTH,
                &mut l as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        DurationInSeconds::new(l)
    }

    fn set_repeated(&mut self, repeated: bool) {
        let request = if repeated {
            EXT_ENABLE_REPEAT
        } else {
            EXT_DISABLE_REPEAT
        };
        unsafe {
            self.extended(request, null_mut(), null_mut(), null_mut());
        }
    }

    fn pos_within_clip(&self) -> Option<PositionInSeconds> {
        let mut p = f64::NAN;
        unsafe {
            self.extended(
                EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED,
                &mut p as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        if p.is_nan() {
            return None;
        }
        Some(PositionInSeconds::new(p))
    }
}

trait PositionInSecondsExt {
    fn has_reached(self, stop_pos: PositionInSeconds) -> bool;
}

impl PositionInSecondsExt for PositionInSeconds {
    fn has_reached(self, stop_pos: PositionInSeconds) -> bool {
        self > stop_pos || (stop_pos - self).get() < BASE_EPSILON
    }
}

unsafe fn with_shifted_samples(
    block: &mut BorrowedPcmSourceTransfer,
    offset: i32,
    f: impl FnOnce(&mut BorrowedPcmSourceTransfer),
) {
    // Shift samples.
    let original_length = block.length();
    let original_samples = block.samples();
    let shifted_samples = original_samples.offset((offset * block.nch()) as _);
    block.set_length(block.length() - offset);
    block.set_samples(shifted_samples);
    // Query inner source.
    f(block);
    // Unshift samples.
    block.set_length(original_length);
    block.set_samples(original_samples);
}
