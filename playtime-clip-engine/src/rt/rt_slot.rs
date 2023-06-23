use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::rt::supplier::{MaterialInfo, WriteAudioRequest, WriteMidiRequest};
use crate::rt::{
    ApplyClipArgs, AudioBufMut, ClipProcessArgs, ClipProcessingOutcome, ClipRecordingPollArgs,
    ColumnProcessTransportChangeArgs, FillSlotMode, HandleSlotEvent, InternalClipPlayState,
    OverridableMatrixSettings, RtClip, RtClipId, RtColumnEvent, RtColumnEventSender,
    RtColumnGarbage, RtColumnSettings, SharedPeak, SharedPos, SlotInstruction, SlotLoadArgs,
    SlotLoadClipArgs, SlotPlayArgs, SlotRecordInstruction, SlotStopArgs,
};
use crate::{ClipEngineResult, ErrorWithPayload, HybridTimeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use indexmap::IndexMap;
use playtime_api::persistence::{ClipPlayStopTiming, SlotId};
use playtime_api::runtime::ClipPlayState;
use reaper_medium::{Bpm, PcmSourceTransfer, PlayState, PositionInSeconds};
use std::{cmp, mem};
use xxhash_rust::xxh3::Xxh3Builder;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RtSlotId(u64);

impl RtSlotId {
    pub fn from_slot_id(slot_id: &SlotId) -> Self {
        Self(base::hash_util::calculate_non_crypto_hash(slot_id))
    }
}

pub type RtClips = IndexMap<RtClipId, RtClip, Xxh3Builder>;

#[derive(Debug, Default)]
pub struct RtSlot {
    id: RtSlotId,
    pub(crate) clips: RtClips,
    /// This is a vec of an option because RtClip doesn't implement Default.
    retired_clips: Vec<Option<RtClip>>,
    runtime_data: InternalRuntimeData,
}

#[derive(Debug, Default)]
struct InternalRuntimeData {
    last_play_state: InternalClipPlayState,
    stop_was_caused_by_transport_change: bool,
}

impl RtSlot {
    pub fn new(id: RtSlotId, clips: RtClips) -> Self {
        Self {
            id,
            retired_clips: Vec::with_capacity(clips.len()),
            clips,
            runtime_data: Default::default(),
        }
    }

    /// Replaces the clips in this slot with the given ones but keeps unchanged clips playing if
    /// possible and fades out still playing old clips.
    ///
    /// Exploits the given clips and replaces them with trash! They should not be used anymore.
    pub fn load(&mut self, mut args: SlotLoadArgs) {
        // Take old clips out
        let mut old_clips = mem::take(&mut self.clips);
        // For each new clip, check if there's a corresponding old clip. In that case, update
        // the old clip instead of completely replacing it with the new one. This keeps unchanged
        // playing clips playing.
        for (_, new_clip) in &mut args.new_clips {
            if let Some(mut old_clip) = old_clips.remove(&new_clip.id()) {
                // We have an old clip with the same ID. Reuse it for smooth transition!
                // Apply the new clip's settings to the old clip.
                let apply_args = ApplyClipArgs {
                    other_clip: new_clip,
                    matrix_settings: args.matrix_settings,
                    column_settings: args.column_settings,
                };
                let _ = old_clip.apply(apply_args);
                // Declare the old clip to be the new clip
                let obsolete_clip = mem::replace(new_clip, old_clip);
                // Dispose the obsolete clip
                args.event_sender
                    .dispose(RtColumnGarbage::Clip(Some(obsolete_clip)));
            }
        }
        // Declare the mixture of updated and new clips as the new clip collection!
        self.clips = args.new_clips;
        // Retire old and now unused clips
        retire_clips(&mut old_clips, &mut self.retired_clips);
        // The old slot might actually still be playing. Notify listeners.
        args.event_sender
            .slot_play_state_changed(self.id, self.runtime_data.last_play_state);
        // Dispose old and now empty clip collection
        args.event_sender.dispose(RtColumnGarbage::Clips(old_clips));
    }

    /// Replaces the given clip in this slot with the given one but keeps the clip playing if
    /// possible and fades out still playing old clips.
    ///
    /// Exploits the given clip and replaces it with trash! It should not be used anymore.
    pub fn load_clip(&mut self, args: SlotLoadClipArgs) -> ClipEngineResult<()> {
        let old_clip = self.get_clip_mut(args.clip_index)?;
        // Apply the new clip's settings to the old clip.
        let apply_args = ApplyClipArgs {
            other_clip: args.new_clip,
            matrix_settings: args.matrix_settings,
            column_settings: args.column_settings,
        };

        old_clip.apply(apply_args)?;
        Ok(())
    }

    pub fn id(&self) -> RtSlotId {
        self.id
    }

    pub fn last_play_state(&self) -> InternalClipPlayState {
        self.runtime_data.last_play_state
    }

    /// Returns the index at which the clip landed.
    pub fn fill(&mut self, clip: RtClip, mode: FillSlotMode) -> usize {
        // TODO-medium Suspend previous clip if playing.
        match mode {
            FillSlotMode::Add => {
                self.clips.insert(clip.id(), clip);
                self.clips.len() - 1
            }
            FillSlotMode::Replace => {
                self.clips.clear();
                self.clips.insert(clip.id(), clip);
                0
            }
        }
    }

    pub fn is_filled(&self) -> bool {
        !self.clips.is_empty()
    }

    pub fn find_clip(&self, index: usize) -> Option<&RtClip> {
        Some(self.clips.get_index(index)?.1)
    }

    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }

    pub fn first_clip(&self) -> Option<&RtClip> {
        Some(self.clips.first()?.1)
    }

    /// See [`RtClip::recording_poll`].
    pub fn recording_poll<H: HandleSlotEvent>(
        &mut self,
        args: ClipRecordingPollArgs,
        event_handler: &H,
    ) -> bool {
        match self.get_clip_mut(0) {
            Ok(clip) => clip.recording_poll(args, event_handler),
            Err(_) => false,
        }
    }

    /// Plays all clips in this slot.
    pub fn play(&mut self, args: SlotPlayArgs) -> ClipEngineResult<()> {
        for clip in self.get_clips_mut()? {
            clip.play(args)?;
        }
        Ok(())
    }

    /// Stops the slot immediately, initiating fade-outs if necessary.
    ///
    /// Also stops recording. Consumer should just wait for the slot to be stopped and then not use
    /// it anymore.
    pub fn initiate_removal(&mut self) {
        for clip in self.clips.values_mut() {
            clip.initiate_removal()
        }
    }

    /// Stops the slot immediately, initiating fade-outs if necessary.
    ///
    /// Doesn't touch recording.
    pub fn panic(&mut self) {
        self.runtime_data.stop_was_caused_by_transport_change = false;
        for clip in self.clips.values_mut() {
            clip.panic();
        }
    }

    /// Stops all clips in this slot.
    pub fn stop<H: HandleSlotEvent>(
        &mut self,
        args: SlotStopArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        self.runtime_data.stop_was_caused_by_transport_change = false;
        let mut instruction = None;
        for clip in self.get_clips_mut()? {
            let inst = clip.stop(args, event_handler)?;
            if let Some(inst) = inst {
                instruction = Some(inst);
            }
        }
        if let Some(instruction) = instruction {
            self.process_instruction(instruction, event_handler);
        }
        Ok(())
    }

    fn process_instruction<H: HandleSlotEvent>(
        &mut self,
        instruction: SlotInstruction,
        event_handler: &H,
    ) {
        use SlotInstruction::*;
        match instruction {
            ClearSlot => {
                self.clear_internal(event_handler);
            }
        }
    }

    /// Removes all the clips in this slot.
    pub fn clear(&mut self) {
        retire_clips(&mut self.clips, &mut self.retired_clips);
    }

    fn clear_internal<H: HandleSlotEvent>(&mut self, event_handler: &H) {
        debug!("Clearing real-time slot");
        if self.clips.is_empty() {
            return;
        }
        let old_clips = mem::take(&mut self.clips);
        event_handler.slot_cleared(old_clips);
        self.runtime_data = InternalRuntimeData::default();
    }

    /// # Errors
    ///
    /// Returns an error either if the instruction is to record on the given new clip but the slot
    /// is not empty, or if the instruction is to record on an existing clip but the slot is empty.
    ///
    /// In both cases, it returns the instruction itself so it can be disposed appropriately.
    pub fn record_clip(
        &mut self,
        instruction: SlotRecordInstruction,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &RtColumnSettings,
    ) -> Result<Option<SlotRuntimeData>, ErrorWithPayload<SlotRecordInstruction>> {
        use SlotRecordInstruction::*;
        match instruction {
            NewClip(instruction) => {
                debug!("Record new clip");
                if !self.clips.is_empty() {
                    return Err(ErrorWithPayload::new(
                        "slot not empty",
                        NewClip(instruction),
                    ));
                }
                let clip = RtClip::recording(instruction);
                let runtime_data =
                    SlotRuntimeData::new(&clip, true).expect("no material info in record_clip");
                self.clips.insert(clip.id(), clip);
                Ok(Some(runtime_data))
            }
            ExistingClip(args) => {
                debug!("Record with existing clip");
                let clip = match self.clips.first_mut() {
                    None => {
                        return Err(ErrorWithPayload::new("slot empty", ExistingClip(args)));
                    }
                    Some(c) => c.1,
                };
                match clip.record(args, matrix_settings, column_settings) {
                    Ok(_) => {
                        let runtime_data = SlotRuntimeData::new(clip, true)
                            .expect("no material info in record_clip");
                        Ok(Some(runtime_data))
                    }
                    Err(e) => Err(e.map_payload(ExistingClip)),
                }
            }
            MidiOverdub(instruction) => {
                debug!("MIDI overdub");
                match self.get_clip_mut(instruction.clip_index) {
                    Ok(clip) => match clip.midi_overdub(instruction) {
                        Ok(_) => Ok(None),
                        Err(e) => Err(e.map_payload(MidiOverdub)),
                    },
                    Err(e) => Err(ErrorWithPayload::new(e, MidiOverdub(instruction))),
                }
            }
        }
    }

    pub fn pause(&mut self) -> ClipEngineResult<()> {
        for clip in self.get_clips_mut()? {
            clip.pause();
        }
        Ok(())
    }

    pub fn seek(&mut self, desired_pos: UnitValue) -> ClipEngineResult<()> {
        for clip in self.get_clips_mut()? {
            clip.seek(desired_pos)?;
        }
        Ok(())
    }

    pub fn write_clip_midi(&mut self, request: WriteMidiRequest) -> ClipEngineResult<()> {
        self.get_clip_mut(0)?.write_midi(request);
        Ok(())
    }

    pub fn write_clip_audio(&mut self, request: impl WriteAudioRequest) -> ClipEngineResult<()> {
        self.get_clip_mut(0)?.write_audio(request);
        Ok(())
    }

    pub fn get_clip_mut(&mut self, index: usize) -> ClipEngineResult<&mut RtClip> {
        Ok(self.clips.get_index_mut(index).ok_or(CLIP_DOESNT_EXIST)?.1)
    }

    pub fn process_transport_change<H: HandleSlotEvent>(
        &mut self,
        args: &SlotProcessTransportChangeArgs,
        event_handler: &H,
    ) -> ClipEngineResult<()> {
        let mut instruction = None;
        {
            for clip in self.clips.values_mut() {
                let inst = match args.column_args.change {
                    TransportChange::PlayState(rel_change) => {
                        // We have a relevant transport change.
                        let state = clip.play_state();
                        use ClipPlayState::*;
                        use RelevantPlayStateChange::*;
                        match rel_change {
                            PlayAfterStop => {
                                match state.get() {
                                    Stopped
                                        if self
                                            .runtime_data
                                            .stop_was_caused_by_transport_change =>
                                    {
                                        // REAPER transport was started from stopped state. Clip is stopped
                                        // as well and was put in that state due to a previous transport
                                        // stop. Play the clip!
                                        play_clip_by_transport(clip, args)
                                    }
                                    ScheduledForPlayStart | Playing | ScheduledForPlayStop => {
                                        // Retrigger (timeline switch)
                                        play_clip_by_transport(clip, args)
                                    }
                                    Stopped
                                    | Paused
                                    | Recording
                                    | ScheduledForRecordingStart
                                    | ScheduledForRecordingStop => {
                                        // Stop and forget.
                                        self.runtime_data.stop_clip_by_transport(
                                            clip,
                                            args,
                                            false,
                                            event_handler,
                                        )?
                                    }
                                }
                            }
                            StopAfterPlay => match state.get() {
                                ScheduledForPlayStart
                                | Playing
                                | ScheduledForPlayStop
                                | Recording
                                | ScheduledForRecordingStart
                                | ScheduledForRecordingStop => {
                                    // Stop and memorize
                                    self.runtime_data.stop_clip_by_transport(
                                        clip,
                                        args,
                                        true,
                                        event_handler,
                                    )?
                                }

                                Stopped | Paused => {
                                    // Stop and forget
                                    self.runtime_data.stop_clip_by_transport(
                                        clip,
                                        args,
                                        false,
                                        event_handler,
                                    )?
                                }
                            },
                            StopAfterPause => self.runtime_data.stop_clip_by_transport(
                                clip,
                                args,
                                false,
                                event_handler,
                            )?,
                        }
                    }
                    TransportChange::PlayCursorJump => {
                        // The play cursor was repositioned.
                        let play_state = clip.play_state();
                        use ClipPlayState::*;
                        if !matches!(
                            play_state.get(),
                            ScheduledForPlayStart | Playing | ScheduledForPlayStop
                        ) {
                            return Ok(());
                        }
                        play_clip_by_transport(clip, args)
                    }
                };
                if let Some(inst) = inst {
                    // Right now there's only one instruction we can have, so this is okay.
                    instruction = Some(inst);
                }
            }
        };
        if let Some(instruction) = instruction {
            self.process_instruction(instruction, event_handler);
        }
        Ok(())
    }

    pub fn process(&mut self, args: &mut SlotProcessArgs) -> SlotProcessingOutcome {
        // Our strategy is to always write all available source channels into the mix
        // buffer. From a performance perspective, it would actually be enough to take
        // only as many channels as we need (= track channel count). However, always using
        // the source channel count as reference is much simpler, in particular when it
        // comes to caching and pre-buffering. Also, in practice this is rarely an issue.
        // Most samples out there used in typical stereo track setups have no more than 2
        // channels. And if they do, the user can always down-mix to the desired channel
        // count up-front.
        let mut num_audio_frames_written = 0;
        // Fade out retired clips
        self.retired_clips.retain_mut(|clip| {
            let keep = if let Some(clip) = clip.as_mut() {
                let outcome = process_clip(clip, args, &mut num_audio_frames_written);
                // As long as the clip still wrote audio frames, we keep it in memory. But as soon
                // as no audio frames are written anymore, we can safely assume it's stopped and
                // drop it.
                outcome.num_audio_frames_written > 0
            } else {
                false
            };
            // If done, dispose the slot in order to avoid deallocation in real-time thread
            if !keep {
                args.event_sender
                    .dispose(RtColumnGarbage::Clip(clip.take()));
            }
            keep
        });
        // Play current clips
        let mut new_slot_play_state = InternalClipPlayState::default();
        for clip in self.clips.values_mut() {
            process_clip(clip, args, &mut num_audio_frames_written);
            // Aggregate clip play states into slot play state
            new_slot_play_state = cmp::max(new_slot_play_state, clip.play_state());
        }
        let last_play_state =
            mem::replace(&mut self.runtime_data.last_play_state, new_slot_play_state);
        SlotProcessingOutcome {
            changed_play_state: if new_slot_play_state != last_play_state {
                Some(new_slot_play_state)
            } else {
                None
            },
            num_audio_frames_written,
        }
    }

    pub fn is_stoppable(&self) -> bool {
        self.clips.values().any(|c| c.play_state().is_stoppable())
    }

    fn get_clips_mut(&mut self) -> ClipEngineResult<impl Iterator<Item = &mut RtClip>> {
        if self.clips.is_empty() {
            return Err(SLOT_NOT_FILLED);
        }
        Ok(self.clips.values_mut())
    }
}

impl InternalRuntimeData {
    fn stop_clip_by_transport<H: HandleSlotEvent>(
        &mut self,
        clip: &mut RtClip,
        args: &SlotProcessTransportChangeArgs,
        keep_starting_with_transport: bool,
        event_handler: &H,
    ) -> ClipEngineResult<Option<SlotInstruction>> {
        self.stop_was_caused_by_transport_change = keep_starting_with_transport;
        let args = SlotStopArgs {
            stop_timing: Some(ClipPlayStopTiming::Immediately),
            timeline: &args.column_args.timeline,
            ref_pos: Some(args.column_args.timeline_cursor_pos),
            enforce_play_stop: true,
            matrix_settings: args.matrix_settings,
            column_settings: args.column_settings,
            audio_request_props: args.column_args.audio_request_props,
        };
        clip.stop(args, event_handler)
    }
}

#[derive(Clone, Debug)]
pub struct SlotProcessTransportChangeArgs<'a> {
    pub column_args: &'a ColumnProcessTransportChangeArgs,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
}

const SLOT_NOT_FILLED: &str = "slot not filled";
const CLIP_DOESNT_EXIST: &str = "clip doesn't exist";

#[derive(Copy, Clone, Debug)]
pub enum TransportChange {
    PlayState(RelevantPlayStateChange),
    PlayCursorJump,
}
#[derive(Copy, Clone, Debug)]
pub enum RelevantPlayStateChange {
    PlayAfterStop,
    StopAfterPlay,
    StopAfterPause,
}

impl RelevantPlayStateChange {
    pub fn from_play_state_change(old: PlayState, new: PlayState) -> Option<Self> {
        use RelevantPlayStateChange::*;
        let change = if !old.is_paused && !old.is_playing && new.is_playing {
            PlayAfterStop
        } else if old.is_playing && !new.is_playing && !new.is_paused {
            StopAfterPlay
        } else if old.is_paused && !new.is_playing && !new.is_paused {
            StopAfterPause
        } else {
            return None;
        };
        Some(change)
    }
}

pub struct SlotProcessingOutcome {
    pub changed_play_state: Option<InternalClipPlayState>,
    pub num_audio_frames_written: usize,
}

fn play_clip_by_transport(
    clip: &mut RtClip,
    args: &SlotProcessTransportChangeArgs,
) -> Option<SlotInstruction> {
    let args = SlotPlayArgs {
        timeline: &args.column_args.timeline,
        ref_pos: Some(args.column_args.timeline_cursor_pos),
        matrix_settings: args.matrix_settings,
        column_settings: args.column_settings,
        start_timing: None,
    };
    clip.play(args).unwrap();
    None
}

#[derive(Clone, Debug)]
pub struct SlotRuntimeData {
    pub play_state: InternalClipPlayState,
    pub pos: SharedPos,
    pub peak: SharedPeak,
    /// The frame count in this material info is supposed to take the section bounds into account.
    pub material_info: MaterialInfo,
}

impl SlotRuntimeData {
    pub fn new(clip: &RtClip, use_recording_material_info: bool) -> ClipEngineResult<Self> {
        let data = Self {
            play_state: clip.play_state(),
            pos: clip.shared_pos(),
            peak: clip.shared_peak(),
            material_info: if use_recording_material_info {
                clip.recording_material_info()?
            } else {
                clip.material_info()?
            },
        };
        Ok(data)
    }

    pub fn mod_frame(&self) -> isize {
        let frame = self.pos.get();
        if frame < 0 {
            frame
        } else if self.material_info.frame_count() > 0 {
            frame % self.material_info.frame_count() as isize
        } else {
            0
        }
    }

    pub fn proportional_position(&self) -> ClipEngineResult<UnitValue> {
        let pos = self.pos.get();
        if pos < 0 {
            return Err("count-in phase");
        }
        let frame_count = self.material_info.frame_count();
        if frame_count == 0 {
            return Err("frame count is zero");
        }
        let mod_pos = pos as usize % frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Ok(proportional)
    }

    pub fn position_in_seconds_during_recording(&self, timeline_tempo: Bpm) -> PositionInSeconds {
        let tempo_factor = self
            .material_info
            .tempo_factor_during_recording(timeline_tempo);
        self.position_in_seconds(tempo_factor)
    }

    pub fn position_in_seconds(&self, tempo_factor: f64) -> PositionInSeconds {
        let pos_in_source_frames = self.mod_frame();
        let pos_in_secs = convert_position_in_frames_to_seconds(
            pos_in_source_frames,
            self.material_info.frame_rate(),
        );
        adjust_pos_in_secs_anti_proportionally(pos_in_secs, tempo_factor)
    }

    pub fn peak(&self) -> UnitValue {
        self.peak.reset()
    }
}

fn process_clip(
    clip: &mut RtClip,
    args: &mut SlotProcessArgs,
    total_num_audio_frames_written: &mut usize,
) -> ClipProcessingOutcome {
    let clip_channel_count = {
        match clip.material_info() {
            Ok(info) => info.channel_count(),
            // If the clip doesn't have material, it's probably recording. We still
            // allow the slot to process because it could propagate some play state
            // changes. With a channel count of zero though.
            Err(_) => 0,
        }
    };
    let mut mix_buffer = AudioBufMut::from_slice(
        args.mix_buffer_chunk,
        clip_channel_count,
        args.block.length() as _,
    )
    .unwrap();
    let mut inner_args = ClipProcessArgs {
        dest_buffer: &mut mix_buffer,
        dest_sample_rate: args.block.sample_rate(),
        midi_event_list: args
            .block
            .midi_event_list_mut()
            .expect("no MIDI event list available"),
        timeline: args.timeline,
        timeline_cursor_pos: args.timeline_cursor_pos,
        timeline_tempo: args.timeline_tempo,
        resync: args.resync,
        matrix_settings: args.matrix_settings,
        column_settings: args.column_settings,
    };
    let outcome = clip.process(&mut inner_args);
    // Aggregate number of written audio frames
    *total_num_audio_frames_written = cmp::max(
        *total_num_audio_frames_written,
        outcome.num_audio_frames_written,
    );
    // Write from mix buffer to destination buffer
    if outcome.num_audio_frames_written > 0 {
        let mut output_buffer = unsafe { AudioBufMut::from_pcm_source_transfer(args.block) };
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
    outcome
}

pub struct SlotProcessArgs<'a> {
    pub block: &'a mut PcmSourceTransfer,
    pub mix_buffer_chunk: &'a mut [f64],
    pub timeline: &'a HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
    pub resync: bool,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
    pub event_sender: &'a Sender<RtColumnEvent>,
}

fn retire_clips(clips: &mut RtClips, retired_clips: &mut Vec<Option<RtClip>>) {
    for (_, mut clip) in clips.drain(..) {
        clip.initiate_removal();
        retired_clips.push(Some(clip))
    }
}
