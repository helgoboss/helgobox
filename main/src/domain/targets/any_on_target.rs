use crate::domain::{
    AdditionalFeedbackEvent, CompartmentKind, CompoundChangeEvent, ControlContext, DEFAULT_TARGET,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, UnresolvedReaperTargetDef,
    format_value_as_on_off,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use helgobox_api::persistence::AnyOnParameter;
use reaper_high::{ChangeEvent, GroupingBehavior, Project, Reaper};
use reaper_medium::GangBehavior;
use std::borrow::Cow;
use std::ptr::null_mut;
use swell_ui::Window;

#[derive(Debug)]
pub struct UnresolvedAnyOnTarget {
    pub parameter: AnyOnParameter,
}

impl UnresolvedReaperTargetDef for UnresolvedAnyOnTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::AnyOn(AnyOnTarget {
            project: context.context().project_or_current_project(),
            parameter: self.parameter,
        })])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnyOnTarget {
    pub project: Project,
    pub parameter: AnyOnParameter,
}

impl RealearnTarget for AnyOnTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // Retriggerable because the logic of this target is unusual: Pressing a button (= receiving
        // on = 100%) is supposed to switch everything to *off*. So the desired target value doesn't
        // correspond to the incoming value.
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        for t in self.project.tracks() {
            use AnyOnParameter::*;
            unsafe {
                match self.parameter {
                    TrackSolo => {
                        t.unsolo(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping)
                    }
                    TrackMute => {
                        t.unmute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping)
                    }
                    TrackArm => t.disarm(
                        false,
                        GangBehavior::DenyGang,
                        GroupingBehavior::PreventGrouping,
                    ),
                    TrackSelection => t.unselect(),
                    MidiEditorFocus => Reaper::get()
                        .medium_reaper()
                        .low()
                        .SetCursorContext(1, null_mut()),
                }
            }
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use AnyOnParameter::*;
        use CompoundChangeEvent::*;
        match evt {
            Reaper(ChangeEvent::TrackSoloChanged(e))
                if self.parameter == TrackSolo && e.track.project() == self.project =>
            {
                (true, None)
            }
            Reaper(ChangeEvent::TrackMuteChanged(e))
                if self.parameter == TrackMute && e.track.project() == self.project =>
            {
                (true, None)
            }
            Reaper(ChangeEvent::TrackArmChanged(e))
                if self.parameter == TrackArm && e.track.project() == self.project =>
            {
                (true, None)
            }
            Reaper(ChangeEvent::TrackSelectedChanged(e))
                if self.parameter == TrackSelection && e.track.project() == self.project =>
            {
                (true, None)
            }
            Additional(AdditionalFeedbackEvent::MidiEditorFocusChanged)
                if self.parameter == MidiEditorFocus =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::AnyOn)
    }
}

impl<'a> Target<'a> for AnyOnTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        use AnyOnParameter::*;
        let on = match self.parameter {
            TrackSolo => self.project.any_solo(),
            TrackMute => self.project.tracks().any(|t| t.is_muted()),
            TrackArm => self.project.tracks().any(|t| t.is_armed(false)),
            TrackSelection => self.project.tracks().any(|t| t.is_selected()),
            MidiEditorFocus => focused_midi_editor().is_some(),
        };
        Some(AbsoluteValue::from_bool(on))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const ANY_ON_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Project,
    name: "Any on (solo/mute/...)",
    short_name: "Any on",
    ..DEFAULT_TARGET
};

pub fn focused_midi_editor() -> Option<Window> {
    let active_midi_editor_window = Reaper::get()
        .medium_reaper()
        .midi_editor_get_active()
        .map(Window::from_hwnd)?;
    let focused_window = Window::focused()?;
    let focused = active_midi_editor_window == focused_window
        || active_midi_editor_window.is_child_of(focused_window);
    if focused {
        Some(active_midi_editor_window)
    } else {
        None
    }
}
