use crate::domain::ui_util::{
    format_value_as_db_without_unit, parse_value_from_db, reaper_volume_unit_value,
};
use crate::domain::{ControlContext, RealearnTarget, TargetCharacter};
use helgoboss_learn::{ControlType, Target, UnitValue};
use reaper_high::{Project, Reaper, Track};
use reaper_medium::{ReaperVolumeValue, TrackAttributeKey};

#[derive(Clone, Debug, PartialEq)]
pub struct TrackPeakTarget {
    pub track: Track,
}

impl<'a> Target<'a> for TrackPeakTarget {
    type Context = Option<ControlContext<'a>>;

    fn current_value(&self, _: Self::Context) -> Option<UnitValue> {
        let reaper = Reaper::get().medium_reaper();
        let channel_count = unsafe {
            reaper.get_media_track_info_value(self.track.raw(), TrackAttributeKey::Nchan) as i32
        };
        if channel_count <= 0 {
            return None;
        }
        let mut sum = 0.0;
        for ch in 0..channel_count {
            let volume = unsafe { reaper.track_get_peak_info(self.track.raw(), ch as u32) };
            sum += volume.get();
        }
        let avg = sum / channel_count as f64;
        let vol = ReaperVolumeValue::new(avg);
        Some(reaper_volume_unit_value(vol))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}

impl RealearnTarget for TrackPeakTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_value_as_db_without_unit(value)
    }

    fn value_unit(&self) -> &'static str {
        "dB"
    }

    fn is_available(&self) -> bool {
        self.track.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }
}
