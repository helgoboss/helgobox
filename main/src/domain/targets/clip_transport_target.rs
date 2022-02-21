use crate::domain::{
    clip_play_state_unit_value, format_value_as_on_off, get_effective_tracks,
    transport_is_enabled_unit_value, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    HitInstructionReturnValue, InstanceStateChanged, MappingCompartment, MappingControlContext,
    RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, TrackDescriptor, TransportAction, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use playtime_clip_engine::{
    clip_timeline, ClipChangedEvent, ClipRecordTiming, RecordArgs, RecordKind, SlotPlayOptions,
    SlotStopBehavior, Timeline,
};
use reaper_high::{Project, Track};

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
        self.action.control_type_and_character()
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
        let clip_matrix = instance_state.clip_matrix_mut();
        match self.action {
            PlayStop => {
                if on {
                    clip_matrix.play_clip_legacy(
                        self.project,
                        self.slot_index,
                        self.track.clone(),
                        self.play_options,
                    )?;
                } else {
                    clip_matrix.stop_clip_legacy(
                        self.slot_index,
                        self.stop_behavior(),
                        self.project,
                    )?;
                }
            }
            PlayPause => {
                if on {
                    clip_matrix.play_clip_legacy(
                        self.project,
                        self.slot_index,
                        self.track.clone(),
                        self.play_options,
                    )?;
                } else {
                    clip_matrix.pause_clip_legacy(self.slot_index)?;
                }
            }
            Stop => {
                if on {
                    clip_matrix.stop_clip_legacy(
                        self.slot_index,
                        self.stop_behavior(),
                        self.project,
                    )?;
                }
            }
            Pause => {
                if on {
                    clip_matrix.pause_clip_legacy(self.slot_index);
                }
            }
            RecordStop => {
                if on {
                    let timing = if true {
                        let timeline = clip_timeline(Some(self.project), false);
                        let next_bar = timeline.next_bar_at(timeline.cursor_pos());
                        ClipRecordTiming::StartOnBarStopOnDemand {
                            start_bar: next_bar,
                        }
                    } else {
                        ClipRecordTiming::StartImmediatelyStopOnDemand
                    };
                    clip_matrix.record_clip_legacy(
                        self.slot_index,
                        self.project,
                        RecordArgs {
                            kind: RecordKind::Normal {
                                play_after: true,
                                timing,
                                detect_downbeat: true,
                            },
                        },
                    );
                } else {
                    clip_matrix.stop_clip_legacy(
                        self.slot_index,
                        SlotStopBehavior::EndOfClip,
                        self.project,
                    );
                }
            }
            Repeat => {
                clip_matrix.toggle_repeat_legacy(self.slot_index)?;
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
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::Clip {
                slot_index: si,
                event,
            }) if *si == self.slot_index => {
                use TransportAction::*;
                match self.action {
                    PlayStop | PlayPause | Stop | Pause | RecordStop => match event {
                        ClipChangedEvent::PlayState(new_state) => {
                            let uv = clip_play_state_unit_value(self.action, *new_state);
                            (true, Some(AbsoluteValue::Continuous(uv)))
                        }
                        _ => (false, None),
                    },
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

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        Some(RealTimeReaperTarget::ClipTransport(self.clone()))
    }
}

impl<'a> Target<'a> for ClipTransportTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        use TransportAction::*;
        let val = match self.action {
            PlayStop | PlayPause | Stop | Pause | RecordStop => {
                let play_state = instance_state
                    .clip_matrix()
                    .clip_play_state(self.slot_index)?;
                clip_play_state_unit_value(self.action, play_state)
            }
            Repeat => {
                let is_looped = instance_state
                    .clip_matrix()
                    .clip_repeated(self.slot_index)?;
                transport_is_enabled_unit_value(is_looped)
            }
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
