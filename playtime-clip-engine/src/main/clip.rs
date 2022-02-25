use crate::main::{ColumnSettings, MatrixSettings};
use crate::rt::supplier::RecorderEquipment;
use crate::rt::{ClipInfo, ClipPlayState, SharedPos};
use crate::{rt, ClipEngineResult};
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{AudioTimeStretchMode, TempoRange, VirtualResampleMode};
use reaper_high::Project;
use reaper_medium::{Bpm, PositionInSeconds};

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
        common_tempo_range: TempoRange,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> ClipEngineResult<rt::Clip> {
        let mut rt_clip = rt::Clip::ready(
            &self.persistent_data,
            permanent_project,
            recorder_equipment.clone(),
            common_tempo_range,
        )?;
        rt_clip.set_audio_resample_mode(
            self.effective_resample_mode(matrix_settings, column_settings),
        );
        rt_clip.set_audio_time_stretch_mode(
            self.effective_time_stretch_mode(matrix_settings, column_settings),
        );
        Ok(rt_clip)
    }

    fn effective_resample_mode(
        &self,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> VirtualResampleMode {
        self.persistent_data
            .audio_settings
            .resample_mode
            .clone()
            .or_else(|| column_settings.resample_mode.clone())
            .unwrap_or_else(|| matrix_settings.resample_mode.clone())
    }

    fn effective_time_stretch_mode(
        &self,
        matrix_settings: &MatrixSettings,
        column_settings: &ColumnSettings,
    ) -> AudioTimeStretchMode {
        self.persistent_data
            .audio_settings
            .time_stretch_mode
            .clone()
            .or_else(|| column_settings.time_stretch_mode.clone())
            .unwrap_or_else(|| matrix_settings.time_stretch_mode.clone())
    }

    /// Connects the given real-time clip to the main clip.
    ///
    /// At the moment this just means that they share a common atomic position.
    pub fn connect_to(&mut self, rt_clip: &rt::Clip) {
        self.runtime_data.pos = rt_clip.shared_pos();
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

    pub fn play_state(&self) -> ClipPlayState {
        self.runtime_data.play_state
    }

    pub fn update_play_state(&mut self, play_state: ClipPlayState) {
        self.runtime_data.play_state = play_state;
    }

    pub fn update_frame_count(&mut self, frame_count: usize) {
        self.runtime_data.frame_count = frame_count;
    }

    pub fn proportional_pos(&self) -> Option<UnitValue> {
        let pos = self.runtime_data.pos.get();
        if pos < 0 {
            return None;
        }
        let frame_count = self.runtime_data.frame_count;
        if frame_count == 0 {
            return None;
        }
        let mod_pos = pos as usize % self.runtime_data.frame_count;
        let proportional = UnitValue::new_clamped(mod_pos as f64 / frame_count as f64);
        Some(proportional)
    }

    pub fn position_in_seconds(&self, timeline_tempo: Bpm) -> Option<PositionInSeconds> {
        // TODO-high At the moment we don't use this anyway. But we should implement it as soon
        //  as we do. Relies on having the current section length, source frame rate, source tempo.
        todo!()
    }

    pub fn info(&self) -> ClipInfo {
        // TODO-high This should be implemented as soon as we hold more derived info here.
        //  - type (MIDI, WAVE, ...)
        //  - original length
        todo!()
    }
}
