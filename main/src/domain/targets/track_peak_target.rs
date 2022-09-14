use crate::domain::ui_util::{
    format_value_as_db_without_unit, parse_value_from_db, volume_unit_value,
};
use crate::domain::{
    get_effective_tracks, Compartment, ControlContext, ExtendedProcessorContext,
    FeedbackResolution, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, TrackDescriptor, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, NumericValue, Target, UnitValue};
use reaper_high::{Project, Reaper, Track, Volume};
use reaper_medium::{ReaperVolumeValue, TrackAttributeKey};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackPeakTarget {
    pub track_descriptor: TrackDescriptor,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackPeakTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| ReaperTarget::TrackPeak(TrackPeakTarget { track }))
                .collect(),
        )
    }

    fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        Some(FeedbackResolution::High)
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        Some(&self.track_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackPeakTarget {
    pub track: Track,
}

impl<'a> Target<'a> for TrackPeakTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let vol = self.peak()?;
        let val = volume_unit_value(vol);
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl TrackPeakTarget {
    fn peak(&self) -> Option<Volume> {
        let reaper = Reaper::get().medium_reaper();
        let vu_mode = unsafe {
            reaper.get_media_track_info_value(self.track.raw(), TrackAttributeKey::VuMode) as i32
        };
        let channel_count = if matches!(vu_mode, 2 | 8) {
            unsafe {
                reaper.get_media_track_info_value(self.track.raw(), TrackAttributeKey::Nchan) as i32
            }
        } else {
            2
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
        Some(Volume::from_reaper_value(vol))
    }
}

impl RealearnTarget for TrackPeakTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str, _: ControlContext) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_db_without_unit(value)
    }

    fn value_unit(&self, _: ControlContext) -> &'static str {
        "dB"
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.track.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(self.peak()?.to_string().into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        Some(NumericValue::Decimal(self.peak()?.db().get()))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackPeak)
    }
}

pub const TRACK_PEAK_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Track: Peak",
    short_name: "Track peak",
    hint: "Feedback only, no control",
    supports_track: true,
    supports_control: false,
    ..DEFAULT_TARGET
};
