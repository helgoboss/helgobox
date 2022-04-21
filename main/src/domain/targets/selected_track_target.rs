use crate::domain::{
    convert_count_to_step_size, convert_unit_value_to_track_index, get_track_name,
    selected_track_unit_value, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitInstructionReturnValue, MappingControlContext, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use reaper_high::{ChangeEvent, Project, Reaper, Track};
use reaper_medium::{CommandId, MasterTrackBehavior};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedSelectedTrackTarget {
    pub scroll_arrange_view: bool,
    pub scroll_mixer: bool,
}

impl UnresolvedReaperTargetDef for UnresolvedSelectedTrackTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::SelectedTrack(SelectedTrackTarget {
            project: context.context().project_or_current_project(),
            scroll_arrange_view: self.scroll_arrange_view,
            scroll_mixer: self.scroll_mixer,
        })])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectedTrackTarget {
    pub project: Project,
    pub scroll_arrange_view: bool,
    pub scroll_mixer: bool,
}

impl RealearnTarget for SelectedTrackTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // `+ 1` because "<Master track>" is also a possible value.
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(self.project.track_count() + 1),
                is_retriggerable: false,
            },
            TargetCharacter::Discrete,
        )
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        _: ControlContext,
    ) -> Result<u32, &'static str> {
        let value = convert_unit_value_to_track_index(self.project, input)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        match convert_unit_value_to_track_index(self.project, value) {
            None => "<Master track>".to_string(),
            Some(i) => (i + 1).to_string(),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let track_index = match value.to_absolute_value()? {
            AbsoluteValue::Continuous(v) => convert_unit_value_to_track_index(self.project, v),
            AbsoluteValue::Discrete(f) => {
                if f.actual() == 0 {
                    None
                } else {
                    Some(f.actual() - 1)
                }
            }
        };
        let track = match track_index {
            None => self.project.master_track(),
            Some(i) => self
                .project
                .track_by_index(i)
                .ok_or("track not available")?,
        };
        track.select_exclusively();
        if self.scroll_arrange_view {
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()));
        }
        if self.scroll_mixer {
            track.scroll_mixer();
        }
        Ok(None)
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
            CompoundChangeEvent::Reaper(ChangeEvent::TrackSelectedChanged(e))
                if e.track.project() == self.project =>
            {
                (true, Some(self.value_for(e.track.index())))
            }
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        Ok(selected_track_unit_value(self.project, index))
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(get_track_name(&self.selected_track()?).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let index = self.selected_track()?.index()?;
        Some(NumericValue::Discrete(index as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::SelectedTrack)
    }
}

impl<'a> Target<'a> for SelectedTrackTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let track_index = self
            .project
            .first_selected_track(MasterTrackBehavior::ExcludeMasterTrack)
            .and_then(|t| t.index());
        Some(self.value_for(track_index))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl SelectedTrackTarget {
    fn value_for(&self, track_index: Option<u32>) -> AbsoluteValue {
        percentage_for_track_within_project(self.project, track_index)
    }

    fn selected_track(&self) -> Option<Track> {
        self.project
            .first_selected_track(MasterTrackBehavior::IncludeMasterTrack)
    }
}

pub fn percentage_for_track_within_project(
    project: Project,
    track_index: Option<u32>,
) -> AbsoluteValue {
    let track_count = project.track_count();
    // Because we count "<Master track>" as a possible value, this is equal.
    let max_value = track_count;
    let actual_value = track_index.map(|i| i + 1).unwrap_or(0);
    AbsoluteValue::Discrete(Fraction::new(actual_value, max_value))
}

pub const SELECTED_TRACK_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Project: Navigate between tracks",
    short_name: "Navigate tracks",
    supports_track_scrolling: true,
    ..DEFAULT_TARGET
};
