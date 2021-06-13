use crate::domain::ui_util::{format_osc_message, log_target_output};
use crate::domain::{
    ControlContext, FeedbackOutput, OscDeviceId, OscFeedbackTask, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, OscArgDescriptor, OscTypeTag, Target, UnitValue,
};
use rosc::OscMessage;

#[derive(Clone, Debug, PartialEq)]
pub struct OscSendTarget {
    pub address_pattern: String,
    pub arg_descriptor: Option<OscArgDescriptor>,
    pub device_id: Option<OscDeviceId>,
}

impl RealearnTarget for OscSendTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
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

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
        let msg = OscMessage {
            addr: self.address_pattern.clone(),
            args: if let Some(desc) = self.arg_descriptor {
                desc.to_concrete_args(value.to_unit_value()?)
                    .ok_or("sending of this OSC type not supported")?
            } else {
                vec![]
            },
        };
        let effective_dev_id = self
            .device_id
            .or_else(|| {
                if let FeedbackOutput::Osc(dev_id) = context.feedback_output? {
                    Some(dev_id)
                } else {
                    None
                }
            })
            .ok_or("no destination device for sending OSC")?;
        if context.output_logging_enabled {
            let text = format!(
                "Device {} | {}",
                effective_dev_id.fmt_short(),
                format_osc_message(&msg)
            );
            log_target_output(context.instance_id, text);
        }
        context
            .osc_feedback_task_sender
            .try_send(OscFeedbackTask::new(effective_dev_id, msg))
            .unwrap();
        Ok(())
    }

    fn can_report_current_value(&self) -> bool {
        false
    }

    fn is_available(&self) -> bool {
        true
    }

    fn supports_automatic_feedback(&self) -> bool {
        false
    }
}

impl<'a> Target<'a> for OscSendTarget {
    type Context = ();

    fn current_value(&self, _context: ()) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
