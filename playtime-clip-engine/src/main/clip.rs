use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::main::{
    create_file_api_source, create_pcm_source_from_api_source, source_util, CreateApiSourceMode,
};
use crate::rt::supplier::{
    ChainEquipment, KindSpecificRecordingOutcome, MaterialInfo, RecorderRequest,
};
use crate::rt::tempo_util::determine_tempo_from_time_base;
use crate::rt::{
    calc_tempo_factor, ClipPlayState, CommittedRecording, OverridableMatrixSettings,
    ProcessingRelevantClipSettings, SharedPos,
};
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{ClipColor, Db};
use reaper_high::{OwnedSource, Project};
use reaper_medium::{Bpm, OwnedPcmSource, PositionInSeconds};

#[derive(Clone, Debug)]
pub struct Clip {
    // Unlike Column and Matrix, we use the API clip here directly because there's almost no
    // unnecessary data inside.
    source: api::Source,
    processing_relevant_settings: ProcessingRelevantClipSettings,
    runtime_data: Option<ClipRuntimeData>,
    /// `true` for the short moment while recording was requested (using the chain of an existing
    /// clip) but has not yet been acknowledged from a real-time thread.
    recording_requested: bool,
}

#[derive(Clone, Debug)]
struct ClipRuntimeData {
    play_state: ClipPlayState,
    pos: SharedPos,
    material_info: MaterialInfo,
}

impl ClipRuntimeData {
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
}

impl Clip {
    pub fn load(api_clip: api::Clip) -> Self {
        Self {
            processing_relevant_settings: ProcessingRelevantClipSettings::from_api(&api_clip),
            source: api_clip.source,
            runtime_data: None,
            recording_requested: false,
        }
    }

    pub fn from_recording(
        recording: CommittedRecording,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<Self> {
        use KindSpecificRecordingOutcome::*;
        let api_source = match recording.kind_specific {
            Midi { mirror_source } => {
                create_api_source_from_mirror_source(mirror_source, temporary_project)?
            }
            Audio { path } => create_file_api_source(temporary_project, &path),
        };
        let clip = Self {
            source: api_source,
            runtime_data: None,
            recording_requested: false,
            processing_relevant_settings: recording.clip_settings,
        };
        Ok(clip)
    }

    pub fn save(&self) -> api::Clip {
        api::Clip {
            source: self.source.clone(),
            time_base: self.processing_relevant_settings.time_base,
            start_timing: self.processing_relevant_settings.start_timing,
            stop_timing: self.processing_relevant_settings.stop_timing,
            looped: self.processing_relevant_settings.looped,
            volume: self.processing_relevant_settings.volume,
            color: ClipColor::PlayTrackColor,
            section: self.processing_relevant_settings.section,
            audio_settings: self.processing_relevant_settings.audio_settings,
            midi_settings: self.processing_relevant_settings.midi_settings,
        }
    }

    pub fn notify_recording_requested(&mut self) -> ClipEngineResult<()> {
        if self.recording_requested {
            return Err("recording has already been requested");
        }
        if self.play_state() == Ok(ClipPlayState::Recording) {
            return Err("already recording");
        }
        self.recording_requested = true;
        Ok(())
    }

    pub fn notify_recording_request_acknowledged(&mut self) {
        self.recording_requested = false;
    }

    pub fn notify_midi_overdub_finished(
        &mut self,
        mirror_source: OwnedPcmSource,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<()> {
        let api_source = create_api_source_from_mirror_source(mirror_source, temporary_project)?;
        self.source = api_source;
        Ok(())
    }

    pub fn create_real_time_clip(
        &self,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &rt::ColumnSettings,
    ) -> ClipEngineResult<rt::Clip> {
        rt::Clip::ready(
            &self.source,
            matrix_settings,
            column_settings,
            &self.processing_relevant_settings,
            permanent_project,
            chain_equipment,
            recorder_request_sender,
        )
    }

    pub fn create_mirror_source_for_midi_overdub(
        &self,
        permanent_project: Option<Project>,
    ) -> ClipEngineResult<OwnedPcmSource> {
        create_pcm_source_from_api_source(&self.source, permanent_project)
    }

    fn runtime_data(&self) -> ClipEngineResult<&ClipRuntimeData> {
        get_runtime_data(&self.runtime_data)
    }

    fn runtime_data_mut(&mut self) -> ClipEngineResult<&mut ClipRuntimeData> {
        get_runtime_data_mut(&mut self.runtime_data)
    }

    /// Connects the given real-time clip to the main clip.
    pub fn connect_to(&mut self, rt_clip: &rt::Clip) {
        let runtime_data = ClipRuntimeData {
            play_state: Default::default(),
            pos: rt_clip.shared_pos(),
            material_info: rt_clip.material_info().unwrap(),
        };
        self.runtime_data = Some(runtime_data);
    }

    pub fn material_info(&self) -> Option<&MaterialInfo> {
        let runtime_data = self.runtime_data.as_ref()?;
        Some(&runtime_data.material_info)
    }

    pub fn looped(&self) -> bool {
        self.processing_relevant_settings.looped
    }

    pub fn toggle_looped(&mut self) -> bool {
        let looped_new = !self.processing_relevant_settings.looped;
        self.processing_relevant_settings.looped = looped_new;
        looped_new
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.processing_relevant_settings.volume = volume;
    }

    pub fn volume(&self) -> Db {
        self.processing_relevant_settings.volume
    }

    pub fn recording_requested(&self) -> bool {
        self.recording_requested
    }

    pub fn play_state(&self) -> ClipEngineResult<ClipPlayState> {
        if self.recording_requested {
            // Report optimistically that we are recording already.
            return Ok(ClipPlayState::Recording);
        }
        Ok(self.runtime_data()?.play_state)
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
        let tempo_factor = self.tempo_factor(timeline_tempo);
        Ok(adjust_pos_in_secs_anti_proportionally(
            pos_in_secs,
            tempo_factor,
        ))
    }

    fn tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        if let Some(tempo) = self.tempo() {
            calc_tempo_factor(tempo, timeline_tempo)
        } else {
            1.0
        }
    }

    /// Returns `None` if time base is not "Beat" or if no runtime data is available.
    fn tempo(&self) -> Option<Bpm> {
        determine_tempo_from_time_base(
            &self.processing_relevant_settings.time_base,
            self.runtime_data().ok()?.material_info.is_midi(),
        )
    }
}

fn get_runtime_data(runtime_data: &Option<ClipRuntimeData>) -> ClipEngineResult<&ClipRuntimeData> {
    runtime_data.as_ref().ok_or(CLIP_RUNTIME_DATA_UNAVAILABLE)
}

fn get_runtime_data_mut(
    runtime_data: &mut Option<ClipRuntimeData>,
) -> ClipEngineResult<&mut ClipRuntimeData> {
    runtime_data.as_mut().ok_or(CLIP_RUNTIME_DATA_UNAVAILABLE)
}

const CLIP_RUNTIME_DATA_UNAVAILABLE: &str = "clip runtime data unavailable";

fn create_api_source_from_mirror_source(
    mirror_source: OwnedPcmSource,
    temporary_project: Option<Project>,
) -> ClipEngineResult<api::Source> {
    let api_source = source_util::create_api_source_from_pcm_source(
        &OwnedSource::new(mirror_source),
        CreateApiSourceMode::AllowEmbeddedData,
        temporary_project,
    );
    api_source.map_err(|_| "failed creating API source from mirror source")
}
