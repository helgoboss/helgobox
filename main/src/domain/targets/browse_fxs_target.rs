use crate::domain::{
    convert_count_to_step_size, convert_unit_value_to_fx_index, get_fx_chains, get_fx_name,
    shown_fx_unit_value, CompartmentKind, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, FxDisplayType, HitResponse, MappingControlContext, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetSection, TargetTypeDef, TrackDescriptor,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, Target, UnitValue,
};
use reaper_high::{ChangeEvent, Fx, FxChain, Project, Track};
use reaper_medium::FxChainVisibility;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedBrowseFxsTarget {
    pub track_descriptor: TrackDescriptor,
    pub is_input_fx: bool,
    pub display_type: FxDisplayType,
}

impl UnresolvedReaperTargetDef for UnresolvedBrowseFxsTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let fx_chains = get_fx_chains(
            context,
            &self.track_descriptor.track,
            self.is_input_fx,
            compartment,
        )?;
        let targets = fx_chains
            .into_iter()
            .map(|fx_chain| {
                ReaperTarget::BrowseFxs(BrowseFxsTarget {
                    fx_chain,
                    display_type: self.display_type,
                })
            })
            .collect();
        Ok(targets)
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        Some(&self.track_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowseFxsTarget {
    pub fx_chain: FxChain,
    pub display_type: FxDisplayType,
}

impl RealearnTarget for BrowseFxsTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        // `+ 1` because "<No FX>" is also a possible value.
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size: convert_count_to_step_size(self.fx_chain.fx_count() + 1),
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
        let value = convert_unit_value_to_fx_index(&self.fx_chain, input)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        match convert_unit_value_to_fx_index(&self.fx_chain, value) {
            None => "<No FX>".to_string(),
            Some(i) => (i + 1).to_string(),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let fx_index = match value.to_absolute_value()? {
            AbsoluteValue::Continuous(v) => convert_unit_value_to_fx_index(&self.fx_chain, v),
            AbsoluteValue::Discrete(f) => {
                if f.actual() == 0 {
                    None
                } else {
                    Some(f.actual() - 1)
                }
            }
        };
        use FxDisplayType::*;
        match fx_index {
            None => match self.display_type {
                FloatingWindow => {
                    self.fx_chain.hide_all_floating_windows();
                }
                Chain => {
                    self.fx_chain.hide()?;
                }
            },
            Some(fx_index) => match self.display_type {
                FloatingWindow => {
                    for (i, fx) in self.fx_chain.index_based_fxs().enumerate() {
                        if i == fx_index as usize {
                            fx.show_in_floating_window()?;
                        } else {
                            fx.hide_floating_window()?;
                        }
                    }
                }
                Chain => {
                    let fx = self
                        .fx_chain
                        .index_based_fx_by_index(fx_index)
                        .ok_or("FX not available")?;
                    fx.show_in_chain()?;
                }
            },
        }
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.fx_chain.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.fx_chain.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx_chain.track()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        use CompoundChangeEvent::*;
        match evt {
            Reaper(ChangeEvent::FxOpened(e)) if e.fx.chain() == &self.fx_chain => (true, None),
            Reaper(ChangeEvent::FxClosed(e)) if e.fx.chain() == &self.fx_chain => (true, None),
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        _: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        Ok(shown_fx_unit_value(&self.fx_chain, index))
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(get_fx_name(&self.current_fx()?).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let index = self.current_fx_index()?;
        Some(NumericValue::Discrete(index as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::BrowseFxs)
    }
}

impl BrowseFxsTarget {
    fn current_fx(&self) -> Option<Fx> {
        use FxDisplayType::*;
        match self.display_type {
            FloatingWindow => self
                .fx_chain
                .index_based_fxs()
                .find(|fx| fx.floating_window().is_some()),
            Chain => {
                use FxChainVisibility::*;
                match self.fx_chain.visibility() {
                    Hidden | Visible(None) | Unknown(_) => None,
                    Visible(Some(i)) => Some(self.fx_chain.fx_by_index_untracked(i)),
                }
            }
        }
    }

    fn current_fx_index(&self) -> Option<u32> {
        use FxDisplayType::*;
        match self.display_type {
            FloatingWindow => self
                .fx_chain
                .index_based_fxs()
                .position(|fx| fx.floating_window().is_some())
                .map(|i| i as u32),
            Chain => {
                use FxChainVisibility::*;
                match self.fx_chain.visibility() {
                    Hidden | Visible(None) | Unknown(_) => None,
                    Visible(Some(i)) => Some(i),
                }
            }
        }
    }
}

impl<'a> Target<'a> for BrowseFxsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let fx_count = self.fx_chain.fx_count();
        // Because we count "<No FX>" as a possible value, this is equal.
        let max_value = fx_count;
        let fx_index = self.current_fx_index();
        let actual_value = fx_index.map(|i| i + 1).unwrap_or(0);
        Some(AbsoluteValue::Discrete(Fraction::new(
            actual_value,
            max_value,
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const BROWSE_FXS_TARGET: TargetTypeDef = TargetTypeDef {
    section: TargetSection::FxChain,
    name: "Browse FXs",
    short_name: "Browse FXs",
    supports_track: true,
    supports_fx_chain: true,
    supports_fx_display_type: true,
    ..DEFAULT_TARGET
};
