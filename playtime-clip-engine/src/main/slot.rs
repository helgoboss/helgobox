use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::main::{
    Clip, ClipMatrixHandler, ClipRecordDestination, ClipRecordHardwareInput,
    ClipRecordHardwareMidiInput, ClipRecordInput, ClipRecordTask, VirtualClipRecordAudioInput,
    VirtualClipRecordHardwareMidiInput,
};
use crate::rt::supplier::{
    ChainEquipment, MaterialInfo, MidiOverdubSettings, QuantizationSettings, Recorder,
    RecorderRequest, RecordingArgs, RecordingEquipment, SupplierChain, MIDI_BASE_BPM,
};
use crate::rt::tempo_util::calc_tempo_factor;
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ClipRecordArgs, ColumnCommandSender, ColumnSetClipLoopedArgs,
    MidiOverdubInstruction, NormalRecordingOutcome, OverridableMatrixSettings,
    RecordNewClipInstruction, SharedColumn, SlotRecordInstruction, SlotRuntimeData,
};
use crate::{clip_timeline, rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ChannelRange, ColumnClipRecordSettings, Db, MatrixClipRecordSettings, MidiClipRecordMode,
    RecordOrigin,
};
use reaper_high::{Guid, OrCurrentProject, OwnedSource, Project, Reaper, Track, TrackRoute};
use reaper_medium::{
    DurationInSeconds, OwnedPcmSource, PcmSource, PositionInSeconds, RecordingInput,
    UiRefreshBehavior,
};
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
    /// Route which was created temporarily for recording.
    temporary_route: Option<TrackRoute>,
}

#[derive(Clone, Debug)]
struct Content {
    clip: Clip,
    runtime_data: SlotRuntimeData,
    /// A copy of the real-time MIDI source. Only set for in-project MIDI, not file MIDI.
    ///
    /// With this, in-project MIDI sources can be opened in the MIDI editor and editing there
    /// has immediate effects. For this to work, the source must be a pooled copy!
    ///
    /// Now that we have pooled MIDI anyway, we don't need to send a finished MIDI recording back
    /// to the main thread using the "mirror source" method (which we did before).
    pooled_midi_source: Option<OwnedSource>,
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
        self.content.is_none() && !self.state.is_pretty_much_recording()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns `None` if this slot doesn't need to be saved (because it's empty).
    pub fn save(&self, temporary_project: Option<Project>) -> Option<api::Slot> {
        let content = self.content.as_ref()?;
        let is_recording = self.state.is_pretty_much_recording()
            || self
                .get_content()
                .ok()?
                .runtime_data
                .play_state
                .is_somehow_recording();
        let pooled_midi_source = if is_recording {
            // When recording, we don't want to interfere with the pooled MIDI that's being
            // changed at the very moment. Also, we don't want to save "uncommitted" data, so
            // we save the last known "stable" MIDI contents.
            None
        } else {
            // When not recording, we inspect the pooled MIDI source.
            content.pooled_midi_source.as_ref()
        };
        let clip = content
            .clip
            .save(pooled_midi_source, temporary_project)
            .ok()?;
        let api_slot = api::Slot {
            row: self.index,
            clip: Some(clip),
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
        let project = playback_track.project();
        if self.state.is_pretty_much_recording() {
            return Err("recording already");
        }
        let (has_existing_clip, desired_midi_overdub_instruction) = match &self.content {
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
                let desired_midi_overdub_instruction = if want_midi_overdub {
                    let instruction = create_midi_overdub_instruction(
                        matrix_record_settings.midi_settings.record_mode,
                        matrix_record_settings.midi_settings.auto_quantize,
                    )?;
                    Some(instruction)
                } else {
                    None
                };
                (true, desired_midi_overdub_instruction)
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
            desired_midi_overdub_instruction,
        )?;
        let (instruction, pooled_midi_source) = match record_stuff.mode_specific {
            ModeSpecificClipRecordStuff::MidiOverdub { instruction } => {
                // We can do MIDI overdub. This is the easiest thing and needs almost no preparation.
                (SlotRecordInstruction::MidiOverdub(instruction), None)
            }
            ModeSpecificClipRecordStuff::Normal {
                recording_equipment,
                pooled_midi_source,
            } => {
                // We record completely new material.
                let args = ClipRecordArgs {
                    recording_equipment,
                    settings: *matrix_record_settings,
                };
                let instruction = if has_existing_clip {
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
                        Some(project),
                        rt_column_settings,
                        overridable_matrix_settings,
                        &args.settings,
                        args.recording_equipment,
                    );
                    let timeline = clip_timeline(Some(project), false);
                    let timeline_cursor_pos = timeline.cursor_pos();
                    let recorder =
                        Recorder::recording(recording_args, recorder_request_sender.clone());
                    let supplier_chain = SupplierChain::new(recorder, chain_equipment.clone())?;
                    let new_clip_instruction = RecordNewClipInstruction {
                        supplier_chain,
                        project: Some(project),
                        shared_pos: Default::default(),
                        timeline,
                        timeline_cursor_pos,
                        settings: *matrix_record_settings,
                    };
                    SlotRecordInstruction::NewClip(new_clip_instruction)
                };
                (instruction, pooled_midi_source)
            }
        };
        // Above code was only for checking preconditions and preparing stuff.
        // Here we do the actual state changes and distribute tasks.
        let next_state = if instruction.is_midi_overdub() {
            let content = self
                .content
                .as_mut()
                .expect("content not set although overdubbing");
            let pooled_midi_source = content
                .pooled_midi_source
                .as_ref()
                .expect("pooled MIDI source not set although overdubbing");
            content
                .clip
                .notify_midi_overdub_requested(pooled_midi_source, Some(project))?;
            SlotState::RequestedOverdubbing
        } else {
            SlotState::RequestedRecording(RequestedRecordingState { pooled_midi_source })
        };
        self.state = next_state;
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

    pub fn start_editing_clip(&self, temporary_project: Project) -> ClipEngineResult<()> {
        let content = self.get_content()?;
        let editor_track = find_or_create_editor_track(temporary_project);
        let item = editor_track.add_item().map_err(|e| e.message())?;
        debug!("Active take before: {:?}", item.active_take());
        let take = item.add_take().map_err(|e| e.message())?;
        let midi_source = if let Some(s) = content.pooled_midi_source.as_ref() {
            Reaper::get().with_pref_pool_midi_when_duplicating(true, || s.clone())
        } else {
            OwnedSource::new(content.clip.create_pcm_source(Some(temporary_project))?)
        };
        take.set_source(midi_source);
        debug!("Active take after: {:?}", item.active_take());
        item.set_position(PositionInSeconds::new(0.0), UiRefreshBehavior::NoRefresh)
            .unwrap();
        item.set_length(DurationInSeconds::new(2.0), UiRefreshBehavior::NoRefresh)
            .unwrap();
        Reaper::get().medium_reaper().update_arrange();
        Ok(())
    }

    pub fn stop_editing_clip(&self, temporary_project: Option<Project>) -> ClipEngineResult<()> {
        let content = self.get_content()?;
        // TODO-high CONTINUE
        Ok(())
    }

    pub fn is_editing_clip(&self, temporary_project: Option<Project>) -> bool {
        if let Some(content) = self.content.as_ref() {
            // TODO-high CONTINUE
            false
        } else {
            false
        }
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
            RequestedOverdubbing | RequestedRecording(_) => {
                Ok(ClipPlayState::ScheduledForRecordingStart)
            }
            Recording(s) => Ok(s.runtime_data.play_state),
        }
    }

    fn runtime_data(&self) -> ClipEngineResult<&SlotRuntimeData> {
        use SlotState::*;
        match &self.state {
            Recording(s) => Ok(&s.runtime_data),
            _ => Ok(&self.get_content()?.runtime_data),
        }
    }

    fn runtime_data_mut(&mut self) -> ClipEngineResult<&mut SlotRuntimeData> {
        use SlotState::*;
        match &mut self.state {
            Recording(s) => Ok(&mut s.runtime_data),
            _ => Ok(&mut get_content_mut(&mut self.content)?.runtime_data),
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

    pub(crate) fn fill_with(
        &mut self,
        clip: Clip,
        rt_clip: &rt::Clip,
        pooled_midi_source: Option<OwnedSource>,
    ) {
        let content = Content {
            clip,
            runtime_data: SlotRuntimeData {
                play_state: Default::default(),
                pos: rt_clip.shared_pos(),
                material_info: rt_clip.material_info().unwrap(),
            },
            pooled_midi_source,
        };
        self.content = Some(content);
    }

    pub fn notify_recording_request_acknowledged(
        &mut self,
        result: Result<Option<SlotRuntimeData>, SlotRecordInstruction>,
    ) -> ClipEngineResult<()> {
        let runtime_data = match result {
            Ok(r) => r,
            Err(_) => {
                debug!("Recording request acknowledged with negative result");
                self.remove_temporary_route();
                self.state = SlotState::Normal;
                return Ok(());
            }
        };
        use SlotState::*;
        match mem::replace(&mut self.state, Normal) {
            Normal => Err("recording was not requested"),
            RequestedOverdubbing => {
                debug!("Acknowledged overdubbing");
                Ok(())
            }
            RequestedRecording(s) => {
                debug!("Acknowledged real recording");
                let runtime_data = runtime_data.expect("no runtime data sent back");
                self.state = {
                    // This must be a real recording, not overdub.
                    let recording_state = RecordingState {
                        pooled_midi_source: s.pooled_midi_source,
                        runtime_data,
                    };
                    SlotState::Recording(recording_state)
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
            .notify_midi_overdub_finished(&OwnedSource::new(mirror_source), temporary_project)
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
                SlotState::RequestedOverdubbing => Err("requested overdubbing"),
                SlotState::RequestedRecording(_) => Err("clip recording was not yet acknowledged"),
                SlotState::Recording(mut s) => {
                    let clip = Clip::from_recording(
                        recording.kind_specific,
                        recording.clip_settings,
                        temporary_project,
                        s.pooled_midi_source.as_ref(),
                    )?;
                    s.runtime_data.material_info = recording.material_info;
                    debug!("Fill slot with clip: {:#?}", &clip);
                    let content = Content {
                        clip,
                        runtime_data: s.runtime_data,
                        pooled_midi_source: s.pooled_midi_source,
                    };
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
enum SlotState {
    /// Either empty or filled.
    ///
    /// Can be overdubbing (check play state).
    Normal,
    /// Used to prevent double invocation during overdubbing acknowledgement phase.
    RequestedOverdubbing,
    /// Used to prevent double invocation during recording acknowledgement phase.
    RequestedRecording(RequestedRecordingState),
    /// Recording (not overdubbing).
    Recording(RecordingState),
}

#[derive(Clone, Debug)]
struct RequestedRecordingState {
    pooled_midi_source: Option<OwnedSource>,
}

#[derive(Clone, Debug)]
struct RecordingState {
    /// This must be set for MIDI recordings.
    pooled_midi_source: Option<OwnedSource>,
    runtime_data: SlotRuntimeData,
}

impl SlotState {
    pub fn is_pretty_much_recording(&self) -> bool {
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
    temporary_route: Option<TrackRoute>,
    mode_specific: ModeSpecificClipRecordStuff,
}

enum ModeSpecificClipRecordStuff {
    Normal {
        recording_equipment: RecordingEquipment,
        pooled_midi_source: Option<OwnedSource>,
    },
    MidiOverdub {
        instruction: MidiOverdubInstruction,
    },
}

fn create_clip_record_stuff(
    slot_index: usize,
    containing_track: Option<&Track>,
    matrix_record_settings: &MatrixClipRecordSettings,
    column_settings: &ColumnClipRecordSettings,
    playback_track: &Track,
    column_source: &SharedColumn,
    desired_midi_overdub_instruction: Option<MidiOverdubInstruction>,
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
    let final_midi_overdub_instruction = if recording_equipment.is_midi() {
        desired_midi_overdub_instruction
    } else {
        // Want overdub but we have a audio input, so don't use overdub mode after all.
        None
    };
    let task = ClipRecordTask {
        input,
        destination: ClipRecordDestination {
            column_source: column_source.downgrade(),
            slot_index,
            is_midi_overdub: final_midi_overdub_instruction.is_some(),
        },
    };
    let stuff = ClipRecordStuff {
        task,
        temporary_route,
        mode_specific: if let Some(instruction) = final_midi_overdub_instruction {
            ModeSpecificClipRecordStuff::MidiOverdub { instruction }
        } else {
            let pooled_midi_source = match &recording_equipment {
                RecordingEquipment::Midi(e) => {
                    Some(OwnedSource::new(e.create_pooled_copy_of_midi_source()))
                }
                RecordingEquipment::Audio(_) => None,
            };
            ModeSpecificClipRecordStuff::Normal {
                recording_equipment,
                pooled_midi_source,
            }
        },
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
pub fn create_midi_overdub_instruction(
    mode: MidiClipRecordMode,
    auto_quantize: bool,
) -> ClipEngineResult<MidiOverdubInstruction> {
    let quantization_settings = if auto_quantize {
        // TODO-high Use project quantization settings
        Some(QuantizationSettings {})
    } else {
        None
    };
    // let instruction = match &self.source {
    //     Source::File(file_based_api_source) => {
    //         // We have a file-based MIDI source only. In the real-time clip, we need to replace
    //         // it with an equivalent in-project MIDI source first. Create it!
    //         let file_based_source = create_pcm_source_from_file_based_api_source(
    //             Some(permanent_project),
    //             file_based_api_source,
    //         )?;
    //         // TODO-high-wait Use Justin's trick to import as in-project MIDI.
    //         let in_project_source = file_based_source;
    //         MidiOverdubInstruction {
    //             in_project_midi_source: Some(in_project_source.clone().into_raw()),
    //             settings: MidiOverdubSettings {
    //                 mirror_source: in_project_source.into_raw(),
    //                 mode,
    //                 quantization_settings,
    //             },
    //         }
    //     }
    //     Source::MidiChunk(s) => {
    //         // We have an in-project MIDI source already. Great!
    //         MidiOverdubInstruction {
    //             in_project_midi_source: None,
    //             settings: MidiOverdubSettings {
    //                 mirror_source: {
    //                     create_pcm_source_from_midi_chunk_based_api_source(s.clone())?
    //                         .into_raw()
    //                 },
    //                 mode,
    //                 quantization_settings,
    //             },
    //         }
    //     }
    // };
    // TODO-high Deal with file-based source.
    let instruction = MidiOverdubInstruction {
        in_project_midi_source: None,
        settings: MidiOverdubSettings {
            mode,
            quantization_settings,
        },
    };
    Ok(instruction)
}

fn find_or_create_editor_track(project: Project) -> Track {
    project
        .tracks()
        .find(|t| {
            if let Some(name) = t.name() {
                name.to_str() == EDITOR_TRACK_NAME
            } else {
                false
            }
        })
        .unwrap_or_else(|| {
            let track = project.add_track();
            track.set_name(EDITOR_TRACK_NAME);
            track
        })
}

const EDITOR_TRACK_NAME: &str = "playtime-editor-track";
