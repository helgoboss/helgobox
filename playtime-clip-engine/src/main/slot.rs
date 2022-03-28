use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::main::{
    Clip, ClipMatrixHandler, ClipRecordDestination, ClipRecordHardwareInput,
    ClipRecordHardwareMidiInput, ClipRecordInput, ClipRecordTask, VirtualClipRecordAudioInput,
    VirtualClipRecordHardwareMidiInput,
};
use crate::rt::supplier::{
    ChainEquipment, MaterialInfo, Recorder, RecorderRequest, RecordingArgs, RecordingEquipment,
    SupplierChain, MIDI_BASE_BPM,
};
use crate::rt::tempo_util::calc_tempo_factor;
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ClipRecordArgs, ColumnCommandSender, ColumnSetClipLoopedArgs,
    NormalRecordingOutcome, OverridableMatrixSettings, RecordNewClipInstruction, SharedColumn,
    SlotRecordInstruction, SlotRuntimeData,
};
use crate::{clip_timeline, rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ChannelRange, ColumnClipRecordSettings, Db, MatrixClipRecordSettings, MidiClipRecordMode,
    RecordOrigin,
};
use reaper_high::{Guid, Project, Track, TrackRoute};
use reaper_medium::{OwnedPcmSource, PositionInSeconds, RecordingInput};
use std::mem;

#[derive(Clone, Debug)]
pub struct Slot {
    index: usize,
    /// If this is set, the slot contains a clip.
    ///
    /// This means one of the following things:
    ///
    /// - The clip is active and can be playing, stopped etc.
    /// - The clip is active and is currently being MIDI-overdubbed.
    /// - The clip is inactive, which means it's about to be replaced with different clip content
    ///   that's in the process of being recorded right now.
    content: Option<Content>,
    state: SlotState,
    /// Route which is just created temporarily for recording.
    temporary_route: Option<TrackRoute>,
}

#[derive(Clone, Debug)]
struct Content {
    clip: Clip,
    runtime_data: SlotRuntimeData,
}

impl Slot {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            content: None,
            state: Default::default(),
            temporary_route: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_none() && matches!(&self.state, SlotState::Normal)
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns `None` if this slot doesn't need to be saved (because it's empty).
    pub fn save(&self) -> Option<api::Slot> {
        let content = self.content.as_ref()?;
        let api_slot = api::Slot {
            row: self.index,
            clip: Some(content.clip.save()),
        };
        Some(api_slot)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_clip<H: ClipMatrixHandler>(
        &mut self,
        matrix_record_settings: &MatrixClipRecordSettings,
        column_record_settings: &ColumnClipRecordSettings,
        rt_column_settings: &rt::ColumnSettings,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        handler: &H,
        containing_track: Option<&Track>,
        overridable_matrix_settings: &OverridableMatrixSettings,
        playback_track: &Track,
        rt_column: &SharedColumn,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<()> {
        // Check preconditions.
        match &self.state {
            SlotState::Normal => {}
            SlotState::RecordingOrOverdubbingRequested => {
                return Err("recording or overdubbing requested already");
            }
            SlotState::Recording(_) => {
                return Err("recording already");
            }
        };
        let (has_existing_clip, midi_overdub_instruction) = match &self.content {
            None => (false, None),
            Some(content) => {
                if content.runtime_data.play_state.is_somehow_recording() {
                    return Err("recording already according to play state");
                }
                use MidiClipRecordMode::*;
                let want_midi_overdub = match matrix_record_settings.midi_settings.record_mode {
                    Normal => false,
                    Overdub | Replace => {
                        // Only allow MIDI overdub if existing clip is a MIDI clip already.
                        content.runtime_data.material_info.is_midi()
                    }
                };
                let midi_overdub_instruction = if want_midi_overdub {
                    let midi_overdub_instruction = content.clip.create_midi_overdub_instruction(
                        playback_track.project(),
                        matrix_record_settings.midi_settings.record_mode,
                        matrix_record_settings.midi_settings.auto_quantize,
                    )?;
                    Some(midi_overdub_instruction)
                } else {
                    None
                };
                (true, midi_overdub_instruction)
            }
        };
        // Prepare tasks, equipment, instructions.
        let record_stuff = create_clip_record_stuff(
            self.index,
            containing_track,
            matrix_record_settings,
            column_record_settings,
            playback_track,
            rt_column,
            midi_overdub_instruction.is_some(),
        )?;
        let midi_overdub_instruction = if record_stuff.task.destination.is_midi_overdub {
            midi_overdub_instruction
        } else {
            // Want overdub but we have a audio input, so don't use overdub mode after all.
            None
        };
        let instruction = if let Some(midi_overdub_instruction) = midi_overdub_instruction {
            // We can do MIDI overdub. This is the easiest thing and needs almost no preparation.
            SlotRecordInstruction::MidiOverdub(midi_overdub_instruction)
        } else {
            // We record completely new material.
            let args = ClipRecordArgs {
                recording_equipment: record_stuff.recording_equipment,
                settings: *matrix_record_settings,
            };
            if has_existing_clip {
                // There's a clip already. That makes it easy because we have the clip struct
                // already, including the complete clip supplier chain, and can reuse it.
                SlotRecordInstruction::ExistingClip(args)
            } else {
                // There's no clip yet so we need to create the clip including the complete supplier
                // chain from scratch. We need to do create much of the stuff here already because
                // we must not allocate in the real-time thread. However, we can't create the
                // complete clip because we don't have enough information (block length, timeline
                // frame rate) available at this point to resolve the initial recording position.
                let recording_args = RecordingArgs::from_stuff(
                    Some(playback_track.project()),
                    rt_column_settings,
                    overridable_matrix_settings,
                    &args.settings,
                    args.recording_equipment,
                );
                let timeline = clip_timeline(Some(playback_track.project()), false);
                let timeline_cursor_pos = timeline.cursor_pos();
                let recorder = Recorder::recording(recording_args, recorder_request_sender.clone());
                let supplier_chain = SupplierChain::new(recorder, chain_equipment.clone())?;
                let new_clip_instruction = RecordNewClipInstruction {
                    supplier_chain,
                    project: Some(playback_track.project()),
                    shared_pos: Default::default(),
                    timeline,
                    timeline_cursor_pos,
                    settings: *matrix_record_settings,
                };
                SlotRecordInstruction::NewClip(new_clip_instruction)
            }
        };
        // Above code was only for checking preconditions and preparing stuff.
        // Here we do the actual state changes and distribute tasks.
        self.state = SlotState::RecordingOrOverdubbingRequested;
        column_command_sender.record_clip(self.index, instruction);
        self.temporary_route = record_stuff.temporary_route;
        handler.request_recording_input(record_stuff.task);
        Ok(())
    }

    fn remove_temporary_route(&mut self) {
        if let Some(route) = self.temporary_route.take() {
            route.delete().unwrap();
        }
    }

    fn get_content(&self) -> ClipEngineResult<&Content> {
        self.content.as_ref().ok_or(SLOT_NOT_FILLED)
    }

    pub fn clip_volume(&self) -> ClipEngineResult<Db> {
        Ok(self.get_content()?.clip.volume())
    }

    pub fn clip_looped(&self) -> ClipEngineResult<bool> {
        Ok(self.get_content()?.clip.looped())
    }

    pub fn set_clip_volume(
        &mut self,
        volume: Db,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<()> {
        let content = get_content_mut(&mut self.content)?;
        content.clip.set_volume(volume);
        column_command_sender.set_clip_volume(self.index, volume);
        Ok(())
    }

    pub fn toggle_clip_looped(
        &mut self,
        column_command_sender: &ColumnCommandSender,
    ) -> ClipEngineResult<ClipChangedEvent> {
        let content = get_content_mut(&mut self.content)?;
        let looped = content.clip.toggle_looped();
        let args = ColumnSetClipLoopedArgs {
            slot_index: self.index,
            looped,
        };
        column_command_sender.set_clip_looped(args);
        Ok(ClipChangedEvent::ClipLooped(looped))
    }

    pub fn play_state(&self) -> ClipEngineResult<ClipPlayState> {
        use SlotState::*;
        match &self.state {
            Normal => Ok(self.get_content()?.runtime_data.play_state),
            RecordingOrOverdubbingRequested => Ok(ClipPlayState::ScheduledForRecordingStart),
            Recording(runtime_data) => Ok(runtime_data.play_state),
        }
    }

    fn runtime_data(&self) -> ClipEngineResult<&SlotRuntimeData> {
        use SlotState::*;
        match &self.state {
            Normal | RecordingOrOverdubbingRequested => Ok(&self.get_content()?.runtime_data),
            Recording(runtime_data) => Ok(runtime_data),
        }
    }

    fn runtime_data_mut(&mut self) -> ClipEngineResult<&mut SlotRuntimeData> {
        use SlotState::*;
        match &mut self.state {
            Normal | RecordingOrOverdubbingRequested => {
                Ok(&mut get_content_mut(&mut self.content)?.runtime_data)
            }
            Recording(runtime_data) => Ok(runtime_data),
        }
    }

    pub fn update_play_state(&mut self, play_state: ClipPlayState) -> ClipEngineResult<()> {
        self.runtime_data_mut()?.play_state = play_state;
        Ok(())
    }

    pub fn update_material_info(&mut self, material_info: MaterialInfo) -> ClipEngineResult<()> {
        self.runtime_data_mut()?.material_info = material_info;
        Ok(())
    }

    pub fn proportional_pos(&self) -> ClipEngineResult<UnitValue> {
        let runtime_data = self.runtime_data()?;
        let pos = runtime_data.pos.get();
        if pos < 0 {
            return Err("count-in phase");
        }
        let frame_count = runtime_data.material_info.frame_count();
        if frame_count == 0 {
            return Err("frame count is zero");
        }
        let mod_pos = pos as usize % frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Ok(proportional)
    }

    pub fn position_in_seconds(
        &self,
        timeline: &HybridTimeline,
    ) -> ClipEngineResult<PositionInSeconds> {
        let runtime_data = self.runtime_data()?;
        let pos_in_source_frames = runtime_data.mod_frame();
        let pos_in_secs = convert_position_in_frames_to_seconds(
            pos_in_source_frames,
            runtime_data.material_info.frame_rate(),
        );
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        let is_midi = runtime_data.material_info.is_midi();
        let tempo_factor = if let Ok(content) = self.get_content() {
            content.clip.tempo_factor(timeline_tempo, is_midi)
        } else if is_midi {
            calc_tempo_factor(MIDI_BASE_BPM, timeline_tempo)
        } else {
            // When recording audio, we have tempo factor 1.0 (original recording tempo).
            1.0
        };
        let tempo_adjusted_secs = adjust_pos_in_secs_anti_proportionally(pos_in_secs, tempo_factor);
        Ok(tempo_adjusted_secs)
    }

    pub fn fill_with(&mut self, clip: Clip, rt_clip: &rt::Clip) {
        let content = Content {
            clip,
            runtime_data: SlotRuntimeData {
                play_state: Default::default(),
                pos: rt_clip.shared_pos(),
                material_info: rt_clip.material_info().unwrap(),
            },
        };
        self.content = Some(content);
    }

    pub fn notify_recording_request_acknowledged(
        &mut self,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    ) -> ClipEngineResult<()> {
        use SlotState::*;
        match &mut self.state {
            Normal => Err("recording was not requested"),
            RecordingOrOverdubbingRequested => {
                self.state = match result {
                    Ok(runtime_data) => {
                        if let Some(runtime_data) = runtime_data {
                            // This must be a real recording, not overdub.
                            debug!("Acknowledged real recording");
                            SlotState::Recording(runtime_data)
                        } else {
                            // Must be overdubbing.
                            debug!("Acknowledged overdubbing");
                            SlotState::Normal
                        }
                    }
                    Err(_) => {
                        debug!("Recording request acknowledged with negative result");
                        self.remove_temporary_route();
                        SlotState::Normal
                    }
                };
                Ok(())
            }
            Recording(_) => Err("recording already"),
        }
    }

    pub fn notify_midi_overdub_finished(
        &mut self,
        mirror_source: OwnedPcmSource,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<()> {
        self.remove_temporary_route();
        get_content_mut(&mut self.content)?
            .clip
            .notify_midi_overdub_finished(mirror_source, temporary_project)
    }

    pub fn slot_cleared(&mut self) -> Option<ClipChangedEvent> {
        self.content.take().map(|_| ClipChangedEvent::Removed)
    }

    pub fn notify_normal_recording_finished(
        &mut self,
        outcome: NormalRecordingOutcome,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<Option<ClipChangedEvent>> {
        self.remove_temporary_route();
        match outcome {
            NormalRecordingOutcome::Committed(recording) => match mem::take(&mut self.state) {
                SlotState::Normal => Err("slot was not recording"),
                SlotState::RecordingOrOverdubbingRequested => {
                    Err("clip recording was not yet acknowledged")
                }
                SlotState::Recording(mut runtime_data) => {
                    let clip = Clip::from_recording(
                        recording.kind_specific,
                        recording.clip_settings,
                        temporary_project,
                    )?;
                    runtime_data.material_info = recording.material_info;
                    debug!("Fill slot with clip: {:#?}", &clip);
                    let content = Content { clip, runtime_data };
                    self.content = Some(content);
                    self.state = SlotState::Normal;
                    Ok(None)
                }
            },
            NormalRecordingOutcome::Canceled => {
                debug!("Recording canceled");
                self.state = SlotState::Normal;
                Ok(Some(ClipChangedEvent::Removed))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum SlotState {
    /// Either empty or filled (where filled can also mean that it's recording in overdub mode).
    Normal,
    /// Requested real recording or overdubbing.
    ///
    /// Used for preventing double record invocations.
    RecordingOrOverdubbingRequested,
    /// Recording (no overdub).
    Recording(SlotRuntimeData),
}

impl SlotState {
    pub fn is_as_good_as_recording(&self) -> bool {
        !matches!(self, Self::Normal)
    }
}

impl Default for SlotState {
    fn default() -> Self {
        SlotState::Normal
    }
}

fn get_content_mut(content: &mut Option<Content>) -> ClipEngineResult<&mut Content> {
    content.as_mut().ok_or(SLOT_NOT_FILLED)
}

struct ClipRecordStuff {
    task: ClipRecordTask,
    recording_equipment: RecordingEquipment,
    temporary_route: Option<TrackRoute>,
}

fn create_clip_record_stuff(
    slot_index: usize,
    containing_track: Option<&Track>,
    matrix_record_settings: &MatrixClipRecordSettings,
    column_settings: &ColumnClipRecordSettings,
    playback_track: &Track,
    column_source: &SharedColumn,
    midi_overdub_desired: bool,
) -> ClipEngineResult<ClipRecordStuff> {
    let (input, temporary_route) = {
        use RecordOrigin::*;
        match &column_settings.origin {
            TrackInput => {
                debug!("Input: track input");
                let track = resolve_recording_track(column_settings, playback_track)?;
                let track_input = track
                    .recording_input()
                    .ok_or("track doesn't have any recording input")?;
                let hw_input = translate_track_input_to_hw_input(track_input)?;
                (ClipRecordInput::HardwareInput(hw_input), None)
            }
            TrackAudioOutput => {
                debug!("Input: track audio output");
                let track = resolve_recording_track(column_settings, playback_track)?;
                let containing_track = containing_track.ok_or(
                    "can't recording track audio output if Playtime runs in monitoring FX chain",
                )?;
                let route = track.add_send_to(containing_track);
                // TODO-medium At the moment, we support stereo routes only. In order to support
                //  multi-channel routes, the user must increase the ReaLearn track channel count.
                //  And we have to:
                //  1. Create multi-channel sends (I_SRCCHAN, I_DSTCHAN)
                //  2. Make sure our ReaLearn instance has enough input pins. Roughly like this:
                // // In VST plug-in
                // let low_context = reaper_low::VstPluginContext::new(self.host.raw_callback().unwrap());
                // let context = VstPluginContext::new(&low_context);
                // let channel_count = unsafe {
                //     context.request_containing_track_channel_count(
                //         NonNull::new(self.host.raw_effect()).unwrap(),
                //     )
                // };
                // unsafe {
                //     (*self.host.raw_effect()).numInputs = channel_count;
                // }
                let channel_range = ChannelRange {
                    first_channel_index: 0,
                    channel_count: track.channel_count(),
                };
                let fx_input = VirtualClipRecordAudioInput::Specific(channel_range);
                (ClipRecordInput::FxInput(fx_input), Some(route))
            }
            FxAudioInput(range) => {
                debug!("Input: FX audio input");
                let fx_input = VirtualClipRecordAudioInput::Specific(*range);
                (ClipRecordInput::FxInput(fx_input), None)
            }
        }
    };
    let recording_equipment = input.create_recording_equipment(
        Some(playback_track.project()),
        matrix_record_settings.midi_settings.auto_quantize,
    );
    let task = ClipRecordTask {
        input,
        destination: ClipRecordDestination {
            column_source: column_source.downgrade(),
            slot_index,
            is_midi_overdub: midi_overdub_desired && recording_equipment.is_midi(),
        },
    };
    let stuff = ClipRecordStuff {
        task,
        recording_equipment,
        temporary_route,
    };
    Ok(stuff)
}

fn resolve_recording_track(
    column_settings: &ColumnClipRecordSettings,
    playback_track: &Track,
) -> ClipEngineResult<Track> {
    if let Some(track_id) = &column_settings.track {
        let track_guid = Guid::from_string_without_braces(track_id.get())?;
        let track = playback_track.project().track_by_guid(&track_guid);
        if track.is_available() {
            Ok(track)
        } else {
            Err("track not available")
        }
    } else {
        Ok(playback_track.clone())
    }
}

const SLOT_NOT_FILLED: &str = "slot not filled";
fn translate_track_input_to_hw_input(
    track_input: RecordingInput,
) -> ClipEngineResult<ClipRecordHardwareInput> {
    let hw_input = match track_input {
        RecordingInput::Mono(i) => {
            let range = ChannelRange {
                first_channel_index: i,
                channel_count: 1,
            };
            ClipRecordHardwareInput::Audio(VirtualClipRecordAudioInput::Specific(range))
        }
        RecordingInput::Stereo(i) => {
            let range = ChannelRange {
                first_channel_index: i,
                channel_count: 2,
            };
            ClipRecordHardwareInput::Audio(VirtualClipRecordAudioInput::Specific(range))
        }
        RecordingInput::Midi { device_id, channel } => {
            let input = ClipRecordHardwareMidiInput { device_id, channel };
            ClipRecordHardwareInput::Midi(VirtualClipRecordHardwareMidiInput::Specific(input))
        }
        _ => return Err(""),
    };
    Ok(hw_input)
}
