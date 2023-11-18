use either::Either;
use reaper_high::{Reaper, Track};
use reaper_medium::{MediaTrack, ReaperVolumeValue, SoloMode, TrackAttributeKey};
use std::iter;

/// Returns whether the peaks should better be hidden even they are available.
///
/// This is for the case if another track is soloed, reporting the peak would be misleading then.
pub fn peaks_should_be_hidden(track: &Track) -> bool {
    let is_master = track.is_master_track();
    (is_master && track.is_muted())
        || (!is_master && track.project().any_solo() && track.solo_mode() == SoloMode::Off)
}

/// Returns the track's peaks as iterator.
///
/// This takes VU mode / channel count intricacies into account. It returns peaks even if another
/// track is soloed! See [`peaks_should_be_hidden`].
pub fn get_track_peaks(
    track: MediaTrack,
) -> impl Iterator<Item = ReaperVolumeValue> + ExactSizeIterator<Item = ReaperVolumeValue> {
    let reaper = Reaper::get().medium_reaper();
    let vu_mode =
        unsafe { reaper.get_media_track_info_value(track, TrackAttributeKey::VuMode) as i32 };
    let channel_count = if matches!(vu_mode, 2 | 8) {
        // These VU modes have multi-channel support.
        unsafe { reaper.get_media_track_info_value(track, TrackAttributeKey::Nchan) as i32 }
    } else {
        // Other VU modes always use stereo.
        2
    };
    if channel_count <= 0 {
        return Either::Left(iter::empty());
    }
    let iter =
        (0..channel_count).map(move |ch| unsafe { reaper.track_get_peak_info(track, ch as u32) });
    Either::Right(iter)
}
