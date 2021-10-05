use crate::domain::{
    AdditionalFeedbackEvent, ControlContext, HitInstructionReturnValue, MappingControlContext,
    RealearnTarget, ReaperTargetType, SeekOptions, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_high::{Project, Reaper};
use reaper_medium::{GetLoopTimeRange2Result, PositionInSeconds, SetEditCurPosOptions};

#[derive(Clone, Debug, PartialEq)]
pub struct SeekTarget {
    pub project: Project,
    pub options: SeekOptions,
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_unit_value()?;
        let info = get_seek_info(self.project, self.options);
        let desired_pos_within_range = value.get() * info.length();
        let desired_pos = info.start_pos.get() + desired_pos_within_range;
        self.project.set_edit_cursor_position(
            PositionInSeconds::new(desired_pos),
            SetEditCurPosOptions {
                move_view: self.options.move_view,
                seek_play: self.options.seek_play,
            },
        );
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            AdditionalFeedbackEvent::BeatChanged(e) if e.project == self.project => {
                let v = current_value_of_seek(self.project, self.options, e.new_value);
                (true, Some(AbsoluteValue::Continuous(v)))
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(format!("{:.3} s", self.position_in_seconds().get()))
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let seconds = self.position_in_seconds();
        Some(NumericValue::Decimal(seconds.get()))
    }

    fn numeric_value_unit(&self, _: ControlContext) -> &'static str {
        "s"
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Seek)
    }
}

impl SeekTarget {
    fn position_in_seconds(&self) -> PositionInSeconds {
        let pos = self.project.play_or_edit_cursor_position();
        let info = get_seek_info(self.project, self.options);
        if pos < info.start_pos {
            PositionInSeconds::new(0.0)
        } else {
            pos - info.start_pos
        }
    }
}

impl<'a> Target<'a> for SeekTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = current_value_of_seek(
            self.project,
            self.options,
            self.project.play_or_edit_cursor_position(),
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
    let info = get_seek_info(project, options);
    if pos < info.start_pos {
        UnitValue::MIN
    } else {
        let pos_within_range = pos.get() - info.start_pos.get();
        UnitValue::new_clamped(pos_within_range / info.length())
    }
}

fn get_seek_info(project: Project, options: SeekOptions) -> SeekInfo {
    if options.use_time_selection {
        if let Some(r) = project.time_selection() {
            return SeekInfo::from_time_range(r);
        }
    }
    if options.use_loop_points {
        if let Some(r) = project.loop_points() {
            return SeekInfo::from_time_range(r);
        }
    }
    if options.use_regions {
        let bm = project.current_bookmark();
        if let Some(i) = bm.region_index {
            if let Some(bm) = project.find_bookmark_by_index(i) {
                let info = bm.basic_info();
                if let Some(end_pos) = info.region_end_position {
                    return SeekInfo::new(info.position, end_pos);
                }
            }
        }
    }
    if options.use_project {
        let length = project.length();
        if length.get() > 0.0 {
            return SeekInfo::new(
                PositionInSeconds::new(0.0),
                PositionInSeconds::new(length.get()),
            );
        }
    }
    // Last fallback: Viewport seeking. We always have a viewport
    let result = Reaper::get()
        .medium_reaper()
        .get_set_arrange_view_2_get(project.context(), 0, 0);
    SeekInfo::new(result.start_time, result.end_time)
}

struct SeekInfo {
    pub start_pos: PositionInSeconds,
    pub end_pos: PositionInSeconds,
}

impl SeekInfo {
    pub fn new(start_pos: PositionInSeconds, end_pos: PositionInSeconds) -> Self {
        Self { start_pos, end_pos }
    }

    fn from_time_range(range: GetLoopTimeRange2Result) -> Self {
        Self::new(range.start, range.end)
    }

    pub fn length(&self) -> f64 {
        self.end_pos.get() - self.start_pos.get()
    }
}
