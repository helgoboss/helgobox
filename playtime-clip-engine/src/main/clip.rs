use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::{
    ChainEquipment, KindSpecificRecordingOutcome, MidiOverdubSettings, QuantizationSettings,
    RecorderRequest,
};
use crate::rt::tempo_util::{calc_tempo_factor, determine_tempo_from_time_base};
use crate::rt::{
    MidiOverdubInstruction, OverridableMatrixSettings, ProcessingRelevantClipSettings,
};
use crate::source_util::{
    create_file_api_source, create_pcm_source_from_api_source, CreateApiSourceMode,
};
use crate::{rt, source_util, ClipEngineResult};
use crossbeam_channel::Sender;
use playtime_api as api;
use playtime_api::{ClipColor, Db, MidiClipRecordMode, Source};
use reaper_high::{OwnedSource, Project, Reaper};
use reaper_medium::{Bpm, OwnedPcmSource};
use std::path::PathBuf;

/// Describes a clip.
///
/// Not loaded yet.
#[derive(Clone, Debug)]
pub struct Clip {
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
        pooled_midi_source: Option<&OwnedSource>,
    ) -> ClipEngineResult<Self> {
        use KindSpecificRecordingOutcome::*;
        let api_source = match kind_specific_outcome {
            Midi {} => {
                let pooled_midi_source =
                    pooled_midi_source.expect("MIDI source must be given for MIDI recordings");
                create_api_source_from_recorded_midi_source(&pooled_midi_source, temporary_project)?
            }
            Audio { path, .. } => create_file_api_source(temporary_project, &path),
        };
        let clip = Self {
            source: api_source,
            processing_relevant_settings: clip_settings,
        };
        Ok(clip)
    }

    /// Creates an API clip.
    ///
    /// If the MIDI source is given, it will create the API source by inspecting the contents of
    /// this MIDI source (instead of just cloning the API source field). With this, changes that
    /// have been made to the source via MIDI editor are correctly saved.
    pub fn save(
        &self,
        midi_source: Option<&OwnedSource>,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<api::Clip> {
        let clip = api::Clip {
            source: {
                if let Some(midi_source) = midi_source {
                    create_api_source_from_recorded_midi_source(midi_source, temporary_project)?
                } else {
                    self.source.clone()
                }
            },
            time_base: self.processing_relevant_settings.time_base,
            start_timing: self.processing_relevant_settings.start_timing,
            stop_timing: self.processing_relevant_settings.stop_timing,
            looped: self.processing_relevant_settings.looped,
            volume: self.processing_relevant_settings.volume,
            color: ClipColor::PlayTrackColor,
            section: self.processing_relevant_settings.section,
            audio_settings: self.processing_relevant_settings.audio_settings,
            midi_settings: self.processing_relevant_settings.midi_settings,
        };
        Ok(clip)
    }

    /// Update API source in case project needs to be saved during recording.
    ///
    /// (Before recording, we should serialize the latest MIDI editor changes to the source
    /// because during recording we won't look into the source in order to not get in-flux
    /// content or interfere with the recording process in negative ways.)
    pub fn update_api_source_before_midi_overdubbing(&mut self, source: api::Source) {
        self.source = source;
    }

    pub fn notify_midi_overdub_finished(
        &mut self,
        mirror_source: &OwnedSource,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<()> {
        let api_source =
            create_api_source_from_recorded_midi_source(mirror_source, temporary_project)?;
        self.source = api_source;
        Ok(())
    }

    pub fn api_source(&self) -> &api::Source {
        &self.source
    }

    pub fn create_pcm_source(
        &self,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<OwnedPcmSource> {
        create_pcm_source_from_api_source(&self.source, temporary_project)
    }

    /// Returns the clip and a pooled copy of the MIDI source (if MIDI).
    pub(crate) fn create_real_time_clip(
        &mut self,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &rt::ColumnSettings,
    ) -> ClipEngineResult<(rt::Clip, Option<OwnedSource>)> {
        let pcm_source = create_pcm_source_from_api_source(&self.source, permanent_project)?;
        let pooled_copy = if matches!(self.source, api::Source::MidiChunk(_)) {
            let clone =
                Reaper::get().with_pref_pool_midi_when_duplicating(true, || pcm_source.clone());
            Some(OwnedSource::new(clone))
        } else {
            None
        };
        let rt_clip = rt::Clip::ready(
            pcm_source,
            matrix_settings,
            column_settings,
            &self.processing_relevant_settings,
            permanent_project,
            chain_equipment,
            recorder_request_sender,
        )?;
        Ok((rt_clip, pooled_copy))
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

pub fn create_api_source_from_recorded_midi_source(
    midi_source: &OwnedSource,
    temporary_project: Option<Project>,
) -> ClipEngineResult<api::Source> {
    let api_source = source_util::create_api_source_from_pcm_source(
        midi_source,
        CreateApiSourceMode::AllowEmbeddedData,
        temporary_project,
    );
    api_source.map_err(|_| "failed creating API source from mirror source")
}
