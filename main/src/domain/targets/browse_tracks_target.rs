use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value_with_none,
    convert_unit_to_discrete_value_with_none, get_reaper_track_area_of_scope,
    get_track_by_scoped_index, get_track_name, scoped_track_index, Compartment,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use realearn_api::persistence::{BrowseTracksMode, TrackScope};
use reaper_high::{ChangeEvent, Project, Reaper, Track};
use reaper_medium::{CommandId, MasterTrackBehavior};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedBrowseTracksTarget {
    pub scroll_arrange_view: bool,
    pub scroll_mixer: bool,
    pub mode: BrowseTracksMode,
}

impl UnresolvedReaperTargetDef for UnresolvedBrowseTracksTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::BrowseTracks(BrowseTracksTarget {
            project: context.context().project_or_current_project(),
            scroll_arrange_view: self.scroll_arrange_view,
            scroll_mixer: self.scroll_mixer,
            mode: self.mode,
        })])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowseTracksTarget {
    pub project: Project,
    pub scroll_arrange_view: bool,
    pub scroll_mixer: bool,
    pub mode: BrowseTracksMode,
}

impl RealearnTarget for BrowseTracksTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: self.step_size(),
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
        let value = convert_unit_value_to_track_index(self.project, self.mode.scope(), input)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        match convert_unit_value_to_track_index(self.project, self.mode.scope(), value) {
            None => "<Master track>".to_string(),
            Some(i) => (i + 1).to_string(),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let track_index = match value.to_absolute_value()? {
            AbsoluteValue::Continuous(v) => {
                convert_unit_value_to_track_index(self.project, self.mode.scope(), v)
            }
            AbsoluteValue::Discrete(f) => {
                if f.actual() == 0 {
                    None
                } else {
                    Some(f.actual() - 1)
                }
            }
        };
        let track = match track_index {
            None => self.project.master_track()?,
            Some(i) => get_track_by_scoped_index(self.project, i, self.mode.scope())
                .ok_or("track not available")?,
        };
        select_track_exclusively_scoped(&track, self.mode);
        if self.scroll_arrange_view {
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()))
                .expect("built-in action should exist");
        }
        if self.scroll_mixer {
            track.scroll_mixer();
        }
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
            CompoundChangeEvent::Reaper(ChangeEvent::TrackSelectedChanged(e))
                if e.track.project() == self.project =>
            {
                (true, None)
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
        let track_count = scoped_track_count(self.project, self.mode.scope());
        let uv = convert_discrete_to_unit_value_with_none(index, track_count);
        Ok(uv)
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        match self.first_selected_track()? {
            ScopedTrack::InScope(t) => {
                let name = get_track_name(&t, self.mode.scope());
                Some(name.into())
            }
            ScopedTrack::OutOfScope { floor_track } => {
                let name = get_track_name(&floor_track, self.mode.scope());
                Some(format!("After {name}").into())
            }
        }
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        match self.first_selected_track()? {
            ScopedTrack::InScope(t) => {
                let index = scoped_track_index(&t, self.mode.scope())?;
                Some(NumericValue::Discrete(index as i32 + 1))
            }
            ScopedTrack::OutOfScope { floor_track } => {
                let index = scoped_track_index(&floor_track, self.mode.scope())?;
                Some(NumericValue::Decimal(index as f64 + 1.5))
            }
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::BrowseTracks)
    }
}

impl<'a> Target<'a> for BrowseTracksTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        match self.first_selected_track() {
            None => Some(self.percentage_for(None)),
            Some(ScopedTrack::InScope(track)) => {
                let track_index = scoped_track_index(&track, self.mode.scope());
                Some(self.percentage_for(track_index))
            }
            Some(ScopedTrack::OutOfScope { floor_track }) => {
                let floor_track_index = scoped_track_index(&floor_track, self.mode.scope());
                let floor_percentage = self.percentage_for(floor_track_index);
                // Add half of the atomic step size to indicate that it's inbetween two values!
                let step_size = self.step_size();
                let inbetween_percentage =
                    floor_percentage.to_unit_value().get() + step_size.get() / 2.0;
                UnitValue::try_new(inbetween_percentage).map(AbsoluteValue::Continuous)
            }
        }
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl BrowseTracksTarget {
    fn percentage_for(&self, track_index: Option<u32>) -> AbsoluteValue {
        percentage_for_scoped_track_within_project(self.project, self.mode.scope(), track_index)
    }

    fn first_selected_track(&self) -> Option<ScopedTrack> {
        first_selected_track_scoped(self.project, self.mode)
    }

    fn step_size(&self) -> UnitValue {
        // `+ 1` because "<Master track>" is also a possible value.
        let count = scoped_track_count(self.project, self.mode.scope()) + 1;
        convert_count_to_step_size(count)
    }
}

enum ScopedTrack {
    InScope(Track),
    /// Selected track is out of scope (e.g. we are interested in TCP scope but it#s only visible
    /// in MCP).
    OutOfScope {
        /// Out-of-scope track comes after this in-scope track.
        floor_track: Track,
    },
}

pub const SELECTED_TRACK_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Project,
    name: "Browse tracks",
    short_name: "Browse tracks",
    supports_track_scrolling: true,
    ..DEFAULT_TARGET
};

pub fn percentage_for_scoped_track_within_project(
    project: Project,
    policy: TrackScope,
    track_index: Option<u32>,
) -> AbsoluteValue {
    let track_count = scoped_track_count(project, policy);
    // Because we count "<Master track>" as a possible value, this is equal.
    let max_value = track_count;
    let actual_value = track_index.map(|i| i + 1).unwrap_or(0);
    AbsoluteValue::Discrete(Fraction::new(actual_value, max_value))
}

fn scoped_track_count(project: Project, scope: TrackScope) -> u32 {
    use TrackScope::*;
    match scope {
        AllTracks => project.track_count(),
        TracksVisibleInTcp | TracksVisibleInMcp => {
            let track_area = get_reaper_track_area_of_scope(scope);
            project.tracks().filter(|t| t.is_shown(track_area)).count() as _
        }
    }
}

fn convert_unit_value_to_track_index(
    project: Project,
    scope: TrackScope,
    value: UnitValue,
) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, scoped_track_count(project, scope))
}

fn select_track_exclusively_scoped(track: &Track, mode: BrowseTracksMode) {
    use BrowseTracksMode::*;
    match mode {
        AllTracks | TracksVisibleInTcp | TracksVisibleInMcp => {
            track.select_exclusively();
        }
        TracksVisibleInTcpAllowTwoSelections | TracksVisibleInMcpAllowTwoSelections => {
            let track_area = get_reaper_track_area_of_scope(mode.scope());
            for t in track
                .project()
                .tracks()
                .filter(|t| t != track && t.is_shown(track_area))
            {
                t.unselect();
            }
            track.select();
        }
    }
}

fn first_selected_track_scoped(project: Project, mode: BrowseTracksMode) -> Option<ScopedTrack> {
    use BrowseTracksMode::*;
    let master_track_behavior = MasterTrackBehavior::ExcludeMasterTrack;
    match mode {
        AllTracks => project
            .first_selected_track(master_track_behavior)
            .map(ScopedTrack::InScope),
        TracksVisibleInTcp | TracksVisibleInMcp => {
            let first_selected_track = project.first_selected_track(master_track_behavior)?;
            let track_area = get_reaper_track_area_of_scope(mode.scope());
            if first_selected_track.is_shown(track_area) {
                Some(ScopedTrack::InScope(first_selected_track))
            } else {
                let selected_track_index = first_selected_track.index()?;
                // Find the first visible track above the currently selected one
                project
                    .tracks()
                    // Enumerate from first track
                    .enumerate()
                    // Search starting from last track
                    .rev()
                    .find(|(i, t)| *i < selected_track_index as usize && t.is_shown(track_area))
                    .map(|(_, floor_track)| ScopedTrack::OutOfScope { floor_track })
            }
        }
        TracksVisibleInTcpAllowTwoSelections | TracksVisibleInMcpAllowTwoSelections => {
            let mut candidate = None;
            for t in project.selected_tracks(master_track_behavior) {
                let track_area = get_reaper_track_area_of_scope(mode.scope());
                let other_track_area = get_other_track_area(track_area);
                if t.is_shown(track_area) {
                    // Track is shown in the relevant area. Good.
                    if candidate.is_none() && t.is_shown(other_track_area) {
                        // Track is also shown in the other area, so it might not be the final
                        // result, but it's at least a candidate.
                        candidate = Some(t);
                    } else {
                        // Track is only shown in the relevant area. Perfect.
                        return Some(ScopedTrack::InScope(t));
                    }
                }
            }
            candidate.map(ScopedTrack::InScope)
        }
    }
}

fn get_other_track_area(track_area: reaper_medium::TrackArea) -> reaper_medium::TrackArea {
    use reaper_medium::TrackArea::*;
    match track_area {
        Tcp => Mcp,
        Mcp => Tcp,
    }
}
