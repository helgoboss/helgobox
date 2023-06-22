use crate::base::{Clip, OnlineData};
use crate::{clip_timeline, ClipEngineResult, QuantizedPosition, Timeline};
use playtime_api::persistence::ClipTimeBase;
use reaper_high::{Item, OwnedSource, Project, ReaperSource, Take, Track};
use reaper_medium::{Bpm, DurationInSeconds, PositionInSeconds, UiRefreshBehavior};

pub fn manifest_clip_on_track(
    clip: &Clip,
    online_data: &OnlineData,
    track: &Track,
) -> ClipEngineResult<ClipOnTrackManifestation> {
    let temporary_project = track.project();
    // TODO-medium Make sure time-based MIDI clips are treated correctly (pretty rare).
    let item = track.add_item().map_err(|e| e.message())?;
    let timeline = clip_timeline(Some(track.project()), true);
    // We must put the item exactly how we would play it so the grid is correct (important
    // for MIDI editor).
    let item_length = online_data.effective_length_in_seconds(clip, &timeline)?;
    let section_start_pos = DurationInSeconds::new(clip.section().start_pos.get());
    let (item_pos, take_offset, tempo) = match clip.time_base() {
        // Place section start exactly on start of project.
        ClipTimeBase::Time => (
            PositionInSeconds::ZERO,
            PositionInSeconds::from(section_start_pos),
            None,
        ),
        ClipTimeBase::Beat(t) => {
            // Place downbeat exactly on start of 2nd bar of project.
            let second_bar_pos = timeline.pos_of_quantized_pos(QuantizedPosition::bar(1));
            let bpm = timeline.tempo_at(second_bar_pos);
            let bps = bpm.get() / 60.0;
            let downbeat_pos = t.downbeat.get() / bps;
            (
                second_bar_pos - downbeat_pos,
                PositionInSeconds::from(section_start_pos),
                Some(bpm),
            )
        }
    };
    // TODO-high Implement "Open in REAPER MIDI editor" again by creating a PCM source ad-hoc
    // let source = if let Some(s) = online_data.pooled_midi_source.as_ref() {
    //     Reaper::get().with_pref_pool_midi_when_duplicating(true, || s.clone())
    // } else
    let source = clip.create_pcm_source(Some(temporary_project))?;
    if online_data.runtime_data.material_info.is_midi() {
        // Because we set a constant preview tempo for our MIDI sources (which is
        // important for our internal processing), IGNTEMPO is set to 1, which means the source
        // is considered as time-based by REAPER. That makes it appear incorrect in the MIDI
        // editor because in reality they are beat-based. The following sets IGNTEMPO to 0
        // for recent REAPER versions. Hopefully this is then only valid for this particular
        // pooled copy.
        // TODO-low This problem might disappear though as soon as we can use
        //  "Source beats" MIDI editor time base (which we can't use at the moment because we rely
        //  on sections).
        let _ = source.reaper_source().ext_set_preview_tempo(None);
    }
    let take = item.add_take().map_err(|e| e.message())?;
    let source = OwnedSource::new(source.into_reaper_source());
    take.set_source(source);
    take.set_start_offset(take_offset).unwrap();
    item.set_position(item_pos, UiRefreshBehavior::NoRefresh)
        .unwrap();
    item.set_length(item_length, UiRefreshBehavior::NoRefresh)
        .unwrap();
    let manifestation = ClipOnTrackManifestation {
        track: track.clone(),
        item,
        take,
        tempo,
    };
    Ok(manifestation)
}

#[derive(Clone, Debug)]
pub struct ClipOnTrackManifestation {
    pub track: Track,
    pub item: Item,
    pub take: Take,
    /// Always set if beat-based.
    pub tempo: Option<Bpm>,
}
