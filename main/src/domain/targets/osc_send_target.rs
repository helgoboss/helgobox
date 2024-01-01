use crate::domain::ui_util::{format_osc_message, log_target_output};
use crate::domain::{
    Compartment, ControlContext, ExtendedProcessorContext, FeedbackOutput, HitResponse,
    MappingControlContext, OscDeviceId, OscFeedbackTask, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use base::NamedChannelSender;
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, FeedbackValue, NumericFeedbackValue,
    OscArgDescriptor, OscTypeTag, Target,
};
use rosc::OscMessage;

#[derive(Debug)]
pub struct UnresolvedOscSendTarget {
    pub address_pattern: String,
    pub arg_descriptor: Option<OscArgDescriptor>,
    pub device_id: Option<OscDeviceId>,
}

impl UnresolvedReaperTargetDef for UnresolvedOscSendTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::SendOsc(OscSendTarget::new(
            self.address_pattern.clone(),
            self.arg_descriptor,
            self.device_id,
        ))])
    }

    fn can_be_affected_by_change_events(&self) -> bool {
        // We don't want to be refreshed because we maintain an artificial value.
        false
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OscSendTarget {
    address_pattern: String,
    arg_descriptor: Option<OscArgDescriptor>,
    device_id: Option<OscDeviceId>,
    // For making basic toggle/relative control possible.
    artificial_value: AbsoluteValue,
}

impl OscSendTarget {
    pub fn new(
        address_pattern: String,
        arg_descriptor: Option<OscArgDescriptor>,
        device_id: Option<OscDeviceId>,
    ) -> Self {
        Self {
            address_pattern,
            arg_descriptor,
            device_id,
            artificial_value: Default::default(),
        }
    }

    pub fn address_pattern(&self) -> &str {
        &self.address_pattern
    }

    pub fn arg_descriptor(&self) -> Option<OscArgDescriptor> {
        self.arg_descriptor
    }

    pub fn device_id(&self) -> Option<OscDeviceId> {
        self.device_id
    }
}

impl RealearnTarget for OscSendTarget {
    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::SendOsc)
    }

    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        if let Some(desc) = self.arg_descriptor {
            use OscTypeTag::*;
            match desc.type_tag() {
                Float | Double => (
                    ControlType::AbsoluteContinuousRetriggerable,
                    TargetCharacter::Continuous,
                ),
                Bool => (
                    ControlType::AbsoluteContinuousRetriggerable,
                    TargetCharacter::Switch,
                ),
                Nil | Inf => (
                    ControlType::AbsoluteContinuousRetriggerable,
                    TargetCharacter::Trigger,
                ),
                _ => (
                    ControlType::AbsoluteContinuousRetriggerable,
                    TargetCharacter::Trigger,
                ),
            }
        } else {
            (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Trigger,
            )
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_unit_value()?;
        let msg = OscMessage {
            addr: self.address_pattern.clone(),
            args: if let Some(desc) = self.arg_descriptor {
                let fb_value =
                    NumericFeedbackValue::new(Default::default(), AbsoluteValue::Continuous(value));
                desc.to_concrete_args(FeedbackValue::Numeric(fb_value))
                    .ok_or("sending of this OSC type not supported")?
            } else {
                vec![]
            },
        };
        let effective_dev_id = self
            .device_id
            .or_else(|| {
                if let FeedbackOutput::Osc(dev_id) = context.control_context.feedback_output? {
                    Some(dev_id)
                } else {
                    None
                }
            })
            .ok_or("no destination device for sending OSC")?;
        if context.control_context.output_logging_enabled {
            let text = format!(
                "Device {} | {}",
                effective_dev_id.fmt_short(),
                format_osc_message(&msg)
            );
            log_target_output(context.control_context.unit_id, text);
        }
        context
            .control_context
            .osc_feedback_task_sender
            .send_complaining(OscFeedbackTask::new(effective_dev_id, msg));
        self.artificial_value = AbsoluteValue::Continuous(value);
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        true
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for OscSendTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        Some(self.artificial_value)
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const OSC_SEND_TARGET: TargetTypeDef = TargetTypeDef {
    name: "OSC: Send message",
    short_name: "Send OSC",
    supports_feedback: false,
    ..DEFAULT_TARGET
};
