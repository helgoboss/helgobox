use crate::domain::{
    get_fx_name, percentage_for_fx_within_chain, ChangeInstanceFxArgs, Compartment, ControlContext,
    ExtendedProcessorContext, FxDescriptor, HitResponse, InstanceFxChangeRequest,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TagScope,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target};
use realearn_api::persistence::FxToolAction;
use reaper_high::{Fx, Project, Track};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedFxToolTarget {
    pub fx_descriptor: FxDescriptor,
    pub action: FxToolAction,
    pub scope: TagScope,
}

impl UnresolvedReaperTargetDef for UnresolvedFxToolTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let fxs = self
            .fx_descriptor
            .resolve(context, compartment)
            .and_then(|fxs| {
                if fxs.is_empty() {
                    Err("resolved to zero FXs")
                } else {
                    Ok(fxs)
                }
            });
        let targets = match fxs {
            Ok(fxs) => fxs
                .into_iter()
                .map(|fx| {
                    ReaperTarget::FxTool(FxToolTarget {
                        fx: Some(fx),
                        action: self.action,
                        scope: self.scope.clone(),
                    })
                })
                .collect(),
            Err(e) => {
                if self.action == FxToolAction::SetAsInstanceFx {
                    // If we just want to *set* the (unresolved) FX as instance FX, we
                    // don't need a resolved target.
                    let target = ReaperTarget::FxTool(FxToolTarget {
                        fx: None,
                        action: self.action,
                        scope: self.scope.clone(),
                    });
                    vec![target]
                } else {
                    // Otherwise we should classify the target as inactive.
                    return Err(e);
                }
            }
        };
        Ok(targets)
    }

    fn fx_descriptor(&self) -> Option<&FxDescriptor> {
        Some(&self.fx_descriptor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxToolTarget {
    pub fx: Option<Fx>,
    pub action: FxToolAction,
    pub scope: TagScope,
}

impl RealearnTarget for FxToolTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn is_available(&self, _: ControlContext) -> bool {
        match &self.fx {
            None => false,
            Some(fx) => fx.is_available(),
        }
    }

    fn project(&self) -> Option<Project> {
        self.fx.as_ref()?.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx.as_ref()?.track()
    }

    fn fx(&self) -> Option<&Fx> {
        self.fx.as_ref()
    }

    fn text_value(&self, _: ControlContext) -> Option<Cow<'static, str>> {
        Some(get_fx_name(self.fx.as_ref()?).into())
    }

    fn numeric_value(&self, _: ControlContext) -> Option<NumericValue> {
        let position = self.fx.as_ref()?.index() + 1;
        Some(NumericValue::Discrete(position as _))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxTool)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        if !value.is_on() {
            return Ok(HitResponse::ignored());
        }
        let request = match self.action {
            FxToolAction::DoNothing => return Ok(HitResponse::ignored()),
            FxToolAction::SetAsInstanceFx => {
                InstanceFxChangeRequest::SetFromMapping(context.mapping_data.qualified_mapping_id())
            }
            FxToolAction::PinAsInstanceFx => {
                let fx = self.fx.as_ref().ok_or("FX could not be resolved")?;
                InstanceFxChangeRequest::Pin {
                    track_guid: fx.track().and_then(|t| {
                        if t.is_master_track() {
                            None
                        } else {
                            Some(*t.guid())
                        }
                    }),
                    is_input_fx: fx.is_input_fx(),
                    fx_guid: fx.get_or_query_guid()?,
                }
            }
        };
        let args = ChangeInstanceFxArgs {
            common: context
                .control_context
                .instance_container_common_args(&self.scope),
            request,
        };
        context
            .control_context
            .instance_container
            .change_instance_fx(args)?;
        Ok(HitResponse::processed_with_effect())
    }

    fn can_report_current_value(&self) -> bool {
        matches!(self.action, FxToolAction::DoNothing)
    }
}

impl<'a> Target<'a> for FxToolTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        match self.action {
            FxToolAction::DoNothing => {
                let fx = self.fx.as_ref()?;
                let fx_index = fx.index();
                percentage_for_fx_within_chain(fx.chain(), fx_index)
            }
            FxToolAction::SetAsInstanceFx | FxToolAction::PinAsInstanceFx => {
                // In future, we might support feedback here.
                None
            }
        }
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const FX_TOOL_TARGET: TargetTypeDef = TargetTypeDef {
    name: "FX",
    short_name: "FX",
    supports_fx: true,
    supports_track: true,
    supports_tags: true,
    ..DEFAULT_TARGET
};
