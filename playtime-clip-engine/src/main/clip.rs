use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::main::{
    create_pcm_source_from_api_source, source_util, ColumnSettings, CreateApiSourceMode,
    MatrixSettings,
};
use crate::rt::supplier::{ChainPreBufferRequest, MaterialInfo, RecorderEquipment};
use crate::rt::{calc_tempo_factor, determine_tempo_from_time_base, ClipPlayState, SharedPos};
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{AudioCacheBehavior, AudioTimeStretchMode, Db, VirtualResampleMode};
use reaper_high::{OwnedSource, Project};
use reaper_medium::{Bpm, OwnedPcmSource, PositionInSeconds};

#[derive(Clone, Debug)]
pub struct Clip {
    // Unlike Column and Matrix, we use the API clip here directly because there's almost no
    // unnecessary data inside.
    persistent_data: api::Clip,
    runtime_data: Option<ClipRuntimeData>,
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
    pub fn load(api_clip: api::Clip) -> Clip {
        Clip {
            persistent_data: api_clip,
            runtime_data: None,
            recording_requested: false,
        }
    }

    pub fn save(&self) -> api::Clip {
        self.persistent_data.clone()
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
        let api_source = source_util::create_api_source_from_pcm_source(
            &OwnedSource::new(mirror_source),
            CreateApiSourceMode::AllowEmbeddedData,
            temporary_project,
        )
        .map_err(|_| "failed creating API source from mirror source")?;
        self.persistent_data.source = api_source;
        Ok(())
    }

    pub fn create_real_time_clip(
        &self,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
        pre_buffer_request_sender: &Sender<ChainPreBufferRequest>,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> ClipEngineResult<rt::Clip> {
        let mut rt_clip = rt::Clip::ready(
            &self.persistent_data,
            permanent_project,
            recorder_equipment,
            pre_buffer_request_sender,
        )?;
        self.configure_real_time_clip(matrix_settings, column_settings, &mut rt_clip);
        Ok(rt_clip)
    }

    pub fn create_mirror_source_for_midi_overdub(
        &self,
        permanent_project: Option<Project>,
    ) -> ClipEngineResult<OwnedPcmSource> {
        create_pcm_source_from_api_source(&self.persistent_data.source, permanent_project)
    }

    fn configure_real_time_clip(
        &self,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
        rt_clip: &mut rt::Clip,
    ) {
        rt_clip.set_audio_resample_mode(
            self.effective_audio_resample_mode(matrix_settings, column_settings),
        );
        rt_clip.set_audio_time_stretch_mode(
            self.effective_audio_time_stretch_mode(matrix_settings, column_settings),
        );
        rt_clip.set_audio_cache_behavior(
            self.effective_audio_cache_behavior(matrix_settings, column_settings),
        );
    }

    fn effective_audio_resample_mode(
        &self,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> VirtualResampleMode {
        self.persistent_data
            .audio_settings
            .resample_mode
            .clone()
            .or_else(|| column_settings.audio_resample_mode.clone())
            .unwrap_or_else(|| matrix_settings.audio_resample_mode.clone())
    }

    fn effective_audio_time_stretch_mode(
        &self,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> AudioTimeStretchMode {
        self.persistent_data
            .audio_settings
            .time_stretch_mode
            .clone()
            .or_else(|| column_settings.audio_time_stretch_mode.clone())
            .unwrap_or_else(|| matrix_settings.audio_time_stretch_mode.clone())
    }

    fn runtime_data(&self) -> ClipEngineResult<&ClipRuntimeData> {
        get_runtime_data(&self.runtime_data)
    }

    fn runtime_data_mut(&mut self) -> ClipEngineResult<&mut ClipRuntimeData> {
        get_runtime_data_mut(&mut self.runtime_data)
    }

    fn effective_audio_cache_behavior(
        &self,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> AudioCacheBehavior {
        self.persistent_data
            .audio_settings
            .cache_behavior
            .clone()
            .or_else(|| column_settings.audio_cache_behavior.clone())
            .unwrap_or_else(|| matrix_settings.audio_cache_behavior.clone())
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

    pub fn data(&self) -> &api::Clip {
        &self.persistent_data
    }

    pub fn data_mut(&mut self) -> &mut api::Clip {
        &mut self.persistent_data
    }

    pub fn toggle_looped(&mut self) -> bool {
        let looped_new = !self.persistent_data.looped;
        self.persistent_data.looped = looped_new;
        looped_new
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.persistent_data.volume = volume;
    }

    pub fn volume(&self) -> Db {
        self.persistent_data.volume
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
            &self.persistent_data.time_base,
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
