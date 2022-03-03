use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, convert_position_in_frames_to_seconds,
};
use crate::main::{ColumnSettings, MatrixSettings};
use crate::rt::supplier::RecorderEquipment;
use crate::rt::{calc_tempo_factor, determine_tempo_from_time_base, ClipPlayState, SharedPos};
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{AudioCacheBehavior, AudioTimeStretchMode, Db, VirtualResampleMode};
use reaper_high::Project;
use reaper_medium::{Bpm, Hz, PositionInSeconds};

#[derive(Clone, Debug)]
pub struct Clip {
    // Unlike Column and Matrix, we use the API clip here directly because there's almost no
    // unnecessary data inside.
    persistent_data: api::Clip,
    runtime_data: ClipRuntimeData,
}

#[derive(Clone, Debug, Default)]
struct ClipRuntimeData {
    play_state: ClipPlayState,
    pos: SharedPos,
    frame_count: usize,
    source_frame_rate: Hz,
    is_midi: bool,
}

impl Clip {
    pub fn load(api_clip: api::Clip) -> Clip {
        Clip {
            persistent_data: api_clip,
            runtime_data: Default::default(),
        }
    }

    pub fn save(&self) -> api::Clip {
        self.persistent_data.clone()
    }

    pub fn create_real_time_clip(
        &self,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> ClipEngineResult<rt::Clip> {
        let mut rt_clip = rt::Clip::ready(
            &self.persistent_data,
            permanent_project,
            recorder_equipment.clone(),
        )?;
        rt_clip.set_audio_resample_mode(
            self.effective_audio_resample_mode(matrix_settings, column_settings),
        );
        rt_clip.set_audio_time_stretch_mode(
            self.effective_audio_time_stretch_mode(matrix_settings, column_settings),
        );
        rt_clip.set_audio_cache_behavior(
            self.effective_audio_cache_behavior(matrix_settings, column_settings),
        );
        Ok(rt_clip)
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
        self.runtime_data.pos = rt_clip.shared_pos();
        self.runtime_data.source_frame_rate = rt_clip.source_frame_rate();
        self.runtime_data.is_midi = rt_clip.is_midi();
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

    pub fn play_state(&self) -> ClipPlayState {
        self.runtime_data.play_state
    }

    pub fn update_play_state(&mut self, play_state: ClipPlayState) {
        self.runtime_data.play_state = play_state;
    }

    pub fn update_frame_count(&mut self, frame_count: usize) {
        self.runtime_data.frame_count = frame_count;
    }

    pub fn proportional_pos(&self) -> ClipEngineResult<UnitValue> {
        let pos = self.runtime_data.pos.get();
        if pos < 0 {
            return Err("count-in phase");
        }
        let frame_count = self.runtime_data.frame_count;
        if frame_count == 0 {
            return Err("no frame count available");
        }
        let mod_pos = pos as usize % self.runtime_data.frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Ok(proportional)
    }

    pub fn position_in_seconds(&self, timeline: &HybridTimeline) -> PositionInSeconds {
        let pos_in_source_frames = self.mod_frame();
        let pos_in_secs = convert_position_in_frames_to_seconds(
            pos_in_source_frames,
            self.runtime_data.source_frame_rate,
        );
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        let tempo_factor = self.tempo_factor(timeline_tempo);
        adjust_pos_in_secs_anti_proportionally(pos_in_secs, tempo_factor)
    }

    fn mod_frame(&self) -> isize {
        let frame = self.runtime_data.pos.get();
        if frame < 0 {
            frame
        } else if self.runtime_data.frame_count > 0 {
            frame % self.runtime_data.frame_count as isize
        } else {
            0
        }
    }

    fn tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        if let Some(tempo) = self.tempo() {
            calc_tempo_factor(tempo, timeline_tempo)
        } else {
            1.0
        }
    }

    /// Returns `None` if time base is not "Beat".
    fn tempo(&self) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.persistent_data.time_base, self.runtime_data.is_midi)
    }
}
