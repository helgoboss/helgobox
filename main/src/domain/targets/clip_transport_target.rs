use crate::domain::clip::{ClipChangedEvent, SlotPlayOptions, SlotStopBehavior};
use crate::domain::{
    clip_play_state_unit_value, format_value_as_on_off, get_effective_tracks,
    transport_is_enabled_unit_value, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    HitInstructionReturnValue, InstanceStateChanged, MappingCompartment, MappingControlContext,
    RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef,
    TrackDescriptor, TransportAction, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Project, Track};

#[derive(Debug)]
pub struct UnresolvedClipTransportTarget {
    pub track_descriptor: Option<TrackDescriptor>,
    pub slot_index: usize,
    pub action: TransportAction,
    pub play_options: SlotPlayOptions,
}

impl UnresolvedReaperTargetDef for UnresolvedClipTransportTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context.project_or_current_project();
        let targets = if let Some(desc) = self.track_descriptor.as_ref() {
            get_effective_tracks(context, &desc.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::ClipTransport(ClipTransportTarget {
                        project,
                        track: Some(track),
                        slot_index: self.slot_index,
                        action: self.action,
                        play_options: self.play_options,
                    })
                })
                .collect()
        } else {
            vec![ReaperTarget::ClipTransport(ClipTransportTarget {
                project,
                track: None,
                slot_index: self.slot_index,
                action: self.action,
                play_options: self.play_options,
            })]
        };
        Ok(targets)
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        self.track_descriptor.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipTransportTarget {
    pub project: Project,
    pub track: Option<Track>,
    pub slot_index: usize,
    pub action: TransportAction,
    pub play_options: SlotPlayOptions,
}

impl ClipTransportTarget {
    fn stop_behavior(&self) -> SlotStopBehavior {
        use SlotStopBehavior::*;
        if self.play_options.next_bar {
            EndOfClip
        } else {
            Immediately
        }
    }
}

impl RealearnTarget for ClipTransportTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
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
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        use TransportAction::*;
        let on = !value.to_unit_value()?.is_zero();
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        match self.action {
            PlayStop => {
                if on {
                    instance_state.play(
                        self.project,
                        self.slot_index,
                        self.track.clone(),
                        self.play_options,
                    )?;
                } else {
                    instance_state.stop(self.slot_index, self.stop_behavior(), self.project)?;
                }
            }
            PlayPause => {
                if on {
                    instance_state.play(
                        self.project,
                        self.slot_index,
                        self.track.clone(),
                        self.play_options,
                    )?;
                } else {
                    instance_state.pause(self.slot_index)?;
                }
            }
            Stop => {
                if on {
                    instance_state.stop(self.slot_index, self.stop_behavior(), self.project)?;
                }
            }
            Pause => {
                if on {
                    instance_state.pause(self.slot_index)?;
                }
            }
            Record => {
                return Err("not supported at the moment");
            }
            Repeat => {
                instance_state.toggle_repeat(self.slot_index)?;
            }
        };
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        if let Some(t) = &self.track {
            if !t.is_available() {
                return false;
            }
        }
        true
    }

    fn project(&self) -> Option<Project> {
        self.track.as_ref().map(|t| t.project())
    }

    fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Reaper(ChangeEvent::PlayStateChanged(e)) => {
                // Feedback handled from instance-scoped feedback events.
                let mut instance_state = context.instance_state.borrow_mut();
                instance_state.process_transport_change(e.new_value);
                (false, None)
            }
            CompoundChangeEvent::Instance(InstanceStateChanged::Clip {
                slot_index: si,
                event,
            }) if *si == self.slot_index => {
                use TransportAction::*;
                match self.action {
                    PlayStop | PlayPause | Stop | Pause => match event {
                        ClipChangedEvent::PlayState(new_state) => (
                            true,
                            Some(AbsoluteValue::Continuous(clip_play_state_unit_value(
                                self.action,
                                *new_state,
                            ))),
                        ),
                        _ => (false, None),
                    },
                    // Not supported at the moment.
                    Record => (false, None),
                    Repeat => match event {
                        ClipChangedEvent::ClipRepeat(new_state) => (
                            true,
                            Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                                *new_state,
                            ))),
                        ),
                        _ => (false, None),
                    },
                }
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipTransport)
    }
}

impl<'a> Target<'a> for ClipTransportTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        use TransportAction::*;
        let val = match self.action {
            PlayStop | PlayPause | Stop | Pause => {
                let play_state = instance_state.get_slot(self.slot_index).ok()?.play_state();
                clip_play_state_unit_value(self.action, play_state)
            }
            Repeat => {
                let is_looped = instance_state
                    .get_slot(self.slot_index)
                    .ok()?
                    .repeat_is_enabled();
                transport_is_enabled_unit_value(is_looped)
            }
            Record => return None,
        };
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const CLIP_TRANSPORT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Invoke transport action",
    short_name: "Clip transport",
    hint: "Experimental target, record not supported",
    supports_track: true,
    supports_slot: true,
    ..DEFAULT_TARGET
};
