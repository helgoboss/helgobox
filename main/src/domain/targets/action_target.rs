use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_bool_as_on_off, get_effective_tracks, ActionInvocationType, AdditionalFeedbackEvent,
    CompartmentKind, CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetSection, TargetTypeDef, TrackDescriptor, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use camino::Utf8Path;
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};
use helgoboss_midi::{U14, U7};
use helgobox_api::persistence::ActionScope;
use reaper_high::{Action, ActionCharacter, Project, Reaper, Track};
use reaper_medium::{
    ActionValueChange, CommandId, Hwnd, MasterTrackBehavior, OpenMediaExplorerMode,
};
use std::borrow::Cow;
use std::convert::TryFrom;

#[derive(Debug)]
pub struct UnresolvedActionTarget {
    pub action: Action,
    pub scope: ActionScope,
    pub invocation_type: ActionInvocationType,
    pub track_descriptor: Option<TrackDescriptor>,
}

impl UnresolvedReaperTargetDef for UnresolvedActionTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context().project_or_current_project();
        let resolved_targets = if let Some(td) = &self.track_descriptor {
            get_effective_tracks(context, &td.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::Action(ActionTarget {
                        action: self.action.clone(),
                        scope: self.scope,
                        invocation_type: self.invocation_type,
                        project,
                        track: Some(track),
                    })
                })
                .collect()
        } else {
            vec![ReaperTarget::Action(ActionTarget {
                action: self.action.clone(),
                scope: self.scope,
                invocation_type: self.invocation_type,
                project,
                track: None,
            })]
        };
        Ok(resolved_targets)
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        self.track_descriptor.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionTarget {
    pub action: Action,
    pub scope: ActionScope,
    pub invocation_type: ActionInvocationType,
    pub project: Project,
    pub track: Option<Track>,
}

impl RealearnTarget for ActionTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        match self.invocation_type {
            ActionInvocationType::Trigger => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            ),
            ActionInvocationType::Absolute14Bit | ActionInvocationType::Absolute7Bit => {
                match self.action.character() {
                    Ok(ActionCharacter::Toggle) => {
                        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
                    }
                    Ok(ActionCharacter::Trigger) => {
                        // If the action character is not toggle, it will emit ReaLearn-generated "fake" values,
                        // because natively, trigger-like REAPER actions don't have the concept of a current value.
                        // If that fake value is incorrect and the control type is not retriggerable, ReaLearn won't
                        // fire. Bad. That's why we need to make it retriggerable.
                        (
                            ControlType::AbsoluteContinuousRetriggerable,
                            TargetCharacter::Continuous,
                        )
                    }
                    Err(_) => (
                        ControlType::AbsoluteContinuousRetriggerable,
                        TargetCharacter::Continuous,
                    ),
                }
            }
            ActionInvocationType::Relative => (ControlType::Relative, TargetCharacter::Discrete),
        }
    }

    fn open(&self, _: ControlContext) {
        // Just open action window
        Reaper::get()
            .main_section()
            .action_by_command_id(CommandId::new(40605))
            .invoke_as_trigger(Some(self.project), None)
            .expect("built-in action should exist");
    }

    fn format_value(&self, _: UnitValue, _: ControlContext) -> String {
        "".to_owned()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        if let Some(track) = &self.track {
            if !track.is_selected()
                || self
                    .project
                    .selected_track_count(MasterTrackBehavior::IncludeMasterTrack)
                    > 1
            {
                track.select_exclusively();
            }
        }
        let response = match value {
            ControlValue::AbsoluteContinuous(v) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if v.is_zero() {
                        HitResponse::ignored()
                    } else {
                        self.invoke_absolute_with_unit_value(v, false)?;
                        HitResponse::processed_with_effect()
                    }
                }
                ActionInvocationType::Absolute14Bit => {
                    self.invoke_absolute_with_unit_value(v, false)?;
                    HitResponse::processed_with_effect()
                }
                ActionInvocationType::Absolute7Bit => {
                    self.invoke_absolute_with_unit_value(v, true)?;
                    HitResponse::processed_with_effect()
                }
                ActionInvocationType::Relative => {
                    return Err("relative invocation type can't take absolute values");
                }
            },
            ControlValue::AbsoluteDiscrete(f) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if f.is_zero() {
                        HitResponse::ignored()
                    } else {
                        self.invoke_absolute_with_fraction(f, false)?;
                        HitResponse::processed_with_effect()
                    }
                }
                ActionInvocationType::Absolute14Bit => {
                    self.invoke_absolute_with_fraction(f, false)?;
                    HitResponse::processed_with_effect()
                }
                ActionInvocationType::Absolute7Bit => {
                    self.invoke_absolute_with_fraction(f, true)?;
                    HitResponse::processed_with_effect()
                }
                ActionInvocationType::Relative => {
                    return Err("relative invocation type can't take absolute values");
                }
            },
            ControlValue::RelativeDiscrete(i) => {
                if let ActionInvocationType::Relative = self.invocation_type {
                    self.action.invoke_relative(
                        i.get(),
                        Some(self.project),
                        self.get_context_window(),
                    )?;
                    HitResponse::processed_with_effect()
                } else {
                    return Err("relative values need relative invocation type");
                }
            }
            ControlValue::RelativeContinuous(i) => {
                if let ActionInvocationType::Relative = self.invocation_type {
                    let i = i.to_discrete_increment();
                    self.action.invoke_relative(
                        i.get(),
                        Some(self.project),
                        self.get_context_window(),
                    )?;
                    HitResponse::processed_with_effect()
                } else {
                    return Err("relative values need relative invocation type");
                }
            }
        };
        Ok(response)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.action.is_available()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            // We can't provide a value from the event itself because the action hooks don't
            // pass values.
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::ActionInvoked(e))
                if Ok(e.command_id) == self.action.command_id() =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_bool_as_on_off(self.action.is_on().ok()??).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Action)
    }
}

impl<'a> Target<'a> for ActionTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = if let Some(state) = self.action.is_on().ok()? {
            // Toggle action: Return toggle state as 0 or 1.
            convert_bool_to_unit_value(state)
        } else if self.invocation_type.is_absolute() {
            // Absolute non-toggle action. Try to return current absolute "fake" value if this is a
            // MIDI CC/mousewheel action.
            if let Some(value) = self.action.normalized_value() {
                UnitValue::new(value)
            } else {
                UnitValue::MIN
            }
        } else {
            // Relative or trigger. Not returning any "fake" value here because this will let the
            // glue section make wrong assumptions, especially for "Trigger" invocation mode.
            return None;
        };
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl ActionTarget {
    fn invoke_absolute_with_fraction(
        &self,
        f: Fraction,
        enforce_7_bit: bool,
    ) -> Result<(), &'static str> {
        let value_change = if enforce_7_bit {
            let v = U7::try_from(f.actual()).map_err(|_| "couldn't convert to U7")?;
            ActionValueChange::AbsoluteLowRes(v)
        } else {
            let v = U14::try_from(f.actual()).map_err(|_| "couldn't convert to U14")?;
            ActionValueChange::AbsoluteHighRes(v)
        };
        self.action.invoke_directly(
            value_change,
            self.get_context_window(),
            self.project.context(),
        )?;
        Ok(())
    }

    fn invoke_absolute_with_unit_value(
        &self,
        v: UnitValue,
        enforce_7_bit: bool,
    ) -> Result<(), &'static str> {
        self.action.invoke_absolute(
            v.get(),
            Some(self.project),
            enforce_7_bit,
            self.get_context_window(),
        )?;
        Ok(())
    }

    fn get_context_window(&self) -> Option<Hwnd> {
        match self.scope {
            ActionScope::Main => None,
            ActionScope::ActiveMidiEditor | ActionScope::ActiveMidiEventListEditor => {
                Reaper::get().medium_reaper().midi_editor_get_active()
            }
            ActionScope::MediaExplorer => Reaper::get()
                .medium_reaper()
                .open_media_explorer(Utf8Path::new(""), OpenMediaExplorerMode::Select),
        }
    }
}

pub const ACTION_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Project,
    name: "Invoke REAPER action",
    short_name: "Action",
    hint: "Limited feedback only",
    supports_track: true,
    if_so_supports_track_must_be_selected: false,
    ..DEFAULT_TARGET
};
