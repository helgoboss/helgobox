use crate::rt::supplier::{
    ChainEquipment, KindSpecificRecordingOutcome, MidiOverdubOutcome, MidiSequence,
    ReaperClipSource, RecorderRequest, RtClipSource,
};
use crate::rt::tempo_util::{calc_tempo_factor, determine_tempo_from_time_base};
use crate::rt::{OverridableMatrixSettings, RtClipId, RtClipSettings};
use crate::source_util::{
    create_file_api_source, create_pcm_source_from_api_source, create_pcm_source_from_media_file,
    make_media_file_path_absolute, CreateApiSourceMode,
};
use crate::{rt, source_util, ClipEngineResult};
use crossbeam_channel::Sender;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    ClipColor, ClipId, ClipTimeBase, Db, MidiChunkSource, Section, SourceOrigin,
};
use reaper_high::{Project, Reaper, Track};
use reaper_medium::{Bpm, PeakFileMode};
use std::error::Error;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

/// Describes a clip.
///
/// Not loaded yet.
#[derive(Clone, PartialEq, Debug)]
pub struct Clip {
    id: ClipId,
    rt_id: RtClipId,
    name: Option<String>,
    color: api::ClipColor,
    source: api::Source,
    frozen_source: Option<api::Source>,
    active_source: SourceOrigin,
    rt_settings: RtClipSettings,
}

impl Clip {
    pub fn load(api_clip: api::Clip) -> Self {
        Self {
            rt_settings: RtClipSettings::from_api(&api_clip),
            rt_id: RtClipId::from_clip_id(&api_clip.id),
            id: api_clip.id,
            name: api_clip.name,
            color: api_clip.color,
            source: api_clip.source,
            frozen_source: api_clip.frozen_source,
            active_source: api_clip.active_source,
        }
    }

    pub fn from_recording(
        id: ClipId,
        kind_specific_outcome: KindSpecificRecordingOutcome,
        clip_settings: RtClipSettings,
        temporary_project: Option<Project>,
        recording_track: &Track,
    ) -> ClipEngineResult<Self> {
        use KindSpecificRecordingOutcome::*;
        let api_source = match kind_specific_outcome {
            Midi { midi_sequence } => create_api_source_from_recorded_midi_sequence(&midi_sequence),
            Audio { path, .. } => create_file_api_source(temporary_project, &path),
        };
        let clip = Self {
            rt_id: RtClipId::from_clip_id(&id),
            id,
            name: recording_track.name().map(|n| n.into_string()),
            color: ClipColor::PlayTrackColor,
            source: api_source,
            frozen_source: None,
            active_source: SourceOrigin::Normal,
            rt_settings: clip_settings,
        };
        Ok(clip)
    }

    pub fn duplicate(&self) -> Self {
        let new_id = ClipId::random();
        Self {
            rt_id: RtClipId::from_clip_id(&new_id),
            id: new_id,
            name: self.name.clone(),
            color: self.color.clone(),
            source: self.source.clone(),
            frozen_source: self.frozen_source.clone(),
            active_source: self.active_source,
            rt_settings: self.rt_settings,
        }
    }

    pub fn rt_settings(&self) -> &RtClipSettings {
        &self.rt_settings
    }

    pub fn set_data(&mut self, clip: Clip) {
        self.color = clip.color;
        self.name = clip.name;
        self.rt_settings = clip.rt_settings;
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Creates an API clip.
    ///
    /// If the MIDI source is given, it will create the API source by inspecting the contents of
    /// this MIDI source (instead of just cloning the API source field). With this, changes that
    /// have been made to the source via MIDI editor are correctly saved.
    pub fn save(&self) -> ClipEngineResult<api::Clip> {
        let clip = api::Clip {
            id: self.id.clone(),
            name: self.name.clone(),
            source: self.source.clone(),
            frozen_source: self.frozen_source.clone(),
            active_source: self.active_source,
            time_base: self.rt_settings.time_base,
            start_timing: self.rt_settings.start_timing,
            stop_timing: self.rt_settings.stop_timing,
            looped: self.rt_settings.looped,
            volume: self.rt_settings.volume,
            color: self.color.clone(),
            section: self.rt_settings.section,
            audio_settings: self.rt_settings.audio_settings,
            midi_settings: self.rt_settings.midi_settings,
        };
        Ok(clip)
    }

    pub fn activate_frozen_source(&mut self, frozen_source: api::Source, tempo: Option<Bpm>) {
        self.frozen_source = Some(frozen_source);
        self.active_source = SourceOrigin::Frozen;
        if let ClipTimeBase::Beat(tb) = &mut self.rt_settings.time_base {
            let tempo = tempo.expect("tempo not given although beat time base");
            tb.audio_tempo = Some(api::Bpm::new(tempo.get()).unwrap())
        }
    }

    pub fn notify_midi_overdub_finished(&mut self, outcome: MidiOverdubOutcome) {
        self.source = create_api_source_from_recorded_midi_sequence(&outcome.midi_sequence);
    }

    pub fn api_source(&self) -> &api::Source {
        &self.source
    }

    pub fn peak_file_contents(
        &self,
        permanent_project: Option<Project>,
    ) -> ClipEngineResult<impl Future<Output = ClipEngineResult<Vec<u8>>> + 'static> {
        let media_file_path = self
            .absolute_media_file_path(permanent_project)
            .ok_or("no audio clip or path not found")?;
        let future = async move {
            let peak_file = get_peak_file_path(&media_file_path);
            tracing::debug!("Trying to read peak file {:?}", &peak_file);
            match fs::read(&peak_file) {
                Ok(content) => Ok(content),
                Err(_) => {
                    // Peak file doesn't exist yet. Build it.
                    let pcm_source = create_pcm_source_from_media_file(&media_file_path, false)?;
                    // Begin building
                    if !pcm_source.as_raw().peaks_build_begin() {
                        // REAPER sometimes seems to have some peaks cached in memory or so? Then
                        // it refuses to begin peak building. Make sure all peaks are cleared and
                        // try starting another build.
                        pcm_source.as_raw().peaks_clear(true);
                        if !pcm_source.as_raw().peaks_build_begin() {
                            return Err(
                                "peaks not available and source says that building not necessary",
                            );
                        }
                    }
                    // Do the actual building
                    tracing::debug!("Building peak file");
                    while pcm_source.as_raw().peaks_build_run() {
                        base::future_util::millis(10).await;
                    }
                    // Finish building
                    pcm_source.as_raw().peaks_build_finish();
                    // We need to query the peak file path again. It's weird but it can be a
                    // different one now!
                    let peak_file = get_peak_file_path(&media_file_path);
                    fs::read(peak_file).map_err(|_| "peaks built but couldn't be found")
                }
            }
        };
        Ok(future)
    }

    fn absolute_media_file_path(&self, permanent_project: Option<Project>) -> Option<PathBuf> {
        let api::Source::File(s) = &self.source else {
            return None;
        };
        make_media_file_path_absolute(permanent_project, &s.path).ok()
    }

    pub fn create_pcm_source(
        &self,
        temporary_project: Option<Project>,
    ) -> ClipEngineResult<ReaperClipSource> {
        create_pcm_source_from_api_source(&self.source, temporary_project)
    }

    pub(crate) fn create_real_time_clip(
        &mut self,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &rt::RtColumnSettings,
    ) -> Result<rt::RtClip, Box<dyn Error>> {
        let api_source = match self.active_source {
            SourceOrigin::Normal => &self.source,
            SourceOrigin::Frozen => self
                .frozen_source
                .as_ref()
                .ok_or("no frozen source given")?,
        };
        let clip_source = if let api::Source::MidiChunk(s) = &self.source {
            let midi_sequence = MidiSequence::parse_from_reaper_midi_chunk(&s.chunk)?;
            RtClipSource::Midi(midi_sequence)
        } else {
            let pcm_source = create_pcm_source_from_api_source(api_source, permanent_project)?;
            RtClipSource::Reaper(pcm_source)
        };
        let rt_clip = rt::RtClip::ready(
            self.rt_id,
            clip_source,
            matrix_settings,
            column_settings,
            self.rt_settings,
            permanent_project,
            chain_equipment,
            recorder_request_sender,
        )?;
        Ok(rt_clip)
    }

    pub fn looped(&self) -> bool {
        self.rt_settings.looped
    }

    pub fn set_looped(&mut self, looped: bool) {
        self.rt_settings.looped = looped;
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.rt_settings.volume = volume;
    }

    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
    }

    pub fn id(&self) -> &ClipId {
        &self.id
    }

    pub fn rt_id(&self) -> RtClipId {
        self.rt_id
    }

    pub fn volume(&self) -> Db {
        self.rt_settings.volume
    }

    pub fn tempo_factor(&self, timeline_tempo: Bpm, is_midi: bool) -> f64 {
        if let Some(tempo) = self.tempo(is_midi) {
            calc_tempo_factor(tempo, timeline_tempo)
        } else {
            1.0
        }
    }

    pub fn time_base(&self) -> &ClipTimeBase {
        &self.rt_settings.time_base
    }

    pub fn section(&self) -> Section {
        self.rt_settings.section
    }

    pub fn set_section(&mut self, section: Section) {
        self.rt_settings.section = section;
    }

    /// Returns `None` if time base is not "Beat".
    fn tempo(&self, is_midi: bool) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.rt_settings.time_base, is_midi)
    }
}

pub fn create_api_source_from_recorded_midi_sequence(midi_sequence: &MidiSequence) -> api::Source {
    api::Source::MidiChunk(MidiChunkSource {
        chunk: midi_sequence.format_as_reaper_midi_chunk(),
    })
}

fn get_peak_file_path(media_file_path: &Path) -> PathBuf {
    Reaper::get().medium_reaper().get_peak_file_name_ex_2(
        media_file_path,
        2000,
        PeakFileMode::Read,
        ".reapeaks",
    )
}
