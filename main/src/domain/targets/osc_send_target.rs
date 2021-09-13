use crate::domain::ui_util::{format_osc_message, log_target_output};
use crate::domain::{
    ControlContext, FeedbackOutput, HitInstructionReturnValue, MappingControlContext, OscDeviceId,
    OscFeedbackTask, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, OscArgDescriptor, OscTypeTag, Target,
};
use rosc::OscMessage;

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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_unit_value()?;
        let msg = OscMessage {
            addr: self.address_pattern.clone(),
            args: if let Some(desc) = self.arg_descriptor {
                desc.to_concrete_args(value)
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
            log_target_output(context.control_context.instance_id, text);
        }
        context
            .control_context
            .osc_feedback_task_sender
            .try_send(OscFeedbackTask::new(effective_dev_id, msg))
            .unwrap();
        self.artificial_value = AbsoluteValue::Continuous(value);
        Ok(None)
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
