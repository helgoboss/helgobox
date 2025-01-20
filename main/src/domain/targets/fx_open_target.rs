use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_value_as_on_off, CompartmentKind, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, FxDescriptor, FxDisplayType, HitResponse, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};
use reaper_medium::FxChainVisibility;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedFxOpenTarget {
    pub fx_descriptor: FxDescriptor,
    pub display_type: FxDisplayType,
}

impl UnresolvedReaperTargetDef for UnresolvedFxOpenTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(self
            .fx_descriptor
            .resolve(context, compartment)?
            .into_iter()
            .map(|fx| {
                ReaperTarget::FxOpen(FxOpenTarget {
                    fx,
                    display_type: self.display_type,
                })
            })
            .collect())
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxOpenTarget {
    pub fx: Fx,
    pub display_type: FxDisplayType,
}

impl RealearnTarget for FxOpenTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        use FxDisplayType::*;
        if value.to_unit_value()?.is_zero() {
            match self.display_type {
                FloatingWindow => {
                    self.fx.hide_floating_window()?;
                }
                Chain => {
                    self.fx.chain().hide()?;
                }
            }
        } else {
            match self.display_type {
                FloatingWindow => {
                    self.fx.show_in_floating_window()?;
                }
                Chain => {
                    self.fx.show_in_chain()?;
                }
            }
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.fx.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.fx.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx.track()
    }

    fn fx(&self) -> Option<&Fx> {
        Some(&self.fx)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use CompoundChangeEvent::*;
        match evt {
            Reaper(ChangeEvent::FxOpened(e)) if e.fx == self.fx => (true, None),
            Reaper(ChangeEvent::FxClosed(e)) if e.fx == self.fx => (true, None),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxOpen)
    }
}

impl<'a> Target<'a> for FxOpenTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        use FxDisplayType::*;
        let is_open = match self.display_type {
            FloatingWindow => self.fx.floating_window().is_some(),
            Chain => {
                use FxChainVisibility::*;
                match self.fx.chain().visibility() {
                    Hidden | Visible(None) | Unknown(_) => false,
                    Visible(Some(i)) => self.fx.index() == i,
                }
            }
        };
        Some(AbsoluteValue::Continuous(convert_bool_to_unit_value(
            is_open,
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_OPEN_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::Fx,
    name: "Open/close",
    short_name: "Open/close FX",
    supports_track: true,
    supports_fx: true,
    supports_fx_display_type: true,
    ..DEFAULT_TARGET
};
