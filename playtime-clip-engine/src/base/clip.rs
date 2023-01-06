use crate::rt::supplier::{
    ChainEquipment, ClipSource, KindSpecificRecordingOutcome, RecorderRequest,
};
use crate::rt::tempo_util::{calc_tempo_factor, determine_tempo_from_time_base};
use crate::rt::{ClipChangeEvent, OverridableMatrixSettings, ProcessingRelevantClipSettings};
use crate::source_util::{
    create_file_api_source, create_pcm_source_from_api_source, CreateApiSourceMode,
};
use crate::{rt, source_util, ClipEngineResult};
use crossbeam_channel::Sender;
use playtime_api::persistence as api;
use playtime_api::persistence::{ClipColor, ClipTimeBase, Db, Section, SourceOrigin};
use reaper_high::{Project, Reaper, Track};
use reaper_medium::Bpm;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use ulid::Ulid;

/// Describes a clip.
///
/// Not loaded yet.
#[derive(Clone, Debug)]
pub struct Clip {
    id: ClipId,
    name: Option<String>,
    color: api::ClipColor,
    source: api::Source,
    frozen_source: Option<api::Source>,
    active_source: SourceOrigin,
    processing_relevant_settings: ProcessingRelevantClipSettings,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct ClipId(Ulid);

impl ClipId {
    pub fn random() -> Self {
        Self(Ulid::new())
    }
}

impl Display for ClipId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ClipId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ulid = Ulid::from_str(s).map_err(|_| "couldn't decode string as ULID")?;
        Ok(ClipId(ulid))
    }
}

impl Clip {
    pub fn load(api_clip: api::Clip) -> Self {
        Self {
            processing_relevant_settings: ProcessingRelevantClipSettings::from_api(&api_clip),
            id: api_clip
                .id
                .and_then(|s| ClipId::from_str(&s).ok())
                .unwrap_or_else(ClipId::random),
            name: api_clip.name,
            color: api_clip.color,
            source: api_clip.source,
            frozen_source: api_clip.frozen_source,
            active_source: api_clip.active_source,
        }
    }

    pub fn from_recording(
        kind_specific_outcome: KindSpecificRecordingOutcome,
        clip_settings: ProcessingRelevantClipSettings,
        temporary_project: Option<Project>,
        pooled_midi_source: Option<&ClipSource>,
        recording_track: &Track,
    ) -> ClipEngineResult<Self> {
        use KindSpecificRecordingOutcome::*;
        let api_source = match kind_specific_outcome {
            Midi {} => {
                let pooled_midi_source =
                    pooled_midi_source.expect("MIDI source must be given for MIDI recordings");
                create_api_source_from_recorded_midi_source(pooled_midi_source, temporary_project)?
            }
            Audio { path, .. } => create_file_api_source(temporary_project, &path),
        };
        let clip = Self {
            id: ClipId::random(),
            name: recording_track.name().map(|n| n.into_string()),
            color: ClipColor::PlayTrackColor,
            source: api_source,
            frozen_source: None,
            active_source: SourceOrigin::Normal,
            processing_relevant_settings: clip_settings,
        };
        Ok(clip)
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Creates an API clip.
    pub fn save(&self, temporary_project: Option<Project>) -> ClipEngineResult<api::Clip> {
        self.save_flexible(None, temporary_project)
    }

    /// Creates an API clip.
    ///
    /// If the MIDI source is given, it will create the API source by inspecting the contents of
    /// this MIDI source (instead of just cloning the API source field). With this, changes that
    /// have been made to the source via MIDI editor are correctly saved.
    pub fn save_flexible(
        &self,
        midi_source: Option<&ClipSource>,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<api::Clip> {
        let clip = api::Clip {
            id: Some(self.id.to_string()),
            name: self.name.clone(),
            source: {
                if let Some(midi_source) = midi_source {
                    create_api_source_from_recorded_midi_source(midi_source, temporary_project)?
                } else {
                    self.source.clone()
                }
            },
            frozen_source: self.frozen_source.clone(),
            active_source: self.active_source,
            time_base: self.processing_relevant_settings.time_base,
            start_timing: self.processing_relevant_settings.start_timing,
            stop_timing: self.processing_relevant_settings.stop_timing,
            looped: self.processing_relevant_settings.looped,
            volume: self.processing_relevant_settings.volume,
            color: self.color.clone(),
            section: self.processing_relevant_settings.section,
            audio_settings: self.processing_relevant_settings.audio_settings,
            midi_settings: self.processing_relevant_settings.midi_settings,
        };
        Ok(clip)
    }

    pub fn activate_frozen_source(&mut self, frozen_source: api::Source, tempo: Option<Bpm>) {
        self.frozen_source = Some(frozen_source);
        self.active_source = SourceOrigin::Frozen;
        if let ClipTimeBase::Beat(tb) = &mut self.processing_relevant_settings.time_base {
            let tempo = tempo.expect("tempo not given although beat time base");
            tb.audio_tempo = Some(api::Bpm::new(tempo.get()).unwrap())
        }
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
        mirror_source: &ClipSource,
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
    ) -> ClipEngineResult<ClipSource> {
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
    ) -> ClipEngineResult<(rt::Clip, Option<ClipSource>)> {
        let api_source = match self.active_source {
            SourceOrigin::Normal => &self.source,
            SourceOrigin::Frozen => self
                .frozen_source
                .as_ref()
                .ok_or("no frozen source given")?,
        };
        let pcm_source = create_pcm_source_from_api_source(api_source, permanent_project)?;
        let pooled_copy = if matches!(self.source, api::Source::MidiChunk(_)) {
            let clone =
                Reaper::get().with_pref_pool_midi_when_duplicating(true, || pcm_source.clone());
            Some(clone)
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

    pub fn set_looped(&mut self, looped: bool) {
        self.processing_relevant_settings.looped = looped;
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.processing_relevant_settings.volume = volume;
    }

    pub fn set_name(&mut self, name: Option<String>) -> ClipChangeEvent {
        self.name = name;
        ClipChangeEvent::Everything
    }

    pub fn id(&self) -> ClipId {
        self.id
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

    pub fn time_base(&self) -> &ClipTimeBase {
        &self.processing_relevant_settings.time_base
    }

    pub fn section(&self) -> Section {
        self.processing_relevant_settings.section
    }

    pub fn set_section(&mut self, section: Section) {
        self.processing_relevant_settings.section = section;
    }

    /// Returns `None` if time base is not "Beat".
    fn tempo(&self, is_midi: bool) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.processing_relevant_settings.time_base, is_midi)
    }
}

pub fn create_api_source_from_recorded_midi_source(
    midi_source: &ClipSource,
    temporary_project: Option<Project>,
) -> ClipEngineResult<api::Source> {
    let api_source = source_util::create_api_source_from_pcm_source(
        midi_source.reaper_source(),
        CreateApiSourceMode::AllowEmbeddedData,
        temporary_project,
    );
    api_source.map_err(|_| "failed creating API source from mirror source")
}
