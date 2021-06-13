use crate::domain::{
    current_value_of_bookmark, format_value_as_on_off, AdditionalFeedbackEvent, ControlContext,
    RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{BookmarkType, ChangeEvent, Project};
use reaper_medium::{AutoSeekBehavior, BookmarkRef};
use std::num::NonZeroU32;

#[derive(Clone, Debug, PartialEq)]
pub struct GoToBookmarkTarget {
    pub project: Project,
    pub bookmark_type: BookmarkType,
    // This counts both markers and regions. We need it for getting the current value.
    pub index: u32,
    // This counts either only markers or only regions. We need it for control. The alternative
    // would be an ID but unfortunately, marker IDs are not unique which means we would
    // unnecessarily lack reliability to go to markers in a position-based way.
    pub position: NonZeroU32,
    pub set_time_selection: bool,
    pub set_loop_points: bool,
}

impl RealearnTarget for GoToBookmarkTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if !value.to_unit_value()?.is_zero() {
            match self.bookmark_type {
                BookmarkType::Marker => self
                    .project
                    .go_to_marker(BookmarkRef::Position(self.position)),
                BookmarkType::Region => {
                    self.project
                        .go_to_region_with_smooth_seek(BookmarkRef::Position(self.position));
                    if self.set_loop_points || self.set_time_selection {
                        if let Some(bookmark) = self.project.find_bookmark_by_type_and_index(
                            BookmarkType::Region,
                            self.position.get() - 1,
                        ) {
                            if let Some(end_pos) = bookmark.basic_info.region_end_position {
                                if self.set_loop_points {
                                    self.project.set_loop_points(
                                        bookmark.basic_info.position,
                                        end_pos,
                                        AutoSeekBehavior::DenyAutoSeek,
                                    );
                                }
                                if self.set_time_selection {
                                    self.project
                                        .set_time_selection(bookmark.basic_info.position, end_pos);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        // Handled both from control-surface and non-control-surface callbacks.
        match evt {
            ChangeEvent::BookmarksChanged(e) if e.project == self.project => (true, None),
            _ => (false, None),
        }
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            AdditionalFeedbackEvent::BeatChanged(e) if e.project == self.project => {
                let v = current_value_of_bookmark(
                    self.project,
                    self.bookmark_type,
                    self.index,
                    e.new_value,
                );
                (true, Some(AbsoluteValue::Continuous(v)))
            }
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for GoToBookmarkTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let val = current_value_of_bookmark(
            self.project,
            self.bookmark_type,
            self.index,
            self.project.play_or_edit_cursor_position(),
        );
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
