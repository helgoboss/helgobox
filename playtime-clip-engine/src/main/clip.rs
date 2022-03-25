use crate::rt::supplier::{ChainEquipment, KindSpecificRecordingOutcome, RecorderRequest};
use crate::rt::tempo_util::{calc_tempo_factor, determine_tempo_from_time_base};
use crate::rt::{
    MidiOverdubInstruction, OverridableMatrixSettings, ProcessingRelevantClipSettings,
};
use crate::source_util::{
    create_file_api_source, create_pcm_source_from_file_based_api_source,
    create_pcm_source_from_midi_chunk_based_api_source, CreateApiSourceMode,
};
use crate::{rt, source_util, ClipEngineResult};
use crossbeam_channel::Sender;
use playtime_api as api;
use playtime_api::{ClipColor, Db, Source};
use reaper_high::{OwnedSource, Project};
use reaper_medium::{Bpm, OwnedPcmSource};

#[derive(Clone, Debug)]
pub struct Clip {
    // Unlike Column and Matrix, we use the API clip here directly because there's almost no
    // unnecessary data inside.
    source: api::Source,
    processing_relevant_settings: ProcessingRelevantClipSettings,
}

impl Clip {
    pub fn load(api_clip: api::Clip) -> Self {
        Self {
            processing_relevant_settings: ProcessingRelevantClipSettings::from_api(&api_clip),
            source: api_clip.source,
        }
    }

    pub fn from_recording(
        kind_specific_outcome: KindSpecificRecordingOutcome,
        clip_settings: ProcessingRelevantClipSettings,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<Self> {
        use KindSpecificRecordingOutcome::*;
        let api_source = match kind_specific_outcome {
            Midi { mirror_source } => {
                create_api_source_from_mirror_source(mirror_source, temporary_project)?
            }
            Audio { path, .. } => create_file_api_source(temporary_project, &path),
        };
        let clip = Self {
            source: api_source,
            processing_relevant_settings: clip_settings,
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
        &mut self,
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

    pub fn create_midi_overdub_instruction(
        &self,
        permanent_project: Project,
    ) -> ClipEngineResult<MidiOverdubInstruction> {
        let instruction = match &self.source {
            Source::File(file_based_api_source) => {
                // We have a file-based MIDI source only. In the real-time clip, we need to replace
                // it with an equivalent in-project MIDI source first. Create it!
                let file_based_source = create_pcm_source_from_file_based_api_source(
                    Some(permanent_project),
                    file_based_api_source,
                )?;
                // TODO-high-wait Asked Justin how to convert into in-project MIDI source.
                let in_project_source = file_based_source;
                MidiOverdubInstruction {
                    in_project_midi_source: Some(in_project_source.clone().into_raw()),
                    mirror_source: in_project_source.into_raw(),
                }
            }
            Source::MidiChunk(s) => {
                // We have an in-project MIDI source already. Great!
                MidiOverdubInstruction {
                    in_project_midi_source: None,
                    mirror_source: create_pcm_source_from_midi_chunk_based_api_source(s.clone())?
                        .into_raw(),
                }
            }
        };
        Ok(instruction)
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

    pub fn tempo_factor(&self, timeline_tempo: Bpm, is_midi: bool) -> f64 {
        if let Some(tempo) = self.tempo(is_midi) {
            calc_tempo_factor(tempo, timeline_tempo)
        } else {
            1.0
        }
    }

    /// Returns `None` if time base is not "Beat".
    fn tempo(&self, is_midi: bool) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.processing_relevant_settings.time_base, is_midi)
    }
}

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
