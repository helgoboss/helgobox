use crate::main::{load_source, ClipData};
use crate::rt::supplier::RecorderEquipment;
use crate::rt::{ClipInfo, ClipPlayState, SharedPos};
use crate::{rt, ClipEngineResult};
use helgoboss_learn::UnitValue;
use playtime_api as api;
use reaper_high::Project;
use reaper_medium::{Bpm, Db, PositionInSeconds};

#[derive(Clone, Debug)]
pub struct Clip {
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

    pub fn create_real_time_clip(
        &self,
        project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
    ) -> ClipEngineResult<rt::Clip> {
        let source = load_source(&self.persistent_data.source, project)?;
        Ok(rt::Clip::from_source(
            source,
            project,
            recorder_equipment.clone(),
        ))
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
