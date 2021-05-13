use crate::core::default_util::is_default;
use crate::core::{prop, Prop};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{ControlType, OscArgDescriptor, OscTypeTag, Target};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, BookmarkType, Fx, FxParameter, Guid, Project, Track, TrackRoute, TrackRoutePartner,
};

use rxrust::prelude::*;
use serde::{Deserialize, Serialize};

use crate::application::VirtualControlElementType;
use crate::domain::{
    find_bookmark, get_fx, get_fx_param, get_non_present_virtual_route_label, get_track_route,
    ActionInvocationType, CompoundMappingTarget, ExpressionEvaluator, ExtendedProcessorContext,
    FxDescriptor, FxDisplayType, FxParameterDescriptor, MappingCompartment, OscDeviceId,
    PlayPosFeedbackResolution, ProcessorContext, RealearnTarget, ReaperTarget, SeekOptions,
    SendMidiDestination, SlotPlayOptions, SoloBehavior, TouchedParameterType, TrackDescriptor,
    TrackExclusivity, TrackRouteDescriptor, TrackRouteSelector, TrackRouteType, TransportAction,
    UnresolvedCompoundMappingTarget, UnresolvedReaperTarget, VirtualChainFx, VirtualControlElement,
    VirtualControlElementId, VirtualFx, VirtualFxParameter, VirtualTarget, VirtualTrack,
    VirtualTrackRoute,
};
use serde_repr::*;
use std::borrow::Cow;

use reaper_medium::{
    AutomationMode, BookmarkId, GlobalAutomationModeOverride, TrackArea, TrackSendDirection,
};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use wildmatch::WildMatch;

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel {
    // # For all targets
    pub category: Prop<TargetCategory>,
    // # For virtual targets
    pub control_element_type: Prop<VirtualControlElementType>,
    pub control_element_id: Prop<VirtualControlElementId>,
    // # For REAPER targets
    // TODO-low Rename this to reaper_target_type
    pub r#type: Prop<ReaperTargetType>,
    // # For action targets only
    // TODO-low Maybe replace Action with just command ID and/or command name
    pub action: Prop<Option<Action>>,
    pub action_invocation_type: Prop<ActionInvocationType>,
    // # For track targets
    pub track_type: Prop<VirtualTrackType>,
    pub track_id: Prop<Option<Guid>>,
    pub track_name: Prop<String>,
    pub track_index: Prop<u32>,
    pub track_expression: Prop<String>,
    pub enable_only_if_track_selected: Prop<bool>,
    // # For track FX targets
    pub fx_type: Prop<VirtualFxType>,
    pub fx_is_input_fx: Prop<bool>,
    pub fx_id: Prop<Option<Guid>>,
    pub fx_name: Prop<String>,
    pub fx_index: Prop<u32>,
    pub fx_expression: Prop<String>,
    pub enable_only_if_fx_has_focus: Prop<bool>,
    // # For track FX parameter targets
    pub param_type: Prop<VirtualFxParameterType>,
    pub param_index: Prop<u32>,
    pub param_name: Prop<String>,
    pub param_expression: Prop<String>,
    // # For track route targets
    pub route_selector_type: Prop<TrackRouteSelectorType>,
    pub route_type: Prop<TrackRouteType>,
    pub route_id: Prop<Option<Guid>>,
    pub route_index: Prop<u32>,
    pub route_name: Prop<String>,
    pub route_expression: Prop<String>,
    // # For track solo targets
    pub solo_behavior: Prop<SoloBehavior>,
    // # For toggleable track targets
    pub track_exclusivity: Prop<TrackExclusivity>,
    // # For transport target
    pub transport_action: Prop<TransportAction>,
    // # For "Load FX snapshot" target
    pub fx_snapshot: Prop<Option<FxSnapshot>>,
    // # For "Automation touch state" target
    pub touched_parameter_type: Prop<TouchedParameterType>,
    // # For "Go to marker/region" target
    pub bookmark_ref: Prop<u32>,
    pub bookmark_type: Prop<BookmarkType>,
    pub bookmark_anchor_type: Prop<BookmarkAnchorType>,
    // # For "Go to marker/region" target and "Seek" target
    pub use_time_selection: Prop<bool>,
    pub use_loop_points: Prop<bool>,
    // # For "Seek" target
    pub use_regions: Prop<bool>,
    pub use_project: Prop<bool>,
    pub move_view: Prop<bool>,
    pub seek_play: Prop<bool>,
    pub feedback_resolution: Prop<PlayPosFeedbackResolution>,
    // # For track show target
    pub track_area: Prop<RealearnTrackArea>,
    // # For track automation mode target
    pub track_automation_mode: Prop<RealearnAutomationMode>,
    // # For automation mode override target
    pub automation_mode_override_type: Prop<AutomationModeOverrideType>,
    // # For FX Open and FX Navigate target
    pub fx_display_type: Prop<FxDisplayType>,
    // # For track selection related targets
    pub scroll_arrange_view: Prop<bool>,
    pub scroll_mixer: Prop<bool>,
    // # For Send MIDI target
    pub raw_midi_pattern: Prop<String>,
    pub send_midi_destination: Prop<SendMidiDestination>,
    // # For Send OSC target
    pub osc_address_pattern: Prop<String>,
    pub osc_arg_index: Prop<Option<u32>>,
    pub osc_arg_type_tag: Prop<OscTypeTag>,
    pub osc_dev_id: Prop<Option<OscDeviceId>>,
    // # For clip targets
    pub slot_index: Prop<usize>,
    pub next_bar: Prop<bool>,
    pub buffered: Prop<bool>,
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            category: prop(TargetCategory::default()),
            control_element_type: prop(VirtualControlElementType::default()),
            control_element_id: prop(Default::default()),
            r#type: prop(ReaperTargetType::FxParameter),
            action: prop(None),
            action_invocation_type: prop(ActionInvocationType::default()),
            track_type: prop(Default::default()),
            track_id: prop(None),
            track_name: prop("".to_owned()),
            track_index: prop(0),
            track_expression: prop("".to_owned()),
            enable_only_if_track_selected: prop(false),
            fx_type: prop(Default::default()),
            fx_is_input_fx: prop(false),
            fx_id: prop(None),
            fx_name: prop("".to_owned()),
            fx_index: prop(0),
            fx_expression: prop("".to_owned()),
            enable_only_if_fx_has_focus: prop(false),
            param_type: prop(Default::default()),
            param_index: prop(0),
            param_name: prop("".to_owned()),
            param_expression: prop("".to_owned()),
            route_selector_type: prop(Default::default()),
            route_type: prop(Default::default()),
            route_id: prop(None),
            route_index: prop(0),
            route_name: prop(Default::default()),
            route_expression: prop(Default::default()),
            solo_behavior: prop(Default::default()),
            track_exclusivity: prop(Default::default()),
            transport_action: prop(TransportAction::default()),
            fx_snapshot: prop(None),
            touched_parameter_type: prop(Default::default()),
            bookmark_ref: prop(0),
            bookmark_type: prop(BookmarkType::Marker),
            bookmark_anchor_type: prop(Default::default()),
            use_time_selection: prop(false),
            use_loop_points: prop(false),
            use_regions: prop(false),
            use_project: prop(true),
            move_view: prop(true),
            seek_play: prop(true),
            feedback_resolution: prop(Default::default()),
            track_area: prop(Default::default()),
            track_automation_mode: prop(Default::default()),
            automation_mode_override_type: prop(Default::default()),
            fx_display_type: prop(Default::default()),
            scroll_arrange_view: prop(false),
            scroll_mixer: prop(false),
            raw_midi_pattern: prop(Default::default()),
            send_midi_destination: prop(Default::default()),
            osc_address_pattern: prop("".to_owned()),
            osc_arg_index: prop(Some(0)),
            osc_arg_type_tag: prop(Default::default()),
            osc_dev_id: prop(None),
            slot_index: prop(0),
            next_bar: prop(false),
            buffered: prop(false),
        }
    }
}

impl TargetModel {
    pub fn supports_control(&self) -> bool {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => self.r#type.get().supports_control(),
            Virtual => true,
        }
    }

    pub fn supports_feedback(&self) -> bool {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => self.r#type.get().supports_feedback(),
            Virtual => true,
        }
    }

    pub fn take_fx_snapshot(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<FxSnapshot, &'static str> {
        let fx = self.with_context(context, compartment).fx()?;
        let fx_info = fx.info()?;
        let fx_snapshot = FxSnapshot {
            fx_type: if fx_info.sub_type_expression.is_empty() {
                fx_info.type_expression
            } else {
                fx_info.sub_type_expression
            },
            fx_name: fx_info.effect_name,
            preset_name: fx.preset_name().map(|n| n.into_string()),
            chunk: Rc::new(fx.tag_chunk()?.content().to_owned()),
        };
        Ok(fx_snapshot)
    }

    pub fn invalidate_fx_index(
        &mut self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) {
        if !self.supports_fx() {
            return;
        }
        if let Ok(actual_fx) = self.with_context(context, compartment).fx() {
            let new_virtual_fx = match self.virtual_fx() {
                Some(virtual_fx) => {
                    match virtual_fx {
                        VirtualFx::ChainFx {
                            is_input_fx,
                            chain_fx: anchor,
                        } => match anchor {
                            VirtualChainFx::ByIdOrIndex(guid, _) => Some(VirtualFx::ChainFx {
                                is_input_fx,
                                chain_fx: VirtualChainFx::ByIdOrIndex(guid, actual_fx.index()),
                            }),
                            _ => None,
                        },
                        // No update necessary
                        VirtualFx::Focused | VirtualFx::This => None,
                    }
                }
                // Shouldn't happen
                None => None,
            };
            if let Some(virtual_fx) = new_virtual_fx {
                self.set_virtual_fx(virtual_fx);
            }
        }
    }

    pub fn set_virtual_track(&mut self, track: VirtualTrack) {
        self.set_track(TrackPropValues::from_virtual_track(track), true);
    }

    pub fn set_track(&mut self, track: TrackPropValues, with_notification: bool) {
        self.track_type
            .set_with_optional_notification(track.r#type, with_notification);
        self.track_id
            .set_with_optional_notification(track.id, with_notification);
        self.track_name
            .set_with_optional_notification(track.name, with_notification);
        self.track_index
            .set_with_optional_notification(track.index, with_notification);
        self.track_expression
            .set_with_optional_notification(track.expression, with_notification);
    }

    pub fn set_virtual_route(&mut self, route: VirtualTrackRoute) {
        self.set_route(TrackRoutePropValues::from_virtual_route(route), true);
    }

    pub fn set_route(&mut self, route: TrackRoutePropValues, with_notification: bool) {
        self.route_selector_type
            .set_with_optional_notification(route.selector_type, with_notification);
        self.route_type
            .set_with_optional_notification(route.r#type, with_notification);
        self.route_id
            .set_with_optional_notification(route.id, with_notification);
        self.route_name
            .set_with_optional_notification(route.name, with_notification);
        self.route_index
            .set_with_optional_notification(route.index, with_notification);
        self.route_expression
            .set_with_optional_notification(route.expression, with_notification);
    }

    pub fn set_virtual_fx(&mut self, fx: VirtualFx) {
        self.set_fx(FxPropValues::from_virtual_fx(fx), true);
    }

    pub fn set_fx(&mut self, fx: FxPropValues, with_notification: bool) {
        self.fx_type
            .set_with_optional_notification(fx.r#type, with_notification);
        self.fx_is_input_fx
            .set_with_optional_notification(fx.is_input_fx, with_notification);
        self.fx_id
            .set_with_optional_notification(fx.id, with_notification);
        self.fx_name
            .set_with_optional_notification(fx.name, with_notification);
        self.fx_index
            .set_with_optional_notification(fx.index, with_notification);
        self.fx_expression
            .set_with_optional_notification(fx.expression, with_notification);
    }

    pub fn set_fx_parameter(&mut self, param: FxParameterPropValues, with_notification: bool) {
        self.param_type
            .set_with_optional_notification(param.r#type, with_notification);
        self.param_name
            .set_with_optional_notification(param.name, with_notification);
        self.param_index
            .set_with_optional_notification(param.index, with_notification);
        self.param_expression
            .set_with_optional_notification(param.expression, with_notification);
    }

    pub fn set_seek_options(&mut self, options: SeekOptions, with_notification: bool) {
        self.use_time_selection
            .set_with_optional_notification(options.use_time_selection, with_notification);
        self.use_loop_points
            .set_with_optional_notification(options.use_loop_points, with_notification);
        self.use_regions
            .set_with_optional_notification(options.use_regions, with_notification);
        self.use_project
            .set_with_optional_notification(options.use_project, with_notification);
        self.move_view
            .set_with_optional_notification(options.move_view, with_notification);
        self.seek_play
            .set_with_optional_notification(options.seek_play, with_notification);
        self.feedback_resolution
            .set_with_optional_notification(options.feedback_resolution, with_notification);
    }

    pub fn seek_options(&self) -> SeekOptions {
        SeekOptions {
            use_time_selection: self.use_time_selection.get(),
            use_loop_points: self.use_loop_points.get(),
            use_regions: self.use_regions.get(),
            use_project: self.use_project.get(),
            move_view: self.move_view.get(),
            seek_play: self.seek_play.get(),
            feedback_resolution: self.feedback_resolution.get(),
        }
    }

    pub fn apply_from_target(&mut self, target: &ReaperTarget, context: &ProcessorContext) {
        use ReaperTarget::*;
        self.category.set(TargetCategory::Reaper);
        self.r#type.set(ReaperTargetType::from_target(target));
        if let Some(actual_fx) = target.fx() {
            let virtual_fx = virtualize_fx(actual_fx, context);
            self.set_virtual_fx(virtual_fx);
            let track = if let Some(track) = actual_fx.track() {
                track.clone()
            } else {
                // Must be monitoring FX. In this case we want the master track (it's REAPER's
                // convention and ours).
                context.project_or_current_project().master_track()
            };
            self.set_virtual_track(virtualize_track(track, context));
        } else if let Some(track) = target.track() {
            self.set_virtual_track(virtualize_track(track.clone(), context));
        }
        if let Some(send) = target.route() {
            let virtual_route = virtualize_route(send, context);
            self.set_virtual_route(virtual_route);
        }
        if let Some(track_exclusivity) = target.track_exclusivity() {
            self.track_exclusivity.set(track_exclusivity);
        }
        match target {
            Action(t) => {
                self.action.set(Some(t.action.clone()));
                self.action_invocation_type.set(t.invocation_type);
            }
            FxParameter(t) => {
                self.param_type.set(VirtualFxParameterType::ByIndex);
                self.param_index.set(t.param.index());
            }
            Transport(t) => {
                self.transport_action.set(t.action);
            }
            AutomationTouchState(t) => {
                self.touched_parameter_type.set(t.parameter_type);
            }
            TrackSolo(t) => {
                self.solo_behavior.set(t.behavior);
            }
            GoToBookmark(t) => {
                self.bookmark_ref.set(t.index);
                self.bookmark_type.set(t.bookmark_type);
            }
            TrackAutomationMode(t) => {
                self.track_automation_mode
                    .set(RealearnAutomationMode::from_reaper(t.mode));
            }
            AutomationModeOverride(t) => match t.mode_override {
                None => {
                    self.automation_mode_override_type
                        .set(AutomationModeOverrideType::None);
                }
                Some(GlobalAutomationModeOverride::Bypass) => {
                    self.automation_mode_override_type
                        .set(AutomationModeOverrideType::Bypass);
                }
                Some(GlobalAutomationModeOverride::Mode(am)) => {
                    self.automation_mode_override_type
                        .set(AutomationModeOverrideType::Override);
                    self.track_automation_mode
                        .set(RealearnAutomationMode::from_reaper(am));
                }
            },
            SendMidi { .. }
            | SendOsc { .. }
            | TrackVolume { .. }
            | TrackPeak { .. }
            | TrackRouteVolume { .. }
            | TrackPan { .. }
            | TrackWidth { .. }
            | TrackArm { .. }
            | TrackSelection { .. }
            | TrackMute { .. }
            | TrackShow { .. }
            | TrackRoutePan { .. }
            | TrackRouteMute { .. }
            | Tempo { .. }
            | Playrate { .. }
            | FxEnable { .. }
            | FxOpen { .. }
            | FxNavigate { .. }
            | FxPreset { .. }
            | SelectedTrack { .. }
            | AllTrackFxEnable { .. }
            | LoadFxSnapshot { .. }
            | ClipTransport { .. }
            | ClipSeek { .. }
            | ClipVolume { .. }
            | Seek { .. } => {}
        };
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.category
            .changed()
            .merge(self.r#type.changed())
            .merge(self.action.changed())
            .merge(self.action_invocation_type.changed())
            .merge(self.track_type.changed())
            .merge(self.track_id.changed())
            .merge(self.track_name.changed())
            .merge(self.track_index.changed())
            .merge(self.track_expression.changed())
            .merge(self.enable_only_if_track_selected.changed())
            .merge(self.fx_type.changed())
            .merge(self.fx_id.changed())
            .merge(self.fx_name.changed())
            .merge(self.fx_index.changed())
            .merge(self.fx_expression.changed())
            .merge(self.fx_is_input_fx.changed())
            .merge(self.enable_only_if_fx_has_focus.changed())
            .merge(self.param_type.changed())
            .merge(self.param_index.changed())
            .merge(self.param_name.changed())
            .merge(self.param_expression.changed())
            .merge(self.route_selector_type.changed())
            .merge(self.route_type.changed())
            .merge(self.route_id.changed())
            .merge(self.route_index.changed())
            .merge(self.route_name.changed())
            .merge(self.route_expression.changed())
            .merge(self.solo_behavior.changed())
            .merge(self.track_exclusivity.changed())
            .merge(self.transport_action.changed())
            .merge(self.control_element_type.changed())
            .merge(self.control_element_id.changed())
            .merge(self.fx_snapshot.changed())
            .merge(self.touched_parameter_type.changed())
            .merge(self.bookmark_ref.changed())
            .merge(self.bookmark_type.changed())
            .merge(self.bookmark_anchor_type.changed())
            .merge(self.use_time_selection.changed())
            .merge(self.use_loop_points.changed())
            .merge(self.use_regions.changed())
            .merge(self.use_project.changed())
            .merge(self.move_view.changed())
            .merge(self.seek_play.changed())
            .merge(self.feedback_resolution.changed())
            .merge(self.track_area.changed())
            .merge(self.track_automation_mode.changed())
            .merge(self.automation_mode_override_type.changed())
            .merge(self.fx_display_type.changed())
            .merge(self.scroll_arrange_view.changed())
            .merge(self.scroll_mixer.changed())
            .merge(self.raw_midi_pattern.changed())
            .merge(self.send_midi_destination.changed())
            .merge(self.osc_address_pattern.changed())
            .merge(self.osc_arg_index.changed())
            .merge(self.osc_arg_type_tag.changed())
            .merge(self.osc_dev_id.changed())
            .merge(self.slot_index.changed())
            .merge(self.next_bar.changed())
            .merge(self.buffered.changed())
    }

    pub fn virtual_track(&self) -> Option<VirtualTrack> {
        use VirtualTrackType::*;
        let track = match self.track_type.get() {
            This => VirtualTrack::This,
            Selected => VirtualTrack::Selected {
                allow_multiple: false,
            },
            SelectedMultiple => VirtualTrack::Selected {
                allow_multiple: true,
            },
            Master => VirtualTrack::Master,
            ById => VirtualTrack::ById(self.track_id.get()?),
            ByName => VirtualTrack::ByName(WildMatch::new(self.track_name.get_ref())),
            ByIndex => VirtualTrack::ByIndex(self.track_index.get()),
            ByIdOrName => VirtualTrack::ByIdOrName(
                self.track_id.get()?,
                WildMatch::new(self.track_name.get_ref()),
            ),
            Dynamic => {
                let evaluator =
                    ExpressionEvaluator::compile(self.track_expression.get_ref()).ok()?;
                VirtualTrack::Dynamic(Box::new(evaluator))
            }
        };
        Some(track)
    }

    pub fn track(&self) -> TrackPropValues {
        TrackPropValues {
            r#type: self.track_type.get(),
            id: self.track_id.get(),
            name: self.track_name.get_ref().clone(),
            expression: self.track_expression.get_ref().clone(),
            index: self.track_index.get(),
        }
    }

    pub fn virtual_fx(&self) -> Option<VirtualFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type.get() {
            Focused => VirtualFx::Focused,
            This => VirtualFx::This,
            _ => VirtualFx::ChainFx {
                is_input_fx: self.fx_is_input_fx.get(),
                chain_fx: self.virtual_chain_fx()?,
            },
        };
        Some(fx)
    }

    pub fn track_route_selector(&self) -> Option<TrackRouteSelector> {
        use TrackRouteSelectorType::*;
        let selector = match self.route_selector_type.get() {
            Dynamic => {
                let evaluator =
                    ExpressionEvaluator::compile(self.route_expression.get_ref()).ok()?;
                TrackRouteSelector::Dynamic(Box::new(evaluator))
            }
            ById => {
                if self.route_type.get() == TrackRouteType::HardwareOutput {
                    // Hardware outputs don't offer stable IDs.
                    TrackRouteSelector::ByIndex(self.route_index.get())
                } else {
                    TrackRouteSelector::ById(self.route_id.get()?)
                }
            }
            ByName => TrackRouteSelector::ByName(WildMatch::new(self.route_name.get_ref())),
            ByIndex => TrackRouteSelector::ByIndex(self.route_index.get()),
        };
        Some(selector)
    }

    pub fn virtual_chain_fx(&self) -> Option<VirtualChainFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type.get() {
            Focused | This => return None,
            ById => VirtualChainFx::ById(self.fx_id.get()?, Some(self.fx_index.get())),
            ByName => VirtualChainFx::ByName(WildMatch::new(self.fx_name.get_ref())),
            ByIndex => VirtualChainFx::ByIndex(self.fx_index.get()),
            ByIdOrIndex => VirtualChainFx::ByIdOrIndex(self.fx_id.get(), self.fx_index.get()),
            Dynamic => {
                let evaluator = ExpressionEvaluator::compile(self.fx_expression.get_ref()).ok()?;
                VirtualChainFx::Dynamic(Box::new(evaluator))
            }
        };
        Some(fx)
    }

    pub fn fx(&self) -> FxPropValues {
        FxPropValues {
            r#type: self.fx_type.get(),
            is_input_fx: self.fx_is_input_fx.get(),
            id: self.fx_id.get(),
            name: self.fx_name.get_ref().clone(),
            expression: self.fx_expression.get_ref().clone(),
            index: self.fx_index.get(),
        }
    }

    pub fn track_route(&self) -> TrackRoutePropValues {
        TrackRoutePropValues {
            selector_type: self.route_selector_type.get(),
            r#type: self.route_type.get(),
            id: self.route_id.get(),
            name: self.route_name.get_ref().clone(),
            expression: self.route_expression.get_ref().clone(),
            index: self.route_index.get(),
        }
    }

    pub fn fx_parameter(&self) -> FxParameterPropValues {
        FxParameterPropValues {
            r#type: self.param_type.get(),
            name: self.param_name.get_ref().clone(),
            expression: self.param_expression.get_ref().clone(),
            index: self.param_index.get(),
        }
    }

    fn track_descriptor(&self) -> Result<TrackDescriptor, &'static str> {
        let desc = TrackDescriptor {
            track: self.virtual_track().ok_or("virtual track not complete")?,
            enable_only_if_track_selected: self.enable_only_if_track_selected.get(),
        };
        Ok(desc)
    }

    fn fx_descriptor(&self) -> Result<FxDescriptor, &'static str> {
        let desc = FxDescriptor {
            track_descriptor: self.track_descriptor()?,
            enable_only_if_fx_has_focus: self.enable_only_if_fx_has_focus.get(),
            fx: self.virtual_fx().ok_or("FX not set")?,
        };
        Ok(desc)
    }

    fn track_route_descriptor(&self) -> Result<TrackRouteDescriptor, &'static str> {
        let desc = TrackRouteDescriptor {
            track_descriptor: self.track_descriptor()?,
            route: self.virtual_track_route()?,
        };
        Ok(desc)
    }

    pub fn virtual_track_route(&self) -> Result<VirtualTrackRoute, &'static str> {
        let route = VirtualTrackRoute {
            r#type: self.route_type.get(),
            selector: self.track_route_selector().ok_or("track route not set")?,
        };
        Ok(route)
    }

    pub fn virtual_fx_parameter(&self) -> Option<VirtualFxParameter> {
        use VirtualFxParameterType::*;
        let param = match self.param_type.get() {
            ByName => VirtualFxParameter::ByName(WildMatch::new(self.param_name.get_ref())),
            ByIndex => VirtualFxParameter::ByIndex(self.param_index.get()),
            Dynamic => {
                let evaluator =
                    ExpressionEvaluator::compile(self.param_expression.get_ref()).ok()?;
                VirtualFxParameter::Dynamic(Box::new(evaluator))
            }
        };
        Some(param)
    }

    fn fx_parameter_descriptor(&self) -> Result<FxParameterDescriptor, &'static str> {
        let desc = FxParameterDescriptor {
            fx_descriptor: self.fx_descriptor()?,
            fx_parameter: self.virtual_fx_parameter().ok_or("FX parameter not set")?,
        };
        Ok(desc)
    }

    pub fn create_target(&self) -> Result<UnresolvedCompoundMappingTarget, &'static str> {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => {
                use ReaperTargetType::*;
                let target = match self.r#type.get() {
                    Action => UnresolvedReaperTarget::Action {
                        action: self.action()?,
                        invocation_type: self.action_invocation_type.get(),
                    },
                    FxParameter => UnresolvedReaperTarget::FxParameter {
                        fx_parameter_descriptor: self.fx_parameter_descriptor()?,
                    },
                    TrackVolume => UnresolvedReaperTarget::TrackVolume {
                        track_descriptor: self.track_descriptor()?,
                    },
                    TrackPeak => UnresolvedReaperTarget::TrackPeak {
                        track_descriptor: self.track_descriptor()?,
                    },
                    TrackSendVolume => UnresolvedReaperTarget::TrackSendVolume {
                        descriptor: self.track_route_descriptor()?,
                    },
                    TrackPan => UnresolvedReaperTarget::TrackPan {
                        track_descriptor: self.track_descriptor()?,
                    },
                    TrackWidth => UnresolvedReaperTarget::TrackWidth {
                        track_descriptor: self.track_descriptor()?,
                    },
                    TrackArm => UnresolvedReaperTarget::TrackArm {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                    },
                    TrackSelection => UnresolvedReaperTarget::TrackSelection {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                        scroll_arrange_view: self.scroll_arrange_view.get(),
                        scroll_mixer: self.scroll_mixer.get(),
                    },
                    TrackMute => UnresolvedReaperTarget::TrackMute {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                    },
                    TrackShow => UnresolvedReaperTarget::TrackShow {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                        area: match self.track_area.get() {
                            RealearnTrackArea::Tcp => TrackArea::Tcp,
                            RealearnTrackArea::Mcp => TrackArea::Mcp,
                        },
                    },
                    TrackAutomationMode => UnresolvedReaperTarget::TrackAutomationMode {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                        mode: self.track_automation_mode.get().to_reaper(),
                    },
                    TrackSolo => UnresolvedReaperTarget::TrackSolo {
                        track_descriptor: self.track_descriptor()?,
                        behavior: self.solo_behavior.get(),
                        exclusivity: self.track_exclusivity.get(),
                    },
                    TrackSendPan => UnresolvedReaperTarget::TrackSendPan {
                        descriptor: self.track_route_descriptor()?,
                    },
                    TrackSendMute => UnresolvedReaperTarget::TrackSendMute {
                        descriptor: self.track_route_descriptor()?,
                    },
                    Tempo => UnresolvedReaperTarget::Tempo,
                    Playrate => UnresolvedReaperTarget::Playrate,
                    AutomationModeOverride => UnresolvedReaperTarget::AutomationModeOverride {
                        mode_override: match self.automation_mode_override_type.get() {
                            AutomationModeOverrideType::Bypass => {
                                Some(GlobalAutomationModeOverride::Bypass)
                            }
                            AutomationModeOverrideType::Override => {
                                Some(GlobalAutomationModeOverride::Mode(
                                    self.track_automation_mode.get().to_reaper(),
                                ))
                            }
                            AutomationModeOverrideType::None => None,
                        },
                    },
                    FxEnable => UnresolvedReaperTarget::FxEnable {
                        fx_descriptor: self.fx_descriptor()?,
                    },
                    FxOpen => UnresolvedReaperTarget::FxOpen {
                        fx_descriptor: self.fx_descriptor()?,
                        display_type: self.fx_display_type.get(),
                    },
                    FxPreset => UnresolvedReaperTarget::FxPreset {
                        fx_descriptor: self.fx_descriptor()?,
                    },
                    SelectedTrack => UnresolvedReaperTarget::SelectedTrack {
                        scroll_arrange_view: self.scroll_arrange_view.get(),
                        scroll_mixer: self.scroll_mixer.get(),
                    },
                    FxNavigate => UnresolvedReaperTarget::FxNavigate {
                        track_descriptor: self.track_descriptor()?,
                        is_input_fx: self.fx_is_input_fx.get(),
                        display_type: self.fx_display_type.get(),
                    },
                    AllTrackFxEnable => UnresolvedReaperTarget::AllTrackFxEnable {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                    },
                    Transport => UnresolvedReaperTarget::Transport {
                        action: self.transport_action.get(),
                    },
                    LoadFxSnapshot => UnresolvedReaperTarget::LoadFxPreset {
                        fx_descriptor: self.fx_descriptor()?,
                        chunk: self
                            .fx_snapshot
                            .get_ref()
                            .as_ref()
                            .ok_or("FX chunk not set")?
                            .chunk
                            .clone(),
                    },
                    LastTouched => UnresolvedReaperTarget::LastTouched,
                    AutomationTouchState => UnresolvedReaperTarget::AutomationTouchState {
                        track_descriptor: self.track_descriptor()?,
                        parameter_type: self.touched_parameter_type.get(),
                        exclusivity: self.track_exclusivity.get(),
                    },
                    GoToBookmark => UnresolvedReaperTarget::GoToBookmark {
                        bookmark_type: self.bookmark_type.get(),
                        bookmark_anchor_type: self.bookmark_anchor_type.get(),
                        bookmark_ref: self.bookmark_ref.get(),
                        set_time_selection: self.use_time_selection.get(),
                        set_loop_points: self.use_loop_points.get(),
                    },
                    Seek => UnresolvedReaperTarget::Seek {
                        options: self.seek_options(),
                    },
                    SendMidi => UnresolvedReaperTarget::SendMidi {
                        pattern: self.raw_midi_pattern.get_ref().parse().unwrap_or_default(),
                        destination: self.send_midi_destination.get(),
                    },
                    SendOsc => UnresolvedReaperTarget::SendOsc {
                        address_pattern: self.osc_address_pattern.get_ref().clone(),
                        arg_descriptor: self.osc_arg_descriptor(),
                        device_id: self.osc_dev_id.get(),
                    },
                    ClipTransport => UnresolvedReaperTarget::ClipTransport {
                        // TODO-medium Make it possible to pass direct HW output channel instead
                        track_descriptor: Some(self.track_descriptor()?),
                        slot_index: self.slot_index.get(),
                        action: self.transport_action.get(),
                        play_options: self.slot_play_options(),
                    },
                    ClipSeek => UnresolvedReaperTarget::ClipSeek {
                        slot_index: self.slot_index.get(),
                        feedback_resolution: self.feedback_resolution.get(),
                    },
                    ClipVolume => UnresolvedReaperTarget::ClipVolume {
                        slot_index: self.slot_index.get(),
                    },
                };
                Ok(UnresolvedCompoundMappingTarget::Reaper(target))
            }
            Virtual => {
                let virtual_target = VirtualTarget::new(self.create_control_element());
                Ok(UnresolvedCompoundMappingTarget::Virtual(virtual_target))
            }
        }
    }

    pub fn slot_play_options(&self) -> SlotPlayOptions {
        SlotPlayOptions {
            next_bar: self.next_bar.get(),
            buffered: self.buffered.get(),
        }
    }

    fn osc_arg_descriptor(&self) -> Option<OscArgDescriptor> {
        let arg_index = self.osc_arg_index.get()?;
        Some(OscArgDescriptor::new(
            arg_index,
            self.osc_arg_type_tag.get(),
            // Doesn't matter for sending
            false,
        ))
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
        compartment: MappingCompartment,
    ) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            context,
            compartment,
        }
    }

    pub fn supports_track(&self) -> bool {
        let target_type = self.r#type.get();
        if !target_type.supports_track() {
            return false;
        }
        self.supports_track_apart_from_type()
    }

    pub fn supports_track_must_be_selected(&self) -> bool {
        if !self.r#type.get().supports_track_must_be_selected() {
            return false;
        }
        self.supports_track_apart_from_type()
    }

    fn supports_track_apart_from_type(&self) -> bool {
        if self.r#type.get() == ReaperTargetType::ClipTransport {
            use TransportAction::*;
            if !matches!(self.transport_action.get(), PlayStop | PlayPause) {
                return false;
            }
        }
        true
    }

    pub fn supports_fx(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.get().supports_fx()
    }

    pub fn supports_automation_mode(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        use ReaperTargetType::*;
        match self.r#type.get() {
            TrackAutomationMode => true,
            AutomationModeOverride => {
                self.automation_mode_override_type.get() == AutomationModeOverrideType::Override
            }
            _ => false,
        }
    }

    pub fn create_control_element(&self) -> VirtualControlElement {
        self.control_element_type
            .get()
            .create_control_element(self.control_element_id.get())
    }

    fn is_reaper(&self) -> bool {
        self.category.get() == TargetCategory::Reaper
    }

    pub fn is_virtual(&self) -> bool {
        self.category.get() == TargetCategory::Virtual
    }

    fn command_id_label(&self) -> Cow<str> {
        match self.action.get_ref() {
            None => "-".into(),
            Some(action) => {
                if action.is_available() {
                    action.command_id().to_string().into()
                } else if let Some(command_name) = action.command_name() {
                    format!("<Not present> ({})", command_name.to_str()).into()
                } else {
                    "<Not present>".into()
                }
            }
        }
    }

    pub fn action(&self) -> Result<Action, &'static str> {
        let action = self.action.get_ref().as_ref().ok_or("action not set")?;
        if !action.is_available() {
            return Err("action not available");
        }
        Ok(action.clone())
    }

    pub fn action_name_label(&self) -> Cow<str> {
        match self.action().ok() {
            None => "-".into(),
            Some(a) => a.name().into_string().into(),
        }
    }
}

impl fmt::Display for TargetModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.category.get() {
            TargetCategory::Reaper => {
                use ReaperTargetType::*;
                let tt = self.r#type.get();
                match tt {
                    Tempo | Playrate | SelectedTrack | LastTouched | Seek | TrackArm | TrackPan
                    | TrackWidth | TrackVolume | TrackPeak | TrackShow | TrackSolo | FxNavigate
                    | FxEnable | TrackMute | AllTrackFxEnable | TrackSelection | FxPreset
                    | FxOpen | FxParameter | TrackSendMute | TrackSendPan | TrackSendVolume
                    | LoadFxSnapshot | SendMidi | SendOsc => f.write_str(tt.short_name()),
                    ClipTransport | ClipSeek | ClipVolume => {
                        write!(f, "{}: Slot {}", tt.short_name(), self.slot_index.get() + 1)
                    }
                    Action => match self.action().ok() {
                        None => write!(f, "Action {}", self.command_id_label()),
                        Some(a) => f.write_str(a.name().to_str()),
                    },
                    AutomationModeOverride => {
                        write!(f, "{}: ", tt.short_name())?;
                        use AutomationModeOverrideType::*;
                        let ovr_type = self.automation_mode_override_type.get();
                        match ovr_type {
                            None | Bypass => write!(f, "{}", ovr_type),
                            Override => write!(f, "{}", self.track_automation_mode.get()),
                        }
                    }
                    Transport => {
                        write!(f, "{}", self.transport_action.get())
                    }
                    GoToBookmark => {
                        let type_label = match self.bookmark_type.get() {
                            BookmarkType::Marker => "Marker",
                            BookmarkType::Region => "Region",
                        };
                        let bm_prefix = match self.bookmark_anchor_type.get() {
                            BookmarkAnchorType::Id => "",
                            BookmarkAnchorType::Index => "#",
                        };
                        write!(
                            f,
                            "Go to {} {}{}",
                            type_label,
                            bm_prefix,
                            self.bookmark_ref.get()
                        )
                    }
                    TrackAutomationMode => write!(
                        f,
                        "{}: {}",
                        tt.short_name(),
                        self.track_automation_mode.get()
                    ),
                    AutomationTouchState => write!(
                        f,
                        "{}: {}",
                        tt.short_name(),
                        self.touched_parameter_type.get()
                    ),
                }
            }
            TargetCategory::Virtual => match self.control_element_id.get() {
                VirtualControlElementId::Indexed(i) => {
                    write!(f, "{} {}", self.control_element_type.get(), i + 1)
                }
                VirtualControlElementId::Named(n) => {
                    write!(f, "{} ({})", n, self.control_element_type.get())
                }
            },
        }
    }
}

pub fn get_fx_param_label(fx_param: Option<&FxParameter>, index: u32) -> Cow<'static, str> {
    let position = index + 1;
    match fx_param {
        None => format!("{}. <Not present>", position).into(),
        Some(p) => {
            let name = p.name();
            let name = name.to_str();
            if name.is_empty() {
                position.to_string().into()
            } else {
                format!("{}. {}", position, name).into()
            }
        }
    }
}

pub fn get_route_label(route: &TrackRoute) -> Cow<'static, str> {
    format!("{}. {}", route.index() + 1, route.name().to_str()).into()
}

pub fn get_virtual_fx_label(fx: Option<&Fx>, virtual_fx: Option<&VirtualFx>) -> Cow<'static, str> {
    let virtual_fx = match virtual_fx {
        None => return "<None>".into(),
        Some(f) => f,
    };
    match virtual_fx {
        VirtualFx::This => "<This>".into(),
        VirtualFx::Focused => "<Focused>".into(),
        VirtualFx::ChainFx { chain_fx, .. } => get_optional_fx_label(chain_fx, fx).into(),
    }
}

pub fn get_virtual_fx_param_label(
    fx_param: Option<&FxParameter>,
    virtual_fx_param: Option<&VirtualFxParameter>,
) -> Cow<'static, str> {
    let virtual_fx_param = match virtual_fx_param {
        None => return "<None>".into(),
        Some(f) => f,
    };
    match virtual_fx_param {
        VirtualFxParameter::Dynamic(_) => "<Dynamic>".into(),
        _ => match fx_param {
            None => format!("<Not present> ({})", virtual_fx_param).into(),
            Some(p) => get_fx_param_label(Some(p), p.index()),
        },
    }
}

pub fn get_virtual_route_label(
    route: Option<&TrackRoute>,
    virtual_route: Option<&VirtualTrackRoute>,
) -> Cow<'static, str> {
    let virtual_route = match virtual_route {
        None => return "<None>".into(),
        Some(r) => r,
    };
    match virtual_route.selector {
        TrackRouteSelector::Dynamic(_) => "<Dynamic>".into(),
        _ => match route {
            None => get_non_present_virtual_route_label(virtual_route).into(),
            Some(r) => get_route_label(r),
        },
    }
}

pub fn get_optional_fx_label(virtual_chain_fx: &VirtualChainFx, fx: Option<&Fx>) -> String {
    match virtual_chain_fx {
        VirtualChainFx::Dynamic(_) => "<Dynamic>".to_string(),
        _ => match fx {
            None => format!("<Not present> ({})", virtual_chain_fx),
            Some(fx) => get_fx_label(fx.index(), fx),
        },
    }
}

pub fn get_fx_label(index: u32, fx: &Fx) -> String {
    format!(
        "{}. {}",
        index + 1,
        // When closing project, this is sometimes not available anymore although the FX is still
        // picked up when querying the list of FXs! Prevent a panic.
        if fx.is_available() {
            fx.name().into_string()
        } else {
            "".to_owned()
        }
    )
}

pub struct TargetModelWithContext<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    compartment: MappingCompartment,
}

impl<'a> TargetModelWithContext<'a> {
    /// Creates a target based on this model's properties and the current REAPER state.
    ///
    /// This returns a target regardless of the activation conditions of the target. Example:
    /// If `enable_only_if_track_selected` is `true` and the track is _not_ selected when calling
    /// this function, the target will still be created!
    ///
    /// # Errors
    ///
    /// Returns an error if not enough information is provided by the model or if something (e.g.
    /// track/FX/parameter) is not available.
    pub fn resolve(&self) -> Result<Vec<CompoundMappingTarget>, &'static str> {
        let unresolved = self.target.create_target()?;
        unresolved.resolve(self.context, self.compartment)
    }

    pub fn resolve_first(&self) -> Result<CompoundMappingTarget, &'static str> {
        let targets = self.resolve()?;
        targets.into_iter().next().ok_or("resolved to empty list")
    }

    pub fn is_known_to_be_roundable(&self) -> bool {
        // TODO-low use cached
        self.resolve_first()
            .map(|t| {
                matches!(
                    t.control_type(),
                    ControlType::AbsoluteContinuousRoundable { .. }
                )
            })
            .unwrap_or(false)
    }
    // Returns an error if the FX doesn't exist.
    pub fn fx(&self) -> Result<Fx, &'static str> {
        get_fx(
            self.context,
            &self.target.fx_descriptor()?,
            self.compartment,
        )
    }

    pub fn project(&self) -> Project {
        self.context.context().project_or_current_project()
    }

    pub fn first_effective_track(&self) -> Result<Track, &'static str> {
        self.target
            .virtual_track()
            .ok_or("virtual track not complete")?
            .resolve(self.context, self.compartment)
            .map_err(|_| "particular track couldn't be resolved")?
            .into_iter()
            .next()
            .ok_or("resolved to empty track list")
    }

    // Returns an error if that send (or track) doesn't exist.
    pub fn track_route(&self) -> Result<TrackRoute, &'static str> {
        get_track_route(
            self.context,
            &self.target.track_route_descriptor()?,
            self.compartment,
        )
    }

    // Returns an error if that param (or FX) doesn't exist.
    fn fx_param(&self) -> Result<FxParameter, &'static str> {
        get_fx_param(
            self.context,
            &self.target.fx_parameter_descriptor()?,
            self.compartment,
        )
    }

    fn route_type_label(&self) -> &'static str {
        match self.target.route_type.get() {
            TrackRouteType::Send => "Send",
            TrackRouteType::Receive => "Receive",
            TrackRouteType::HardwareOutput => "Output",
        }
    }

    fn route_label(&self) -> Cow<str> {
        get_virtual_route_label(
            self.track_route().ok().as_ref(),
            self.target.virtual_track_route().ok().as_ref(),
        )
    }

    fn fx_label(&self) -> Cow<str> {
        get_virtual_fx_label(self.fx().ok().as_ref(), self.target.virtual_fx().as_ref())
    }

    fn fx_param_label(&self) -> Cow<str> {
        get_virtual_fx_param_label(
            self.fx_param().ok().as_ref(),
            self.target.virtual_fx_parameter().as_ref(),
        )
    }

    fn track_label(&self) -> String {
        if let Some(t) = self.target.virtual_track() {
            t.with_context(self.context, self.compartment).to_string()
        } else {
            "<Undefined>".to_owned()
        }
    }
}

impl<'a> Display for TargetModelWithContext<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetCategory::*;
        match self.target.category.get() {
            Reaper => {
                use ReaperTargetType::*;
                let tt = self.target.r#type.get();
                match tt {
                    Tempo | Playrate | SelectedTrack | LastTouched | Seek | SendMidi | SendOsc => {
                        write!(f, "{}", tt)
                    }
                    ClipTransport | ClipSeek | ClipVolume => {
                        write!(f, "{}", tt)
                    }
                    Action => write!(
                        f,
                        "{}\n{}\n{}",
                        tt,
                        self.target.command_id_label(),
                        self.target.action_name_label()
                    ),
                    FxParameter => write!(
                        f,
                        "{}\nTrack {}\nFX {}\nParam {}",
                        tt,
                        self.track_label(),
                        self.fx_label(),
                        self.fx_param_label()
                    ),
                    TrackVolume | TrackPeak | TrackPan | TrackWidth | TrackArm | TrackSelection
                    | TrackMute | TrackSolo | TrackShow | FxNavigate | AllTrackFxEnable => {
                        write!(f, "{}\nTrack {}", tt, self.track_label())
                    }
                    TrackAutomationMode => {
                        write!(
                            f,
                            "{}\nTrack {}\n{}",
                            tt,
                            self.track_label(),
                            self.target.track_automation_mode.get()
                        )
                    }
                    TrackSendVolume | TrackSendPan | TrackSendMute => write!(
                        f,
                        "{}\nTrack {}\n{} {}",
                        tt,
                        self.track_label(),
                        self.route_type_label(),
                        self.route_label()
                    ),
                    FxOpen | FxEnable | FxPreset => write!(
                        f,
                        "{}\nTrack {}\nFX {}",
                        tt,
                        self.track_label(),
                        self.fx_label(),
                    ),
                    Transport => write!(f, "{}\n{}", tt, self.target.transport_action.get()),
                    AutomationModeOverride => write!(
                        f,
                        "{}\n{}",
                        tt,
                        self.target.automation_mode_override_type.get()
                    ),
                    LoadFxSnapshot => write!(
                        f,
                        "{}\n{}",
                        tt,
                        self.target
                            .fx_snapshot
                            .get_ref()
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".to_owned())
                    ),
                    AutomationTouchState => write!(
                        f,
                        "{}\nTrack {}\n{}",
                        tt,
                        self.track_label(),
                        self.target.touched_parameter_type.get()
                    ),
                    GoToBookmark => {
                        let bookmark_type = self.target.bookmark_type.get();
                        let detail_label = {
                            let anchor_type = self.target.bookmark_anchor_type.get();
                            let bookmark_ref = self.target.bookmark_ref.get();
                            let res = find_bookmark(
                                self.project(),
                                bookmark_type,
                                anchor_type,
                                bookmark_ref,
                            );
                            if let Ok(res) = res {
                                get_bookmark_label(
                                    res.index_within_type,
                                    res.basic_info.id,
                                    &res.bookmark.name(),
                                )
                            } else {
                                get_non_present_bookmark_label(anchor_type, bookmark_ref)
                            }
                        };
                        write!(f, "{}\n{}", tt, detail_label)
                    }
                }
            }
            Virtual => write!(f, "Virtual\n{}", self.target.create_control_element()),
        }
    }
}

pub fn get_bookmark_label(index_within_type: u32, id: BookmarkId, name: &str) -> String {
    format!("{}. {} (ID {})", index_within_type + 1, name, id)
}

pub fn get_non_present_bookmark_label(
    anchor_type: BookmarkAnchorType,
    bookmark_ref: u32,
) -> String {
    match anchor_type {
        BookmarkAnchorType::Id => format!("<Not present> (ID {})", bookmark_ref),
        BookmarkAnchorType::Index => format!("{}. <Not present>", bookmark_ref),
    }
}

/// Type of a target
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum ReaperTargetType {
    // Global targets
    #[display(fmt = "Global: Last touched")]
    LastTouched = 20,
    #[display(fmt = "Global: Set automation mode override")]
    AutomationModeOverride = 26,

    // Project targets
    #[display(fmt = "Project: Invoke REAPER action")]
    Action = 0,
    #[display(fmt = "Project: Invoke transport action")]
    Transport = 16,
    #[display(fmt = "Project: Navigate between tracks")]
    SelectedTrack = 14,
    #[display(fmt = "Project: Seek")]
    Seek = 23,
    #[display(fmt = "Project: Set playrate")]
    Playrate = 11,
    #[display(fmt = "Project: Set tempo")]
    Tempo = 10,

    // Marker/region targets
    #[display(fmt = "Marker/region: Go to")]
    GoToBookmark = 22,

    // Track targets
    #[display(fmt = "Track: Arm/disarm")]
    TrackArm = 5,
    #[display(fmt = "Track: Enable/disable all FX")]
    AllTrackFxEnable = 15,
    #[display(fmt = "Track: Mute/unmute")]
    TrackMute = 7,
    #[display(fmt = "Track: Peak")]
    TrackPeak = 34,
    #[display(fmt = "Track: Select/unselect")]
    TrackSelection = 6,
    #[display(fmt = "Track: Set automation mode")]
    TrackAutomationMode = 25,
    #[display(fmt = "Track: Set automation touch state")]
    AutomationTouchState = 21,
    #[display(fmt = "Track: Set pan")]
    TrackPan = 4,
    #[display(fmt = "Track: Set stereo pan width")]
    TrackWidth = 17,
    #[display(fmt = "Track: Set volume")]
    TrackVolume = 2,
    #[display(fmt = "Track: Show/hide")]
    TrackShow = 24,
    #[display(fmt = "Track: Solo/unsolo")]
    TrackSolo = 8,

    // FX chain targets
    #[display(fmt = "FX chain: Navigate between FXs")]
    // FX targets
    FxNavigate = 28,
    #[display(fmt = "FX: Enable/disable")]
    FxEnable = 12,
    #[display(fmt = "FX: Load snapshot")]
    LoadFxSnapshot = 19,
    #[display(fmt = "FX: Navigate between presets")]
    FxPreset = 13,
    #[display(fmt = "FX: Open/close")]
    FxOpen = 27,
    #[display(fmt = "FX: Set parameter value")]
    FxParameter = 1,

    // Send targets
    #[display(fmt = "Send: Mute/unmute")]
    TrackSendMute = 18,
    #[display(fmt = "Send: Set pan")]
    TrackSendPan = 9,
    #[display(fmt = "Send: Set volume")]
    TrackSendVolume = 3,

    // Clip targets
    #[display(fmt = "Clip: Invoke transport action")]
    ClipTransport = 31,
    #[display(fmt = "Clip: Seek")]
    ClipSeek = 32,
    #[display(fmt = "Clip: Volume")]
    ClipVolume = 33,

    // Misc
    #[display(fmt = "MIDI: Send message")]
    SendMidi = 29,
    #[display(fmt = "OSC: Send message")]
    SendOsc = 30,
}

impl Default for ReaperTargetType {
    fn default() -> Self {
        ReaperTargetType::FxParameter
    }
}

impl ReaperTargetType {
    pub fn from_target(target: &ReaperTarget) -> ReaperTargetType {
        use ReaperTarget::*;
        match target {
            Action { .. } => ReaperTargetType::Action,
            FxParameter { .. } => ReaperTargetType::FxParameter,
            TrackVolume { .. } => ReaperTargetType::TrackVolume,
            TrackPeak { .. } => ReaperTargetType::TrackPeak,
            TrackRouteVolume { .. } => ReaperTargetType::TrackSendVolume,
            TrackPan { .. } => ReaperTargetType::TrackPan,
            TrackWidth { .. } => ReaperTargetType::TrackWidth,
            TrackArm { .. } => ReaperTargetType::TrackArm,
            TrackSelection { .. } => ReaperTargetType::TrackSelection,
            TrackMute { .. } => ReaperTargetType::TrackMute,
            TrackSolo { .. } => ReaperTargetType::TrackSolo,
            TrackRoutePan { .. } => ReaperTargetType::TrackSendPan,
            TrackRouteMute { .. } => ReaperTargetType::TrackSendMute,
            Tempo { .. } => ReaperTargetType::Tempo,
            Playrate { .. } => ReaperTargetType::Playrate,
            FxEnable { .. } => ReaperTargetType::FxEnable,
            FxPreset { .. } => ReaperTargetType::FxPreset,
            SelectedTrack { .. } => ReaperTargetType::SelectedTrack,
            AllTrackFxEnable { .. } => ReaperTargetType::AllTrackFxEnable,
            Transport { .. } => ReaperTargetType::Transport,
            LoadFxSnapshot { .. } => ReaperTargetType::LoadFxSnapshot,
            AutomationTouchState { .. } => ReaperTargetType::AutomationTouchState,
            GoToBookmark { .. } => ReaperTargetType::GoToBookmark,
            Seek { .. } => ReaperTargetType::Seek,
            TrackShow { .. } => ReaperTargetType::TrackShow,
            TrackAutomationMode { .. } => ReaperTargetType::TrackAutomationMode,
            AutomationModeOverride { .. } => ReaperTargetType::AutomationModeOverride,
            FxOpen { .. } => ReaperTargetType::FxOpen,
            FxNavigate { .. } => ReaperTargetType::FxNavigate,
            SendMidi { .. } => ReaperTargetType::SendMidi,
            SendOsc { .. } => ReaperTargetType::SendOsc,
            ClipTransport { .. } => ReaperTargetType::ClipTransport,
            ClipSeek { .. } => ReaperTargetType::ClipSeek,
            ClipVolume { .. } => ReaperTargetType::ClipVolume,
        }
    }

    pub fn supports_feedback_resolution(self) -> bool {
        use ReaperTargetType::*;
        matches!(self, Seek | ClipSeek)
    }

    pub fn supports_track(self) -> bool {
        use ReaperTargetType::*;
        match self {
            FxParameter | TrackVolume | TrackPeak | TrackSendVolume | TrackPan | TrackWidth
            | TrackArm | TrackSelection | TrackMute | TrackShow | TrackAutomationMode
            | TrackSolo | TrackSendPan | TrackSendMute | FxEnable | FxOpen | FxNavigate
            | FxPreset | AllTrackFxEnable | LoadFxSnapshot | AutomationTouchState
            | ClipTransport => true,
            Action
            | Tempo
            | Playrate
            | SelectedTrack
            | Transport
            | LastTouched
            | GoToBookmark
            | Seek
            | SendMidi
            | SendOsc
            | AutomationModeOverride
            | ClipSeek
            | ClipVolume => false,
        }
    }

    pub fn supports_track_must_be_selected(self) -> bool {
        use ReaperTargetType::*;
        self.supports_track() && !matches!(self, TrackSelection)
    }

    pub fn supports_track_scrolling(self) -> bool {
        use ReaperTargetType::*;
        matches!(self, TrackSelection | SelectedTrack)
    }

    pub fn supports_slot(self) -> bool {
        use ReaperTargetType::*;
        matches!(self, ClipSeek | ClipTransport | ClipVolume)
    }

    pub fn supports_fx(self) -> bool {
        use ReaperTargetType::*;
        match self {
            FxParameter | FxOpen | FxEnable | FxPreset | LoadFxSnapshot => true,
            TrackSendVolume
            | TrackSendPan
            | TrackSendMute
            | TrackVolume
            | TrackPeak
            | TrackPan
            | TrackWidth
            | TrackArm
            | TrackSelection
            | TrackMute
            | TrackSolo
            | Action
            | Tempo
            | Playrate
            | SelectedTrack
            | AllTrackFxEnable
            | Transport
            | LastTouched
            | AutomationTouchState
            | GoToBookmark
            | Seek
            | TrackShow
            | TrackAutomationMode
            | AutomationModeOverride
            | SendMidi
            | SendOsc
            | ClipTransport
            | ClipSeek
            | ClipVolume
            | FxNavigate => false,
        }
    }

    pub fn supports_fx_chain(self) -> bool {
        use ReaperTargetType::*;
        self.supports_fx() || matches!(self, FxNavigate)
    }

    pub fn supports_fx_display_type(self) -> bool {
        use ReaperTargetType::*;
        matches!(self, FxOpen | FxNavigate)
    }

    pub fn supports_send(self) -> bool {
        use ReaperTargetType::*;
        match self {
            TrackSendVolume | TrackSendPan | TrackSendMute => true,
            FxParameter
            | TrackVolume
            | TrackPeak
            | TrackPan
            | TrackWidth
            | TrackArm
            | TrackSelection
            | TrackMute
            | TrackSolo
            | FxEnable
            | FxPreset
            | Action
            | Tempo
            | Playrate
            | SelectedTrack
            | AllTrackFxEnable
            | Transport
            | LoadFxSnapshot
            | LastTouched
            | AutomationTouchState
            | GoToBookmark
            | Seek
            | TrackShow
            | TrackAutomationMode
            | AutomationModeOverride
            | FxOpen
            | SendMidi
            | SendOsc
            | ClipTransport
            | ClipSeek
            | ClipVolume
            | FxNavigate => false,
        }
    }

    pub fn supports_track_exclusivity(self) -> bool {
        use ReaperTargetType::*;
        match self {
            TrackArm | TrackSelection | AllTrackFxEnable | TrackMute | TrackSolo
            | AutomationTouchState | TrackShow | TrackAutomationMode => true,
            TrackSendVolume
            | TrackSendPan
            | TrackSendMute
            | FxParameter
            | TrackVolume
            | TrackPeak
            | TrackPan
            | TrackWidth
            | FxEnable
            | FxPreset
            | Action
            | Tempo
            | Playrate
            | SelectedTrack
            | Transport
            | LoadFxSnapshot
            | LastTouched
            | GoToBookmark
            | Seek
            | AutomationModeOverride
            | FxOpen
            | SendMidi
            | SendOsc
            | ClipTransport
            | ClipSeek
            | ClipVolume
            | FxNavigate => false,
        }
    }

    pub fn supports_control(&self) -> bool {
        true
    }

    pub fn supports_feedback(&self) -> bool {
        use ReaperTargetType::*;
        !matches!(self, SendMidi | SendOsc)
    }

    pub fn hint(&self) -> &'static str {
        use ReaperTargetType::*;
        match self {
            FxEnable => "No feedback from automation",
            FxPreset => "Automatic feedback since REAPER v6.13",
            Action => "Limited feedback only",
            Seek => "Experimental target",
            ClipTransport => "Experimental target, record not supported",
            TrackSendMute | AllTrackFxEnable | TrackShow => "No automatic feedback",
            TrackPeak => "Feedback only, no control",
            _ => "",
        }
    }

    pub fn short_name(&self) -> &'static str {
        use ReaperTargetType::*;
        match self {
            LastTouched => "Last touched",
            AutomationModeOverride => "Automation override",
            Action => "Action",
            Transport => "Transport",
            SelectedTrack => "Navigate tracks",
            Seek => "Seek",
            Playrate => "Playrate",
            Tempo => "Tempo",
            GoToBookmark => "Go to bookmark",
            TrackArm => "(Dis)arm track",
            AllTrackFxEnable => "Enable/disable all track FX",
            TrackMute => "(Un)mute track",
            TrackSelection => "(Un)select track",
            TrackAutomationMode => "Track automation mode",
            AutomationTouchState => "Automation touch state",
            TrackPan => "Track pan",
            TrackWidth => "Track pan width",
            TrackVolume => "Track volume",
            TrackPeak => "Track peak",
            TrackShow => "Show/hide track",
            TrackSolo => "(Un)solo track",
            FxNavigate => "Navigate FXs",
            FxEnable => "Enable/disable FX",
            LoadFxSnapshot => "Load FX snapshot",
            FxPreset => "Navigate FX presets",
            FxOpen => "Open/close FX",
            FxParameter => "FX parameter value",
            TrackSendMute => "(Un)mute send",
            TrackSendPan => "Send pan",
            TrackSendVolume => "Send volume",
            SendMidi => "Send MIDI",
            SendOsc => "Send OSC",
            ClipTransport => "Clip transport",
            ClipSeek => "Clip seek",
            ClipVolume => "Clip volume",
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TargetCategory {
    #[serde(rename = "reaper")]
    #[display(fmt = "REAPER")]
    Reaper,
    #[serde(rename = "virtual")]
    #[display(fmt = "Virtual")]
    Virtual,
}

impl TargetCategory {
    pub fn default_for(compartment: MappingCompartment) -> Self {
        use TargetCategory::*;
        match compartment {
            MappingCompartment::ControllerMappings => Virtual,
            MappingCompartment::MainMappings => Reaper,
        }
    }

    pub fn is_allowed_in(self, compartment: MappingCompartment) -> bool {
        use TargetCategory::*;
        match compartment {
            MappingCompartment::ControllerMappings => true,
            MappingCompartment::MainMappings => {
                matches!(self, Reaper)
            }
        }
    }
}

impl Default for TargetCategory {
    fn default() -> Self {
        TargetCategory::Reaper
    }
}

fn virtualize_track(track: Track, context: &ProcessorContext) -> VirtualTrack {
    let own_track = context
        .track()
        .cloned()
        .unwrap_or_else(|| context.project_or_current_project().master_track());
    if own_track == track {
        VirtualTrack::This
    } else if track.is_master_track() {
        VirtualTrack::Master
    } else if context.is_on_monitoring_fx_chain() {
        // Doesn't make sense to refer to tracks via ID if we are on monitoring FX chain.
        VirtualTrack::ByIndex(track.index().expect("impossible"))
    } else {
        VirtualTrack::ById(*track.guid())
    }
}

fn virtualize_fx(fx: &Fx, context: &ProcessorContext) -> VirtualFx {
    if context.containing_fx() == fx {
        VirtualFx::This
    } else {
        VirtualFx::ChainFx {
            is_input_fx: fx.is_input_fx(),
            chain_fx: if context.is_on_monitoring_fx_chain() {
                // Doesn't make sense to refer to FX via UUID if we are on monitoring FX chain.
                VirtualChainFx::ByIndex(fx.index())
            } else if let Some(guid) = fx.guid() {
                VirtualChainFx::ById(guid, Some(fx.index()))
            } else {
                // Don't know how that can happen but let's handle it gracefully.
                VirtualChainFx::ByIdOrIndex(None, fx.index())
            },
        }
    }
}

fn virtualize_route(route: &TrackRoute, context: &ProcessorContext) -> VirtualTrackRoute {
    let partner = route.partner();
    VirtualTrackRoute {
        r#type: match route.direction() {
            TrackSendDirection::Receive => TrackRouteType::Receive,
            TrackSendDirection::Send => {
                if matches!(partner, Some(TrackRoutePartner::HardwareOutput(_))) {
                    TrackRouteType::HardwareOutput
                } else {
                    TrackRouteType::Send
                }
            }
        },
        selector: if context.is_on_monitoring_fx_chain() {
            // Doesn't make sense to refer to route via related-track UUID if we are on monitoring
            // FX chain.
            TrackRouteSelector::ByIndex(route.index())
        } else {
            match partner {
                None | Some(TrackRoutePartner::HardwareOutput(_)) => {
                    TrackRouteSelector::ByIndex(route.index())
                }
                Some(TrackRoutePartner::Track(t)) => TrackRouteSelector::ById(*t.guid()),
            }
        },
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, IntoEnumIterator, TryFromPrimitive, IntoPrimitive, Display,
)]
#[repr(usize)]
pub enum VirtualTrackType {
    #[display(fmt = "<This>")]
    This,
    #[display(fmt = "<Selected>")]
    Selected,
    #[display(fmt = "<All selected>")]
    SelectedMultiple,
    #[display(fmt = "<Dynamic>")]
    Dynamic,
    #[display(fmt = "<Master>")]
    Master,
    #[display(fmt = "By ID")]
    ById,
    #[display(fmt = "By name")]
    ByName,
    #[display(fmt = "By position")]
    ByIndex,
    #[display(fmt = "By ID or name")]
    ByIdOrName,
}

impl Default for VirtualTrackType {
    fn default() -> Self {
        Self::This
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum BookmarkAnchorType {
    #[display(fmt = "By ID")]
    Id,
    #[display(fmt = "By position")]
    Index,
}

impl Default for BookmarkAnchorType {
    fn default() -> Self {
        Self::Id
    }
}

impl VirtualTrackType {
    pub fn from_virtual_track(virtual_track: &VirtualTrack) -> Self {
        use VirtualTrack::*;
        match virtual_track {
            This => Self::This,
            Selected { allow_multiple } => {
                if *allow_multiple {
                    Self::SelectedMultiple
                } else {
                    Self::Selected
                }
            }
            Dynamic(_) => Self::Dynamic,
            Master => Self::Master,
            ByIdOrName(_, _) => Self::ByIdOrName,
            ById(_) => Self::ById,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use VirtualTrackType::*;
        matches!(self, ByIdOrName | ById)
    }

    pub fn track_selected_condition_makes_sense(&self) -> bool {
        use VirtualTrackType::*;
        !matches!(self, Selected | SelectedMultiple)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum VirtualFxType {
    #[display(fmt = "<This>")]
    #[serde(rename = "this")]
    This,
    #[display(fmt = "<Focused>")]
    #[serde(rename = "focused")]
    Focused,
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
    #[display(fmt = "By ID or pos")]
    #[serde(rename = "id-or-index")]
    ByIdOrIndex,
}

impl Default for VirtualFxType {
    fn default() -> Self {
        Self::ById
    }
}

impl VirtualFxType {
    pub fn from_virtual_fx(virtual_fx: &VirtualFx) -> Self {
        use VirtualFx::*;
        match virtual_fx {
            This => VirtualFxType::This,
            Focused => VirtualFxType::Focused,
            ChainFx { chain_fx, .. } => {
                use VirtualChainFx::*;
                match chain_fx {
                    Dynamic(_) => Self::Dynamic,
                    ById(_, _) => Self::ById,
                    ByName(_) => Self::ByName,
                    ByIndex(_) => Self::ByIndex,
                    ByIdOrIndex(_, _) => Self::ByIdOrIndex,
                }
            }
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use VirtualFxType::*;
        matches!(self, ById | ByIdOrIndex)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum VirtualFxParameterType {
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
}

impl Default for VirtualFxParameterType {
    fn default() -> Self {
        Self::ByIndex
    }
}

impl VirtualFxParameterType {
    pub fn from_virtual_fx_parameter(param: &VirtualFxParameter) -> Self {
        use VirtualFxParameter::*;
        match param {
            Dynamic(_) => Self::Dynamic,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum TrackRouteSelectorType {
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
}

impl Default for TrackRouteSelectorType {
    fn default() -> Self {
        Self::ByIndex
    }
}

impl TrackRouteSelectorType {
    pub fn from_route_selector(selector: &TrackRouteSelector) -> Self {
        use TrackRouteSelector::*;
        match selector {
            Dynamic(_) => Self::Dynamic,
            ById(_) => Self::ById,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxSnapshot {
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_type: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub preset_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub chunk: Rc<String>,
}

impl Clone for FxSnapshot {
    fn clone(&self) -> Self {
        Self {
            fx_type: self.fx_type.clone(),
            fx_name: self.fx_name.clone(),
            preset_name: self.preset_name.clone(),
            // We want a totally detached duplicate.
            chunk: Rc::new((*self.chunk).clone()),
        }
    }
}

impl Display for FxSnapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let fmt_size = bytesize::ByteSize(self.chunk.len() as _);
        write!(
            f,
            "{} | {} | {}",
            self.preset_name.as_deref().unwrap_or("-"),
            fmt_size,
            self.fx_name,
        )
    }
}

#[derive(Default)]
pub struct TrackPropValues {
    pub r#type: VirtualTrackType,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl TrackPropValues {
    pub fn from_virtual_track(track: VirtualTrack) -> Self {
        Self {
            r#type: VirtualTrackType::from_virtual_track(&track),
            id: track.id(),
            name: track.name().unwrap_or_default(),
            index: track.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct TrackRoutePropValues {
    pub selector_type: TrackRouteSelectorType,
    pub r#type: TrackRouteType,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl TrackRoutePropValues {
    pub fn from_virtual_route(route: VirtualTrackRoute) -> Self {
        Self {
            selector_type: TrackRouteSelectorType::from_route_selector(&route.selector),
            r#type: route.r#type,
            id: route.id(),
            name: route.name().unwrap_or_default(),
            index: route.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct FxPropValues {
    pub r#type: VirtualFxType,
    pub is_input_fx: bool,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl FxPropValues {
    pub fn from_virtual_fx(fx: VirtualFx) -> Self {
        Self {
            r#type: VirtualFxType::from_virtual_fx(&fx),
            is_input_fx: fx.is_input_fx(),
            id: fx.id(),
            name: fx.name().unwrap_or_default(),
            index: fx.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct FxParameterPropValues {
    pub r#type: VirtualFxParameterType,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum RealearnTrackArea {
    #[serde(rename = "tcp")]
    #[display(fmt = "Track control panel")]
    Tcp,
    #[serde(rename = "mcp")]
    #[display(fmt = "Mixer control panel")]
    Mcp,
}

impl Default for RealearnTrackArea {
    fn default() -> Self {
        Self::Tcp
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum RealearnAutomationMode {
    #[display(fmt = "Trim/Read")]
    TrimRead = 0,
    #[display(fmt = "Read")]
    Read = 1,
    #[display(fmt = "Touch")]
    Touch = 2,
    #[display(fmt = "Write")]
    Write = 3,
    #[display(fmt = "Latch")]
    Latch = 4,
    #[display(fmt = "Latch Preview")]
    LatchPreview = 5,
}

impl Default for RealearnAutomationMode {
    fn default() -> Self {
        Self::TrimRead
    }
}

impl RealearnAutomationMode {
    fn to_reaper(self) -> AutomationMode {
        use RealearnAutomationMode::*;
        match self {
            TrimRead => AutomationMode::TrimRead,
            Read => AutomationMode::Read,
            Touch => AutomationMode::Touch,
            Write => AutomationMode::Write,
            Latch => AutomationMode::Latch,
            LatchPreview => AutomationMode::LatchPreview,
        }
    }

    fn from_reaper(value: AutomationMode) -> Self {
        use AutomationMode::*;
        match value {
            TrimRead => Self::TrimRead,
            Read => Self::Read,
            Touch => Self::Touch,
            Write => Self::Write,
            Latch => Self::Latch,
            LatchPreview => Self::LatchPreview,
            Unknown(_) => Self::TrimRead,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum AutomationModeOverrideType {
    #[serde(rename = "none")]
    #[display(fmt = "None")]
    None,
    #[serde(rename = "bypass")]
    #[display(fmt = "Bypass all envelopes")]
    Bypass,
    #[serde(rename = "override")]
    #[display(fmt = "Override")]
    Override,
}

impl Default for AutomationModeOverrideType {
    fn default() -> Self {
        Self::Bypass
    }
}
