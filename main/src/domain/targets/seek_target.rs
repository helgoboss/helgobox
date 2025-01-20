use crate::domain::{
    with_seek_behavior, AdditionalFeedbackEvent, CompartmentKind, CompoundChangeEvent,
    ControlContext, ExtendedProcessorContext, FeedbackResolution, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, SeekOptions,
    TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, NumericValue, PropValue, Target, UnitValue,
};
use helgobox_api::persistence::SeekBehavior;
use reaper_high::{Project, Reaper};
use reaper_medium::{
    GetLoopTimeRange2Result, PositionInSeconds, SetEditCurPosOptions, TimeMode, TimeModeOverride,
};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedSeekTarget {
    pub options: SeekOptions,
    pub behavior: SeekBehavior,
}

impl UnresolvedReaperTargetDef for UnresolvedSeekTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context().project_or_current_project();
        Ok(vec![ReaperTarget::Seek(SeekTarget {
            project,
            options: self.options,
            behavior: self.behavior,
        })])
    }

    fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        Some(self.options.feedback_resolution)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeekTarget {
    pub project: Project,
    pub options: SeekOptions,
    pub behavior: SeekBehavior,
}

impl RealearnTarget for SeekTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // TODO-low "Seek" could support rounding/discrete (beats, measures, seconds, ...)
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_unit_value()?;
        let info = get_seek_info(self.project, self.options, false);
        let desired_pos_within_range = value.get() * info.length();
        let desired_pos = info.start_pos + desired_pos_within_range;
        with_seek_behavior(self.behavior, || {
            self.project.set_edit_cursor_position(
                desired_pos,
                SetEditCurPosOptions {
                    move_view: self.options.move_view,
                    seek_play: self.options.seek_play,
                },
            );
        });
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::BeatChanged(e))
                if e.project == self.project =>
            {
                let v = current_value_of_seek(self.project, self.options, e.new_value);
                (true, Some(AbsoluteValue::Continuous(v)))
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(format!("{:.3} s", self.corrected_display_pos().get()).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let seconds = self.corrected_display_pos();
        Some(NumericValue::Decimal(seconds.get()))
    }

    fn numeric_value_unit(&self, _: ControlContext) -> &'static str {
        "s"
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Seek)
    }

    fn prop_value(&self, key: &str, _: ControlContext) -> Option<PropValue> {
        let mut iter = key.split('.');
        match (iter.next(), iter.next(), iter.next()) {
            (Some("position"), Some(pos_type), suffix) => {
                use TimeMode::*;
                use TimeModeOverride::*;
                let mode = match pos_type {
                    "project_default" => match self.project.transport_time_mode() {
                        ProjectDefault => self.project.ruler_time_mode(),
                        Mode(m) => m,
                    },
                    "time" => Time,
                    "measures_beats_time" => MeasuresBeatsTime,
                    "measures_beats" => MeasuresBeats,
                    "measures_beats_minimal" => MeasuresBeatsMinimal,
                    "seconds" => Seconds,
                    "samples" => Samples,
                    "hmsf" => HoursMinutesSecondsFrames,
                    "absolute_frames" => AbsoluteFrames,
                    _ => return None,
                };
                let reaper = Reaper::get().medium_reaper();
                match suffix {
                    // Use native REAPER time string format
                    None => {
                        let text = reaper.format_timestr_pos(
                            self.reversely_corrected_display_pos(),
                            32,
                            TimeModeOverride::Mode(mode),
                        );
                        Some(PropValue::Text(text.into_string().into()))
                    }
                    // Use format tailored to Mackie timecode display
                    Some("mcu") => {
                        let text = match mode {
                            Samples => {
                                let text = reaper.format_timestr_pos(
                                    self.reversely_corrected_display_pos(),
                                    32,
                                    TimeModeOverride::Mode(TimeMode::Samples),
                                );
                                text.into_string()
                            }
                            Time => {
                                let text = reaper.format_timestr_pos(
                                    self.reversely_corrected_display_pos(),
                                    32,
                                    TimeModeOverride::Mode(TimeMode::Time),
                                );
                                // [*h:]?m:ss.fff
                                let mut comp = text.to_str().split(&[':', '.'][..]);
                                match (comp.next(), comp.next(), comp.next(), comp.next()) {
                                    (Some(m), Some(ss), Some(fff), None) => {
                                        format!("{m:0>2}{ss:0>2}{fff:0>3}")
                                    }
                                    (Some(h), Some(m), Some(ss), Some(fff)) => {
                                        format!("{h}{m:0>2}{ss:0>2}{fff:0>3}")
                                    }
                                    _ => String::new(),
                                }
                            }
                            MeasuresBeatsTime | MeasuresBeats | MeasuresBeatsMinimal => {
                                let text = reaper.format_timestr_pos(
                                    self.reversely_corrected_display_pos(),
                                    32,
                                    TimeModeOverride::Mode(TimeMode::MeasuresBeatsTime),
                                );
                                // *m.b.ff
                                let mut comp = text.to_str().split('.');
                                if let (Some(m), Some(b), Some(ff)) =
                                    (comp.next(), comp.next(), comp.next())
                                {
                                    format!("{m}{b:>2}   {ff:0>2}")
                                } else {
                                    String::new()
                                }
                            }
                            Seconds => {
                                let pos = self.corrected_display_pos().get();
                                format!(
                                    "{}{} {:02}",
                                    if pos.is_sign_negative() { "-" } else { "" },
                                    pos.abs() as i32,
                                    (pos.abs() * 100.0) as i32 % 100
                                )
                            }
                            HoursMinutesSecondsFrames => {
                                let text = reaper.format_timestr_pos(
                                    self.reversely_corrected_display_pos(),
                                    32,
                                    TimeModeOverride::Mode(TimeMode::HoursMinutesSecondsFrames),
                                );
                                // *hh:mm:ss:ff
                                let mut comp = text.to_str().split(':');
                                if let (Some(hh), Some(mm), Some(ss), Some(ff)) =
                                    (comp.next(), comp.next(), comp.next(), comp.next())
                                {
                                    format!("{hh}{mm:0>2}{ss:0>2} {ff:0>2}")
                                } else {
                                    String::new()
                                }
                            }
                            AbsoluteFrames => {
                                let text = reaper.format_timestr_pos(
                                    self.reversely_corrected_display_pos(),
                                    32,
                                    TimeModeOverride::Mode(TimeMode::AbsoluteFrames),
                                );
                                text.into_string()
                            }
                            Unknown(m) => format!("{m:?}"),
                        };
                        Some(PropValue::Text(text.into()))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl SeekTarget {
    /// Substracts the project time offset if the seek context is not the project.
    pub fn reversely_corrected_display_pos(&self) -> PositionInSeconds {
        let (pos, seek_info) = self.internal_display_info();
        if seek_info.context == SeekContext::Project {
            pos
        } else {
            pos - self.project.time_offset()
        }
    }

    /// Adds the project time offset if the seek context is the project.
    pub fn corrected_display_pos(&self) -> PositionInSeconds {
        let (pos, seek_info) = self.internal_display_info();
        if seek_info.context == SeekContext::Project {
            self.project.time_offset() + pos
        } else {
            pos
        }
    }

    fn internal_display_info(&self) -> (PositionInSeconds, SeekInfo) {
        let pos = self
            .project
            .play_or_edit_cursor_position()
            .unwrap_or_default();
        let info = get_seek_info(self.project, self.options, true);
        (pos - info.start_pos, info)
    }
}

#[derive(Eq, PartialEq)]
enum SeekContext {
    TimeSelection,
    LoopPoints,
    Region,
    Project,
    Viewport,
}

impl<'a> Target<'a> for SeekTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = current_value_of_seek(
            self.project,
            self.options,
            self.project
                .play_or_edit_cursor_position()
                .unwrap_or_default(),
        );
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

fn current_value_of_seek(
    project: Project,
    options: SeekOptions,
    pos: PositionInSeconds,
) -> UnitValue {
    let info = get_seek_info(project, options, false);
    if pos < info.start_pos {
        UnitValue::MIN
    } else {
        let pos_within_range = pos.get() - info.start_pos.get();
        UnitValue::new_clamped(pos_within_range / info.length())
    }
}

fn get_seek_info(project: Project, options: SeekOptions, ignore_project_length: bool) -> SeekInfo {
    if options.use_time_selection {
        if let Some(r) = project.time_selection() {
            return SeekInfo::from_time_range(SeekContext::TimeSelection, r);
        }
    }
    if options.use_loop_points {
        if let Some(r) = project.loop_points() {
            return SeekInfo::from_time_range(SeekContext::LoopPoints, r);
        }
    }
    if options.use_regions {
        let bm = project.current_bookmark();
        if let Some(i) = bm.region_index {
            if let Some(bm) = project.find_bookmark_by_index(i) {
                let info = bm.basic_info();
                if let Some(end_pos) = info.region_end_position {
                    return SeekInfo::new(SeekContext::Region, info.position, end_pos);
                }
            }
        }
    }
    if options.use_project {
        if ignore_project_length {
            return SeekInfo::new(
                SeekContext::Project,
                PositionInSeconds::ZERO,
                PositionInSeconds::new_panic(f64::MAX),
            );
        } else {
            let length = project.length();
            if length.get() > 0.0 {
                return SeekInfo::new(SeekContext::Project, PositionInSeconds::ZERO, length.into());
            }
        }
    }
    // Last fallback: Viewport seeking. We always have a viewport
    let result = Reaper::get()
        .medium_reaper()
        .get_set_arrange_view_2_get(project.context(), 0, 0);
    SeekInfo::new(SeekContext::Viewport, result.start_time, result.end_time)
}

struct SeekInfo {
    pub context: SeekContext,
    pub start_pos: PositionInSeconds,
    pub end_pos: PositionInSeconds,
}

impl SeekInfo {
    pub fn new(
        context: SeekContext,
        start_pos: PositionInSeconds,
        end_pos: PositionInSeconds,
    ) -> Self {
        Self {
            context,
            start_pos,
            end_pos,
        }
    }

    fn from_time_range(context: SeekContext, range: GetLoopTimeRange2Result) -> Self {
        Self::new(context, range.start, range.end)
    }

    pub fn length(&self) -> f64 {
        self.end_pos.get() - self.start_pos.get()
    }
}

pub const SEEK_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Project,
    name: "Seek",
    short_name: "Seek",
    supports_feedback_resolution: true,
    supports_seek_behavior: true,
    ..DEFAULT_TARGET
};
