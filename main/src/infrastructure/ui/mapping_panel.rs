use anyhow::{bail, Context};
use camino::Utf8PathBuf;
use derive_more::Display;
use helgoboss_midi::{Channel, ShortMessageType, U7};
use itertools::Itertools;
use reaper_high::{
    BookmarkType, Fx, FxChain, Project, Reaper, SendPartnerType, Track, TrackRoutePartner,
};
use reaper_low::raw;
use reaper_medium::{
    Hbrush, InitialAction, NativeColor, PromptForActionResult, SectionId, WindowContext,
};
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::convert::TryInto;
use std::ptr::null;
use std::rc::Rc;
use std::time::Duration;
use std::{cmp, iter};
use strum::IntoEnumIterator;

use helgoboss_learn::{
    format_percentage_without_unit, AbsoluteMode, AbsoluteValue, ButtonUsage, ControlValue,
    DiscreteIncrement, DisplayType, EncoderUsage, FeedbackType, FireMode, GroupInteraction,
    Interval, MackieSevenSegmentDisplayScope, MidiClockTransportMessage, ModeParameter, OscTypeTag,
    OutOfRangeBehavior, PercentIo, RgbColor, SourceCharacter, TakeoverMode, Target, UnitValue,
    ValueSequence, VirtualColor, DEFAULT_OSC_ARG_VALUE_RANGE,
};
use helgobox_api::persistence::{
    ActionScope, Axis, BrowseTracksMode, FxDescriptor, FxToolAction, LearnableTargetKind,
    MidiScriptKind, MonitoringMode, MouseButton, PlaytimeColumnAction, PlaytimeColumnDescriptor,
    PlaytimeColumnDescriptorKind, PlaytimeMatrixAction, PlaytimeRowAction, PlaytimeRowDescriptor,
    PlaytimeRowDescriptorKind, PlaytimeSlotDescriptor, PlaytimeSlotDescriptorKind,
    PlaytimeSlotManagementAction, PlaytimeSlotTransportAction, PotFilterKind, SeekBehavior,
    TrackToolAction, VirtualControlElementCharacter,
};
use swell_ui::{
    DeviceContext, DialogUnits, Point, SharedView, SwellStringArg, View, ViewContext, WeakView,
    Window,
};

use crate::application::{
    build_smart_command_name_from_action, format_osc_feedback_args, get_bookmark_label_by_id,
    get_fx_label, get_fx_param_label, get_non_present_bookmark_label, get_optional_fx_label,
    get_route_label, parse_osc_feedback_args, Affected, AutomationModeOverrideType,
    BookmarkAnchorType, Change, CompartmentProp, ConcreteFxInstruction, ConcreteTrackInstruction,
    MappingChangeContext, MappingCommand, MappingModel, MappingModificationKind, MappingProp,
    MappingRefModel, MappingSnapshotTypeForLoad, MappingSnapshotTypeForTake, MidiSourceType,
    ModeCommand, ModeModel, ModeProp, RealearnAutomationMode, RealearnTrackArea, ReaperSourceType,
    SessionProp, SharedMapping, SharedUnitModel, SourceCategory, SourceCommand, SourceModel,
    SourceProp, StreamDeckButtonBackgroundType, StreamDeckButtonForegroundType, TargetCategory,
    TargetCommand, TargetModel, TargetModelFormatVeryShort, TargetModelWithContext, TargetProp,
    TargetUnit, TrackRouteSelectorType, UnitModel, VirtualFxParameterType, VirtualFxType,
    VirtualTrackType, WeakUnitModel, KEY_UNDEFINED_LABEL,
};
use crate::base::{notification, when, Prop};
use crate::domain::ui_util::{
    format_as_percentage_without_unit, format_tags_as_csv, parse_unit_value_from_percentage,
};
use crate::domain::{
    control_element_domains, AnyOnParameter, Backbone, ControlContext, Exclusivity,
    FeedbackSendBehavior, KeyStrokePortability, MouseActionType, PortabilityIssue, ReaperTarget,
    ReaperTargetType, SendMidiDestinationType, SimpleExclusivity, SourceFeedbackEvent,
    TargetControlEvent, TouchedRouteParameterType, TrackGangBehavior, WithControlContext,
};
use crate::domain::{
    get_non_present_virtual_route_label, get_non_present_virtual_track_label,
    resolve_track_route_by_index, ActionInvocationType, CompartmentKind, CompoundMappingTarget,
    ExtendedProcessorContext, FeedbackResolution, FxDisplayType, QualifiedMappingId,
    RealearnTarget, SoloBehavior, TargetCharacter, TouchedTrackParameterType, TrackExclusivity,
    TrackRouteType, TransportAction, VirtualControlElement, VirtualControlElementId, VirtualFx,
};
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::color_panel::{ColorPanel, ColorPanelDesc};
use crate::infrastructure::ui::help::{
    ConceptTopic, HelpTopic, HelpTopicDescription, MappingTopic, SourceTopic, TargetTopic,
};
use crate::infrastructure::ui::menus::{
    build_enum_variants_menu, get_midi_input_device_list_label, get_param_name,
};
use crate::infrastructure::ui::ui_element_container::UiElementContainer;
use crate::infrastructure::ui::util::colors::ColorPair;
use crate::infrastructure::ui::util::{
    close_child_panel_if_open, colors, compartment_parameter_dropdown_contents,
    open_child_panel_dyn, open_in_browser, parse_tags_from_csv, symbols, view,
    MAPPING_PANEL_SCALING,
};
use crate::infrastructure::ui::{
    menus, EelControlTransformationEngine, EelFeedbackTransformationEngine, EelMidiScriptEngine,
    ItemProp, LuaFeedbackScriptEngine, LuaMidiScriptEngine, MappingHeaderPanel, MappingRowsPanel,
    OscFeedbackArgumentsEngine, PlainTextEngine, RawMidiScriptEngine, ScriptEditorInput,
    ScriptEngine, SimpleScriptEditorPanel, TextualFeedbackExpressionEngine, UnitPanel,
    YamlEditorPanel,
};
use base::hash_util::NonCryptoHashMap;
use base::Global;
use playtime_api::persistence::{ColumnAddress, RowAddress, SlotAddress};

#[derive(Debug)]
pub struct MappingPanel {
    view: ViewContext,
    session: WeakUnitModel,
    mapping: RefCell<Option<SharedMapping>>,
    main_panel: WeakView<UnitPanel>,
    mapping_color_panel: SharedView<ColorPanel>,
    source_color_panel: SharedView<ColorPanel>,
    target_color_panel: SharedView<ColorPanel>,
    glue_color_panel: SharedView<ColorPanel>,
    help_color_panel: SharedView<ColorPanel>,
    mapping_header_panel: SharedView<MappingHeaderPanel>,
    is_invoked_programmatically: Cell<bool>,
    window_cache: RefCell<Option<WindowCache>>,
    extra_panel: RefCell<Option<SharedView<dyn View>>>,
    active_help_topic: RefCell<Prop<Option<HelpTopic>>>,
    ui_element_container: RefCell<UiElementContainer>,
    // Fires when a mapping is about to change or the panel is hidden.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
}

struct ImmutableMappingPanel<'a> {
    session: &'a UnitModel,
    mapping: &'a MappingModel,
    source: &'a SourceModel,
    mode: &'a ModeModel,
    target: &'a TargetModel,
    view: &'a ViewContext,
    panel: &'a SharedView<MappingPanel>,
}

struct MutableMappingPanel<'a> {
    session: &'a mut UnitModel,
    mapping: &'a mut MappingModel,
    panel: &'a SharedView<MappingPanel>,
    view: &'a ViewContext,
}

#[derive(Debug)]
struct WindowCache {
    mode_min_target_value: Window,
    mode_max_target_value: Window,
    mode_min_source_value: Window,
    mode_max_source_value: Window,
    mode_min_step_size: Window,
    mode_max_step_size: Window,
    mode_fire_line_2: Window,
    mode_fire_line_3: Window,
    target_value: Window,
}

const SAME_AS_INPUT_DEV: &str = "<Same as input device>";

impl MappingPanel {
    pub fn new(session: WeakUnitModel, main_panel: WeakView<UnitPanel>) -> MappingPanel {
        MappingPanel {
            view: Default::default(),
            session: session.clone(),
            mapping: None.into(),
            main_panel,
            mapping_color_panel: SharedView::new(ColorPanel::new(build_mapping_color_panel_desc())),
            source_color_panel: SharedView::new(ColorPanel::new(build_source_color_panel_desc())),
            target_color_panel: SharedView::new(ColorPanel::new(build_target_color_panel_desc())),
            glue_color_panel: SharedView::new(ColorPanel::new(build_glue_color_panel_desc())),
            help_color_panel: SharedView::new(ColorPanel::new(build_help_color_panel_desc())),
            mapping_header_panel: SharedView::new(MappingHeaderPanel::new(
                session,
                Point::new(DialogUnits(7 + 5), DialogUnits(12)).scale(&MAPPING_PANEL_SCALING),
                None,
            )),
            is_invoked_programmatically: false.into(),
            window_cache: None.into(),
            extra_panel: Default::default(),
            active_help_topic: Default::default(),
            ui_element_container: Default::default(),
            party_is_over_subject: Default::default(),
        }
    }

    fn handle_hovered_ui_element(&self, resource_id: Option<u32>) {
        let help_topic = resource_id.and_then(find_help_topic_for_resource);
        self.active_help_topic.borrow_mut().set(help_topic);
    }

    fn set_invoked_programmatically(&self, value: bool) {
        self.is_invoked_programmatically.set(value);
        // I already ran into a borrow error because the mapping header panel was updated
        // as well as part of a programmatic reaction but didn't have that flag set. So set it
        // there as well!
        self.mapping_header_panel
            .set_invoked_programmatically(value);
    }

    pub fn handle_affected(
        self: &SharedView<Self>,
        affected: &Affected<SessionProp>,
        initiator: Option<u32>,
    ) {
        use Affected::*;
        use CompartmentProp::*;
        use SessionProp::*;
        match affected {
            One(InCompartment(compartment, One(InMapping(mapping_id, affected))))
                if Some(QualifiedMappingId::new(*compartment, *mapping_id))
                    == self.qualified_mapping_id() =>
            {
                // At this point we know already it's a prop change for *our* mapping.
                // Mark as programmatic invocation.
                let panel_clone = self.clone();
                panel_clone.set_invoked_programmatically(true);
                scopeguard::defer! { panel_clone.set_invoked_programmatically(false); }
                // If the reaction can't be displayed anymore because the mapping is not filled anymore,
                // so what.
                let _ = self.clone().read(|view| match affected {
                        Multiple => {
                            view.invalidate_all_controls();
                        }
                        One(prop) => {
                            use MappingProp as P;
                            match prop {
                                P::Name => {
                                    view.invalidate_window_title();
                                    view.panel
                                        .mapping_header_panel
                                        .invalidate_due_to_changed_prop(ItemProp::Name, initiator);
                                }
                                P::Tags => {
                                    view.panel
                                        .mapping_header_panel
                                        .invalidate_due_to_changed_prop(ItemProp::Tags, initiator);
                                }
                                P::AdvancedSettings => {
                                    view.invalidate_mapping_advanced_settings_button();
                                }
                                P::ControlIsEnabled => {
                                    view.panel
                                        .mapping_header_panel
                                        .invalidate_due_to_changed_prop(
                                            ItemProp::ControlEnabled,
                                            initiator,
                                        );
                                    view.invalidate_mode_controls();
                                }
                                P::FeedbackIsEnabled => {
                                    view.panel
                                        .mapping_header_panel
                                        .invalidate_due_to_changed_prop(
                                            ItemProp::FeedbackEnabled,
                                            initiator,
                                        );
                                    view.invalidate_mode_controls();
                                }
                                P::BeepOnSuccess => {
                                    view.invalidate_beep_on_success_checkbox();
                                }
                                P::IsEnabled => {
                                    view.invalidate_mapping_enabled_check_box();
                                }
                                P::VisibleInProjection => {
                                    view.invalidate_mapping_visible_in_projection_check_box();
                                }
                                P::FeedbackSendBehavior => {
                                    view.invalidate_mapping_feedback_send_behavior_combo_box();
                                }
                                P::GroupId => {}
                                P::InActivationCondition(p) => match p {
                                    Multiple => {
                                        view.panel.mapping_header_panel.invalidate_controls();
                                    }
                                    One(p) => {
                                        let item_prop = ItemProp::from_activation_condition_prop(p);
                                        view.panel
                                            .mapping_header_panel
                                            .invalidate_due_to_changed_prop(item_prop, initiator);
                                    }
                                },
                                P::InSource(p) => match p {
                                    Multiple => {
                                        view.invalidate_source_controls();
                                        view.invalidate_mode_controls();
                                    }
                                    One(p) => {
                                        use SourceProp as P;
                                        match p {
                                            P::Category | P::MidiSourceType | P::ReaperSourceType | P::ControlElementType => {
                                                view.invalidate_source_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::Channel => {
                                                view.invalidate_source_control_visibilities();
                                                view.invalidate_source_line_3_combo_box();
                                                view.invalidate_source_line_5_combo_box();
                                            }
                                            P::MidiMessageNumber => {
                                                view.invalidate_source_line_4_combo_box_2();
                                            }
                                            P::ParameterNumberMessageNumber |
                                            P::ControlElementId => {
                                                view.invalidate_source_line_4_edit_control(initiator);
                                            }
                                            P::OscArgIndex => {
                                                view.invalidate_source_line_4(initiator);
                                                view.invalidate_source_line_5(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::CustomCharacter |
                                            P::OscArgTypeTag => {
                                                view.invalidate_source_line_4_combo_box_2();
                                                view.invalidate_source_line_5(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::MidiClockTransportMessage => {
                                                view.invalidate_source_line_3_combo_box();
                                            }
                                            P::IsRegistered => {
                                                view.invalidate_source_line_4_check_box();
                                            }
                                            P::Is14Bit => {
                                                view.invalidate_source_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::DisplayType => {
                                                view.invalidate_source_controls();
                                            }
                                            P::DisplayId => {
                                                view.invalidate_source_line_4_combo_box_2();
                                            }
                                            P::Line => {
                                                view.invalidate_source_line_5_combo_box();
                                            }
                                            P::OscAddressPattern | P::TimerMillis => {
                                                view.invalidate_source_line_3_edit_control(initiator);
                                            }
                                            P::RawMidiPattern => {
                                                view.invalidate_source_line_7_edit_control(initiator);
                                            }
                                            P::ParameterIndex => {
                                                view.invalidate_source_line_3_combo_box()
                                            }
                                            P::MidiScriptKind => {
                                                view.invalidate_source_line_3(initiator);
                                            }
                                            P::MidiScript | P::OscFeedbackArgs => {
                                                view.invalidate_source_line_7_edit_control(initiator);
                                            }
                                            P::OscArgValueRange => {
                                                view.invalidate_source_line_5(initiator);
                                            }
                                            P::OscArgIsRelative => {
                                                view.invalidate_source_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::Keystroke => {
                                                view.invalidate_source_line_3(initiator);
                                            }
                                            P::ButtonIndex => { view.invalidate_source_line_3(initiator); }
                                            P::ButtonBackgroundType | P::ButtonBackgroundImagePath => {
                                                view.invalidate_source_line_4(initiator);
                                            }
                                            P::ButtonForegroundType | P::ButtonForegroundImagePath => {
                                                view.invalidate_source_line_5(initiator);
                                            }
                                            P::ButtonStaticText => {
                                                view.invalidate_source_line_7_edit_control(initiator);
                                            }
                                        }
                                    }
                                }
                                P::InMode(p) => match p {
                                    Multiple => {
                                        view.invalidate_mode_controls();
                                    }
                                    One(p) => {
                                        use ModeProp as P;
                                        match p {
                                            P::AbsoluteMode => {
                                                view.invalidate_mode_controls();
                                            }
                                            P::TargetValueInterval => {
                                                view.invalidate_mode_min_target_value_controls(
                                                    initiator,
                                                );
                                                view.invalidate_mode_max_target_value_controls(
                                                    initiator,
                                                );
                                            }
                                            P::SourceValueInterval => {
                                                view.invalidate_mode_source_value_controls(initiator);
                                            }
                                            P::Reverse => {
                                                view.invalidate_mode_reverse_check_box();
                                            }
                                            P::FireMode => {
                                                view.invalidate_mode_controls_internal(initiator);
                                            }
                                            P::PressDurationInterval | P::TurboRate => {
                                                view.invalidate_mode_fire_controls(initiator);
                                            }
                                            P::OutOfRangeBehavior => {
                                                view.invalidate_mode_out_of_range_behavior_combo_box();
                                            }
                                            P::RoundTargetValue => {
                                                view.invalidate_mode_round_target_value_check_box();
                                            }
                                            P::TakeoverMode => {
                                                view.invalidate_mode_takeover_mode_combo_box();
                                            }
                                            P::ButtonUsage => {
                                                view.invalidate_mode_button_usage_combo_box();
                                            }
                                            P::EncoderUsage => {
                                                view.invalidate_mode_encoder_usage_combo_box();
                                            }
                                            P::EelControlTransformation => {
                                                view.invalidate_mode_controls_internal(initiator);
                                            }
                                            P::EelFeedbackTransformation | P::TextualFeedbackExpression => {
                                                view.invalidate_mode_eel_feedback_transformation_edit_control(initiator);
                                            }
                                            P::StepSizeInterval | P::StepFactorInterval => {
                                                view.invalidate_mode_step_controls(initiator);
                                            }
                                            P::Rotate => {
                                                view.invalidate_mode_rotate_check_box();
                                            }
                                            P::MakeAbsolute => {
                                                view.invalidate_mode_controls();
                                            }
                                            P::GroupInteraction => {
                                                view.invalidate_mode_group_interaction_combo_box();
                                            }
                                            P::TargetValueSequence => {
                                                view.invalidate_mode_target_value_sequence_edit_control(initiator);
                                            }
                                            P::FeedbackType => {
                                                view.invalidate_mode_controls();
                                            }
                                            P::FeedbackColor | P::FeedbackBackgroundColor => {
                                                view.invalidate_mode_feedback_type_button();
                                            }
                                            P::FeedbackValueTable => {
                                                // No representation in GUI at the moment.
                                            }
                                            P::LegacyJumpInterval => {
                                                // Not supported in UI anymore since 2.14.0-pre.10
                                            }
                                        }
                                    }
                                },

                                P::InTarget(p) => match p {
                                    Multiple => {
                                        view.invalidate_target_controls(None);
                                        view.invalidate_mode_controls();
                                    }
                                    One(p) => {
                                        use TargetProp as P;
                                        match p {
                                            P::Category | P::TargetType | P::ControlElementType => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_controls(None);
                                                view.invalidate_mode_controls();
                                            }
                                            P::TrackType | P::TrackIndex | P::TrackId | P::TrackName
                                            | P::TrackExpression | P::BookmarkType | P::BookmarkAnchorType
                                            | P::BookmarkRef | P::TransportAction | P::AnyOnParameter
                                            | P::SmartCommandName | P::ActionScope => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::BrowseTracksMode => {
                                                view.invalidate_target_line_2(initiator);
                                            }
                                            P::MappingSnapshotTypeForLoad | P::MappingSnapshotTypeForTake | P::MappingSnapshotId => {
                                                view.invalidate_target_line_2(initiator);
                                            }
                                            P::MappingSnapshotDefaultValue => {
                                                view.invalidate_target_line_3(initiator);
                                            }
                                            P::ControlElementId => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_line_2(initiator);
                                            }
                                            P::Learnable => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::Unit => {
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::FxType | P::FxIndex | P::FxId | P::FxName | P::FxExpression | P::FxIsInputFx => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::RouteSelectorType | P::RouteType | P::RouteIndex | P::RouteId | P::RouteName | P::RouteExpression => {
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::ParamType | P::ParamName | P::ParamExpression => {
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::ParamIndex => {
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                                // This one is for compartment parameter target
                                                view.invalidate_target_line_2_label_2();
                                            }
                                            P::ActionInvocationType => {
                                                view.invalidate_target_line_3(None);
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::SeekBehavior | P::TouchedTrackParameterType | P::AutomationMode | P::MonitoringMode | P::TrackArea => {
                                                view.invalidate_target_line_3(None);
                                            }
                                            P::SoloBehavior => {
                                                view.invalidate_target_line_3(None);
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::AutomationModeOverrideType => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_line_2_combo_box_2(initiator);
                                                view.invalidate_target_line_3(None);
                                            }
                                            P::FxSnapshot | P::FxDisplayType => {
                                                view.invalidate_target_line_4(None);
                                                view.invalidate_target_value_controls();
                                            }
                                            P::TrackExclusivity => {
                                                view.invalidate_target_line_4(initiator);
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::TrackToolAction | P::FxToolAction => {
                                                view.invalidate_target_line_4(initiator);
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::GroupId => {
                                                view.invalidate_target_line_2(initiator);
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::OscArgIndex |
                                            P::OscArgTypeTag => {
                                                view.invalidate_target_line_4(initiator);
                                                view.invalidate_target_line_5(initiator);
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::OscArgValueRange => {
                                                view.invalidate_target_line_5(initiator);
                                            }
                                            P::MouseActionType => {
                                                view.invalidate_target_controls(initiator);
                                            }
                                            P::PotFilterItemKind => {
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::Axis => {
                                                view.invalidate_target_line_3(initiator);
                                            }
                                            P::MouseButton => {
                                                view.invalidate_target_line_4(initiator);
                                            }
                                            P::ScrollArrangeView | P::SeekPlay => {
                                                view.invalidate_target_check_boxes();
                                                view.invalidate_target_value_controls();
                                            }
                                            P::GangBehavior => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::EnableOnlyIfTrackSelected | P::ScrollMixer | P::MoveView => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::WithTrack => {
                                                view.invalidate_target_controls(None);
                                            }
                                            P::EnableOnlyIfFxHasFocus | P::UseProject => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::UseRegions => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::UseLoopPoints | P::PollForFeedback | P::Retrigger | P::RealTime => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::UseTimeSelection => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::FeedbackResolution => {
                                                view.invalidate_target_line_2_combo_box_1();
                                            }
                                            P::OscAddressPattern => {
                                                view.invalidate_target_line_3(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::RawMidiPattern => {
                                                view.invalidate_target_line_4(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::OscDevId => {
                                                view.invalidate_target_line_2(None);
                                            }
                                            P::SendMidiDestination => {
                                                view.invalidate_target_line_2(None);
                                                view.invalidate_target_line_3(None);
                                            }
                                            P::MidiInputDevice => {
                                                view.invalidate_target_line_3(None);
                                            }
                                            P::Tags => {
                                                view.invalidate_target_line_4_edit_control(initiator);
                                            }
                                            P::Exclusivity => {
                                                view.invalidate_target_line_4_combo_box_1();
                                            }
                                            P::ActiveMappingsOnly => {
                                                view.invalidate_target_check_box_2();
                                            }
                                            P::PlaytimeRow | P::PlaytimeRowAction | P::StopColumnIfSlotEmpty | P::PlaytimeSlot | P::PlaytimeColumn | P::PlaytimeSlotManagementAction | P::PlaytimeSlotTransportAction | P::PlaytimeColumnAction | P::PlaytimeMatrixAction => {
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::TouchedRouteParameterType => {
                                                view.invalidate_target_line_3_combo_box_2();
                                            }
                                            P::MappingModificationKind => {
                                                view.invalidate_target_line_2(initiator);
                                            }
                                            P::MappingRef => {
                                                view.invalidate_target_line_3(initiator);
                                                view.invalidate_target_line_4(initiator);
                                            }
                                            P::IncludedTargets => {
                                                view.invalidate_target_line_2(initiator);
                                            }
                                            P::TouchCause => {
                                                // Not shown in mapping panel at the moment, only
                                                // in egui view (in a non-reactive way).
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
            }
            _ => {}
        }
    }

    fn source_match_indicator_control(&self) -> Window {
        self.view
            .require_control(root::IDC_MAPPING_MATCHED_INDICATOR_TEXT)
    }

    fn toggle_learn_source(&self) {
        let session = self.session();
        session
            .borrow_mut()
            .toggle_learning_source(
                self.session.clone(),
                self.qualified_mapping_id().expect("no mapping displayed"),
            )
            .expect("mapping must exist");
    }

    fn toggle_learn_target(&self) {
        let session = self.session();
        session.borrow_mut().toggle_learning_target(
            self.session.clone(),
            self.qualified_mapping_id().expect("no mapping"),
        );
    }

    fn handle_target_line_2_button_press(self: SharedView<Self>) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let category = mapping.borrow().target_model.category();
        match category {
            TargetCategory::Reaper => {
                let target_type = mapping.borrow().target_model.target_type();
                match target_type {
                    ReaperTargetType::LastTouched => {
                        self.open_learnable_targets_picker(mapping);
                    }
                    ReaperTargetType::ModifyMapping => {
                        self.open_learnable_targets_picker(mapping);
                    }
                    ReaperTargetType::CompartmentParameterValue => {
                        self.pick_compartment_parameter(mapping);
                    }
                    _ => {
                        self.write(|p| p.handle_target_line_2_button_press_internal());
                    }
                }
            }
            TargetCategory::Virtual => {
                let control_element_type =
                    mapping.borrow().target_model.control_element_character();
                let window = self.view.require_window();
                let text = pick_virtual_control_element(
                    window,
                    control_element_type,
                    &HashMap::default(),
                    true,
                    true,
                )
                .ok_or("nothing picked")?;
                let element_id = text.parse().unwrap_or_default();
                self.change_mapping(MappingCommand::ChangeTarget(
                    TargetCommand::SetControlElementId(element_id),
                ));
            }
        };
        Ok(())
    }

    fn open_learnable_targets_picker(self: SharedView<Self>, mapping: SharedMapping) {
        #[cfg(not(feature = "egui"))]
        {
            let _ = mapping;
            crate::infrastructure::ui::util::alert_feature_not_available();
        }
        #[cfg(feature = "egui")]
        {
            let value = {
                let mapping = mapping.borrow();
                crate::infrastructure::ui::egui_views::target_filter_panel::Value {
                    included_targets: mapping.target_model.included_targets().clone(),
                    touch_cause: mapping.target_model.touch_cause(),
                }
            };
            let session = self.session.clone();
            let panel = crate::infrastructure::ui::TargetFilterPanel::new(value, move |value| {
                let mut mapping = mapping.borrow_mut();
                UnitModel::change_mapping_from_ui_simple(
                    session.clone(),
                    &mut mapping,
                    MappingCommand::ChangeTarget(TargetCommand::SetLearnableTargetKinds(
                        value.included_targets,
                    )),
                    None,
                );
                UnitModel::change_mapping_from_ui_simple(
                    session.clone(),
                    &mut mapping,
                    MappingCommand::ChangeTarget(TargetCommand::SetTouchCause(value.touch_cause)),
                    None,
                );
            });
            self.open_extra_panel(panel);
        }
    }

    fn pick_compartment_parameter(&self, mapping: SharedMapping) {
        let menu = {
            let mapping = mapping.borrow();
            let param_index = mapping.target_model.compartment_param_index();
            menus::menu_containing_realearn_params(
                &self.session,
                mapping.compartment(),
                param_index,
            )
        };
        let result = self
            .view
            .require_window()
            .open_popup_menu(menu, Window::cursor_pos());
        if let Some(param_index) = result {
            self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetParamIndex(
                param_index.get(),
            )));
        }
    }

    fn pick_target_midi_input_device(&self, mapping: SharedMapping) {
        let menu = {
            let mapping = mapping.borrow();
            let current_value = mapping.target_model.midi_input_device();
            menus::midi_device_input_menu(current_value, SAME_AS_INPUT_DEV)
        };
        let result = self
            .view
            .require_window()
            .open_popup_menu(menu, Window::cursor_pos());
        if let Some(midi_input_device) = result {
            self.change_mapping(MappingCommand::ChangeTarget(
                TargetCommand::SetMidiInputDevice(midi_input_device),
            ));
        }
    }

    fn handle_target_line_3_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let target_type = mapping.borrow().target_model.target_type();
        match target_type {
            ReaperTargetType::SendMidi => {
                self.pick_target_midi_input_device(mapping);
            }
            ReaperTargetType::ModifyMapping => {
                let menu = {
                    let mapping = mapping.borrow();
                    let current_other_session_id = mapping.target_model.mapping_ref().session_id();
                    let session = self.session();
                    let session = session.borrow();
                    menus::menu_containing_sessions(&session, current_other_session_id)
                };
                let new_other_session_id = self
                    .view
                    .require_window()
                    .open_popup_menu(menu, Window::cursor_pos());
                if let Some(new_other_session_id) = new_other_session_id {
                    // Chosen something in the menu
                    let new_mapping_ref = if let Some(session_id) = new_other_session_id {
                        // Chosen another instance
                        MappingRefModel::ForeignMapping {
                            session_id,
                            mapping_key: None,
                        }
                    } else {
                        // Chosen this instance
                        MappingRefModel::OwnMapping { mapping_id: None }
                    };
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMappingRef(new_mapping_ref),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_source_line_4_button_press(&self) -> Result<(), &'static str> {
        let m = self.displayed_mapping().ok_or("no mapping set")?;
        let source_category = m.borrow().source_model.category();
        match source_category {
            SourceCategory::StreamDeck => {
                let current_value = m.borrow().source_model.button_background_type();
                let menu =
                    build_enum_variants_menu(StreamDeckButtonBackgroundType::iter(), current_value);
                if let Some(new_value) = self
                    .view
                    .require_window()
                    .open_popup_menu(menu, Window::cursor_pos())
                {
                    self.change_mapping(MappingCommand::ChangeSource(
                        SourceCommand::SetButtonBackgroundType(new_value),
                    ));
                    if new_value.wants_image() {
                        let current_file = m
                            .borrow()
                            .source_model
                            .button_background_image_path()
                            .to_path_buf();
                        if let Some(new_file_name) = pick_png_file(current_file) {
                            self.change_mapping(MappingCommand::ChangeSource(
                                SourceCommand::SetButtonBackgroundImagePath(new_file_name),
                            ));
                        }
                    }
                }
            }
            SourceCategory::Virtual => {
                let (control_element_type, control_enabled, feedback_enabled) = {
                    let m = m.borrow();
                    (
                        m.source_model.control_element_character(),
                        m.control_is_enabled(),
                        m.feedback_is_enabled(),
                    )
                };
                let window = self.view.require_window();
                let controller_mappings: Vec<_> = {
                    let session = self.session();
                    let session = session.borrow();
                    session
                        .mappings(CompartmentKind::Controller)
                        .cloned()
                        .collect()
                };
                let grouped_mappings =
                    group_mappings_by_virtual_control_element(controller_mappings.iter());
                let text = pick_virtual_control_element(
                    window,
                    control_element_type,
                    &grouped_mappings,
                    control_enabled,
                    feedback_enabled,
                )
                .ok_or("nothing picked")?;
                let control_element_id = text.parse().unwrap_or_default();
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetControlElementId(control_element_id),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_source_line_5_button_press(&self) -> Result<(), &'static str> {
        let m = self.displayed_mapping().ok_or("no mapping set")?;
        let source_category = m.borrow().source_model.category();
        match source_category {
            SourceCategory::StreamDeck => {
                let current_value = m.borrow().source_model.button_foreground_type();
                let menu =
                    build_enum_variants_menu(StreamDeckButtonForegroundType::iter(), current_value);
                if let Some(new_value) = self
                    .view
                    .require_window()
                    .open_popup_menu(menu, Window::cursor_pos())
                {
                    self.change_mapping(MappingCommand::ChangeSource(
                        SourceCommand::SetButtonForegroundType(new_value),
                    ));
                    if new_value.wants_image() {
                        let current_file = m
                            .borrow()
                            .source_model
                            .button_foreground_image_path()
                            .to_path_buf();
                        if let Some(new_file_name) = pick_png_file(current_file) {
                            self.change_mapping(MappingCommand::ChangeSource(
                                SourceCommand::SetButtonForegroundImagePath(new_file_name),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn feedback_type_button_pressed(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let current_color = mapping.borrow().mode_model.feedback_color().cloned();
        let current_background_color = mapping
            .borrow()
            .mode_model
            .feedback_background_color()
            .cloned();
        let result = show_feedback_popup_menu(
            self.view.require_window(),
            current_color,
            current_background_color,
        )?;
        match result {
            FeedbackPopupMenuResult::EditMultiLine => {
                self.edit_feedback_descriptor();
            }
            FeedbackPopupMenuResult::ChangeColor(instruction) => {
                let cmd = match instruction.target {
                    ColorTarget::Color => ModeCommand::SetFeedbackColor(instruction.color),
                    ColorTarget::BackgroundColor => {
                        ModeCommand::SetFeedbackBackgroundColor(instruction.color)
                    }
                };
                self.change_mapping(MappingCommand::ChangeMode(cmd));
            }
        }
        Ok(())
    }

    fn open_detailed_help_for_current_ui_element(&self) -> Option<()> {
        let topic = self.active_help_topic.borrow().get()?;
        let help_url = topic.get_details()?;
        open_in_browser(&help_url.doc_url);
        Some(())
    }

    fn change_mapping(&self, val: MappingCommand) {
        self.change_mapping_with_initiator(val, None);
    }

    fn change_mapping_with_initiator(&self, val: MappingCommand, initiator: Option<u32>) {
        let session = self.session();
        let mut session = session.borrow_mut();
        let mapping = self.displayed_mapping().expect("no mapping set");
        let mut mapping = mapping.borrow_mut();
        session.change_mapping_from_ui_expert(&mut mapping, val, initiator, self.session.clone());
    }

    fn notify_user_on_anyhow_error(&self, result: anyhow::Result<()>) {
        if let Err(e) = result {
            self.notify_user_about_anyhow_error(e);
        }
    }

    fn notify_user_about_anyhow_error(&self, e: anyhow::Error) {
        self.view
            .require_window()
            .alert("ReaLearn", format!("{e:#}"));
    }

    fn handle_target_line_4_button_press(&self) -> anyhow::Result<()> {
        let mapping = self.displayed_mapping().context("no mapping set")?;
        let target_type = mapping.borrow().target_model.target_type();
        match target_type {
            ReaperTargetType::Action => {
                let reaper = Reaper::get().medium_reaper();
                use InitialAction::*;
                let initial_action = match mapping.borrow().target_model.resolve_action() {
                    None => NoneSelected,
                    Some(a) => Selected(a.command_id()?),
                };
                let action_scope = mapping.borrow().target_model.action_scope();
                let section_id = SectionId::new(action_scope.section_id());
                // TODO-low Add this to reaper-high with rxRust
                if reaper.low().pointers().PromptForAction.is_none() {
                    bail!("Please update to REAPER >= 6.12 in order to pick actions!");
                }
                reaper.prompt_for_action_create(initial_action, section_id);
                let shared_mapping = self.mapping();
                let weak_session = self.session.clone();
                Global::control_surface_rx()
                    .main_thread_idle()
                    .take_until(self.party_is_over())
                    .map(move |_| {
                        Reaper::get()
                            .medium_reaper()
                            .prompt_for_action_poll(section_id)
                    })
                    .filter(|r| *r != PromptForActionResult::NoneSelected)
                    .take_while(|r| *r != PromptForActionResult::ActionWindowGone)
                    .subscribe_complete(
                        move |r| {
                            if let PromptForActionResult::Selected(command_id) = r {
                                let session = weak_session.upgrade().expect("session gone");
                                let action = Reaper::get()
                                    .main_section()
                                    .action_by_command_id(command_id);
                                let mut mapping = shared_mapping.borrow_mut();
                                let cmd = MappingCommand::ChangeTarget(
                                    TargetCommand::SetSmartCommandName(
                                        build_smart_command_name_from_action(&action),
                                    ),
                                );
                                session.borrow_mut().change_mapping_from_ui_expert(
                                    &mut mapping,
                                    cmd,
                                    None,
                                    weak_session.clone(),
                                );
                            }
                        },
                        move || {
                            Reaper::get()
                                .medium_reaper()
                                .prompt_for_action_finish(section_id);
                        },
                    );
            }
            ReaperTargetType::LoadFxSnapshot => {
                // Important that neither session nor mapping is mutably borrowed while doing this
                // because state of our ReaLearn instance is not unlikely to be
                // queried as well!
                let compartment = mapping.borrow().compartment();
                let fx_snapshot = mapping
                    .borrow()
                    .target_model
                    .take_fx_snapshot(self.session().borrow().extended_context(), compartment)?;
                self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetFxSnapshot(
                    Some(fx_snapshot),
                )));
            }
            ReaperTargetType::ModifyMapping => {
                let (compartment, current_mapping_ref) = {
                    let mapping = mapping.borrow();
                    (
                        mapping.compartment(),
                        mapping.target_model.mapping_ref().clone(),
                    )
                };
                let new_mapping_ref = match current_mapping_ref {
                    MappingRefModel::OwnMapping { mapping_id } => {
                        let menu =
                            menus::menu_containing_mappings(&self.session, compartment, mapping_id);
                        let mapping_id = self
                            .view
                            .require_window()
                            .open_popup_menu(menu, Window::cursor_pos());
                        mapping_id.map(|mapping_id| MappingRefModel::OwnMapping { mapping_id })
                    }
                    MappingRefModel::ForeignMapping {
                        session_id,
                        mapping_key,
                    } => {
                        let session = BackboneShell::get()
                            .find_unit_model_by_key(&session_id)
                            .context("session not found")?;
                        let mapping_id = {
                            mapping_key.and_then(|key| {
                                session.borrow().find_mapping_id_by_key(compartment, &key)
                            })
                        };
                        let menu = menus::menu_containing_mappings(
                            &Rc::downgrade(&session),
                            compartment,
                            mapping_id,
                        );
                        let mapping_id = self
                            .view
                            .require_window()
                            .open_popup_menu(menu, Window::cursor_pos());
                        mapping_id.map(|mapping_id| {
                            let mapping_key = mapping_id.map(|mapping_id| {
                                let session = session.borrow();
                                let mapping = session
                                    .find_mapping_by_id(compartment, mapping_id)
                                    .expect("currently picked mapping not found");
                                let mapping = mapping.borrow();
                                mapping.key().clone()
                            });
                            MappingRefModel::ForeignMapping {
                                session_id,
                                mapping_key,
                            }
                        })
                    }
                };
                if let Some(new_mapping_ref) = new_mapping_ref {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMappingRef(new_mapping_ref),
                    ));
                }
            }
            ReaperTargetType::SendMidi => {
                if let Some(action) = open_send_midi_menu(self.view.require_window()) {
                    match action {
                        SendMidiMenuAction::PitchBendChangePreset { channel } => {
                            let status_byte: u8 = ShortMessageType::PitchBendChange.into();
                            let msg =
                                format!("{:02X} [0gfe dcba] [0nml kjih]", status_byte + channel);
                            self.change_mapping(MappingCommand::ChangeTarget(
                                TargetCommand::SetRawMidiPattern(msg),
                            ));
                        }
                        SendMidiMenuAction::DoubleDataBytePreset {
                            channel,
                            msg_type,
                            i,
                        } => {
                            let status_byte: u8 = msg_type.into();
                            let msg =
                                format!("{:02X} {:02X} [0gfe dcba]", status_byte + channel, i);
                            self.change_mapping(MappingCommand::ChangeTarget(
                                TargetCommand::SetRawMidiPattern(msg),
                            ));
                        }
                        SendMidiMenuAction::SingleDataBytePreset {
                            channel,
                            msg_type,
                            last_byte,
                        } => {
                            let status_byte: u8 = msg_type.into();
                            let msg = format!(
                                "{:02X} [0gfe dcba] {:02X}",
                                status_byte + channel,
                                last_byte
                            );
                            self.change_mapping(MappingCommand::ChangeTarget(
                                TargetCommand::SetRawMidiPattern(msg),
                            ));
                        }
                        SendMidiMenuAction::EditMultiLine => {
                            let session = self.session.clone();
                            let engine = Box::new(RawMidiScriptEngine);
                            let help_url =
                                "https://docs.helgoboss.org/realearn/goto#target-midi-send-message";
                            self.edit_script_in_simple_editor(
                                engine,
                                help_url,
                                |m| m.target_model.raw_midi_pattern().to_owned(),
                                move |m, text| {
                                    UnitModel::change_mapping_from_ui_simple(
                                        session.clone(),
                                        m,
                                        MappingCommand::ChangeTarget(
                                            TargetCommand::SetRawMidiPattern(text),
                                        ),
                                        None,
                                    );
                                },
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn is_free(&self) -> bool {
        self.mapping.borrow().is_none()
    }

    pub fn mapping_ptr(&self) -> *const MappingModel {
        match self.mapping.borrow().as_ref() {
            None => null(),
            Some(m) => m.as_ptr() as _,
        }
    }

    fn qualified_mapping_id(&self) -> Option<QualifiedMappingId> {
        let mapping = self.mapping.borrow();
        let mapping = mapping.as_ref()?;
        let mapping = mapping.borrow();
        Some(mapping.qualified_id())
    }

    pub fn force_scroll_to_mapping_in_main_panel(&self) {
        if let Some(id) = self.qualified_mapping_id() {
            self.main_panel().force_scroll_to_mapping(id);
        }
    }

    fn main_panel(&self) -> SharedView<UnitPanel> {
        self.main_panel.upgrade().expect("main view gone")
    }

    fn handle_source_line_7_button_press(&self) {
        let mapping = self.mapping();
        let category = mapping.borrow().source_model.category();
        match category {
            SourceCategory::Midi => {
                let source_type = mapping.borrow().source_model.midi_source_type();
                match source_type {
                    MidiSourceType::Raw => {
                        let session = self.session.clone();
                        let engine = Box::new(RawMidiScriptEngine);
                        let help_url = "https://docs.helgoboss.org/realearn/goto#source-midi-raw";
                        self.edit_script_in_simple_editor(
                            engine,
                            help_url,
                            |m| m.source_model.raw_midi_pattern().to_owned(),
                            move |m, text| {
                                UnitModel::change_mapping_from_ui_simple(
                                    session.clone(),
                                    m,
                                    MappingCommand::ChangeSource(SourceCommand::SetRawMidiPattern(
                                        text,
                                    )),
                                    None,
                                );
                            },
                        );
                    }
                    MidiSourceType::Script => {
                        let session = self.session.clone();
                        self.edit_midi_source_script_internal(
                            |m| m.source_model.midi_script().to_owned(),
                            move |m, eel| {
                                UnitModel::change_mapping_from_ui_simple(
                                    session.clone(),
                                    m,
                                    MappingCommand::ChangeSource(SourceCommand::SetMidiScript(eel)),
                                    None,
                                );
                            },
                        );
                    }
                    _ => {}
                }
            }
            SourceCategory::Osc => {
                let session = self.session.clone();
                self.edit_osc_feedback_arguments_internal(
                    |m| format_osc_feedback_args(m.source_model.osc_feedback_args()),
                    move |m, text| {
                        let args = parse_osc_feedback_args(&text);
                        UnitModel::change_mapping_from_ui_simple(
                            session.clone(),
                            m,
                            MappingCommand::ChangeSource(SourceCommand::SetOscFeedbackArgs(args)),
                            None,
                        );
                    },
                );
            }
            SourceCategory::StreamDeck => {
                let session = self.session.clone();
                self.edit_stream_deck_static_text_internal(
                    |m| m.source_model.button_static_text().to_string(),
                    move |m, text| {
                        UnitModel::change_mapping_from_ui_simple(
                            session.clone(),
                            m,
                            MappingCommand::ChangeSource(SourceCommand::SetButtonStaticText(text)),
                            None,
                        );
                    },
                );
            }
            _ => {}
        }
    }

    fn edit_control_transformation(&self) {
        let session = self.session.clone();
        let engine = Box::new(EelControlTransformationEngine);
        let help_url = "https://docs.helgoboss.org/realearn/goto#control-transformation";
        let get_value = |m: &MappingModel| m.mode_model.eel_control_transformation().to_owned();
        let set_value = move |m: &mut MappingModel, eel: String| {
            UnitModel::change_mapping_from_ui_simple(
                session.clone(),
                m,
                MappingCommand::ChangeMode(ModeCommand::SetEelControlTransformation(eel)),
                None,
            );
        };
        #[cfg(feature = "egui")]
        self.edit_script_in_advanced_editor(engine, help_url, get_value, set_value);
        #[cfg(not(feature = "egui"))]
        self.edit_script_in_simple_editor(engine, help_url, get_value, set_value);
    }

    fn edit_feedback_descriptor(&self) {
        let mapping = self.mapping();
        let feedback_type = mapping.borrow().mode_model.feedback_type();
        match feedback_type {
            FeedbackType::Numeric => self.edit_feedback_transformation(),
            FeedbackType::Text => self.edit_textual_feedback_expression(),
            FeedbackType::Dynamic => self.edit_lua_feedback_script(),
        }
    }

    fn edit_feedback_transformation(&self) {
        let session = self.session.clone();
        self.edit_script_in_simple_editor(
            Box::new(EelFeedbackTransformationEngine),
            "https://docs.helgoboss.org/realearn/goto#numeric-feedback-type",
            |m| m.mode_model.eel_feedback_transformation().to_owned(),
            move |m, eel| {
                UnitModel::change_mapping_from_ui_simple(
                    session.clone(),
                    m,
                    MappingCommand::ChangeMode(ModeCommand::SetEelFeedbackTransformation(eel)),
                    None,
                );
            },
        );
    }

    fn edit_textual_feedback_expression(&self) {
        let session = self.session.clone();
        self.edit_script_in_simple_editor(
            Box::new(TextualFeedbackExpressionEngine),
            "https://docs.helgoboss.org/realearn/goto#text-feedback",
            |m| m.mode_model.textual_feedback_expression().to_owned(),
            move |m, eel| {
                UnitModel::change_mapping_from_ui_simple(
                    session.clone(),
                    m,
                    MappingCommand::ChangeMode(ModeCommand::SetTextualFeedbackExpression(eel)),
                    None,
                );
            },
        );
    }

    fn edit_lua_feedback_script(&self) {
        let session = self.session.clone();
        self.edit_script_in_simple_editor(
            Box::new(LuaFeedbackScriptEngine::new()),
            "https://docs.helgoboss.org/realearn/goto#dynamic-feedback",
            |m| m.mode_model.textual_feedback_expression().to_owned(),
            move |m, eel| {
                UnitModel::change_mapping_from_ui_simple(
                    session.clone(),
                    m,
                    MappingCommand::ChangeMode(ModeCommand::SetTextualFeedbackExpression(eel)),
                    None,
                );
            },
        );
    }

    fn edit_osc_feedback_arguments_internal(
        &self,
        get_initial_value: impl Fn(&MappingModel) -> String,
        apply: impl Fn(&mut MappingModel, String) + 'static,
    ) {
        let engine = Box::new(OscFeedbackArgumentsEngine);
        let help_url = "https://docs.helgoboss.org/realearn/goto#osc-feedback-arguments-expression";
        self.edit_script_in_simple_editor(engine, help_url, get_initial_value, apply);
    }

    fn edit_stream_deck_static_text_internal(
        &self,
        get_initial_value: impl Fn(&MappingModel) -> String,
        apply: impl Fn(&mut MappingModel, String) + 'static,
    ) {
        let engine = Box::new(PlainTextEngine);
        let help_url = "https://docs.helgoboss.org/realearn/sources/stream-deck.html#linux";
        self.edit_script_in_simple_editor(engine, help_url, get_initial_value, apply);
    }

    fn edit_midi_source_script_internal(
        &self,
        get_initial_value: impl Fn(&MappingModel) -> String,
        apply: impl Fn(&mut MappingModel, String) + 'static,
    ) {
        let mapping = self.mapping();
        let engine: Box<dyn ScriptEngine> = match mapping.borrow().source_model.midi_script_kind() {
            MidiScriptKind::Eel => Box::new(EelMidiScriptEngine),
            MidiScriptKind::Lua => Box::new(LuaMidiScriptEngine::new()),
        };
        let help_url = "https://docs.helgoboss.org/realearn/goto#source-midi-script";
        self.edit_script_in_simple_editor(engine, help_url, get_initial_value, apply);
    }

    fn edit_script_in_simple_editor(
        &self,
        engine: Box<dyn ScriptEngine>,
        help_url: &'static str,
        get_initial_content: impl Fn(&MappingModel) -> String,
        apply: impl Fn(&mut MappingModel, String) + 'static,
    ) {
        let mapping = self.mapping();
        let weak_mapping = Rc::downgrade(&mapping);
        let initial_content = { get_initial_content(&mapping.borrow()) };
        let input = ScriptEditorInput {
            initial_value: initial_content,
            engine,
            help_url,
            set_value: move |edited_script| {
                let m = match weak_mapping.upgrade() {
                    None => return,
                    Some(m) => m,
                };
                apply(&mut m.borrow_mut(), edited_script);
            },
        };
        let editor = SimpleScriptEditorPanel::new(input);
        self.open_extra_panel(editor);
    }

    #[cfg(feature = "egui")]
    fn edit_script_in_advanced_editor(
        &self,
        engine: Box<dyn ScriptEngine>,
        help_url: &'static str,
        get_initial_content: impl Fn(&MappingModel) -> String,
        apply: impl Fn(&mut MappingModel, String) + 'static,
    ) {
        let mapping = self.mapping();
        let weak_mapping = Rc::downgrade(&mapping);
        let initial_content = { get_initial_content(&mapping.borrow()) };
        let input = ScriptEditorInput {
            initial_value: initial_content,
            engine,
            help_url,
            set_value: move |edited_script| {
                let m = match weak_mapping.upgrade() {
                    None => return,
                    Some(m) => m,
                };
                apply(&mut m.borrow_mut(), edited_script);
            },
        };
        let editor = crate::infrastructure::ui::AdvancedScriptEditorPanel::new(
            input,
            crate::infrastructure::ui::CONTROL_TRANSFORMATION_TEMPLATES,
        );
        self.open_extra_panel(editor);
    }

    fn open_extra_panel(&self, panel: impl View + 'static) {
        open_child_panel_dyn(&self.extra_panel, panel, self.view.require_window());
    }

    fn close_open_child_windows(&self) {
        close_child_panel_if_open(&self.extra_panel);
    }

    fn edit_yaml(
        &self,
        get_initial_value: impl Fn(&MappingModel) -> Option<serde_yaml::Mapping>,
        apply: impl Fn(&mut MappingModel, Option<serde_yaml::Mapping>) -> Result<(), String> + 'static,
    ) {
        let mapping = self.mapping();
        let weak_mapping = Rc::downgrade(&mapping);
        let initial_value = { get_initial_value(&mapping.borrow()) };
        let editor = YamlEditorPanel::new(initial_value, move |yaml_mapping| {
            let m = match weak_mapping.upgrade() {
                None => return,
                Some(m) => m,
            };
            let result = apply(&mut m.borrow_mut(), yaml_mapping);
            if let Err(e) = result {
                notification::alert(format!(
                    "Your changes have been applied and saved but they contain the following error and therefore won't have any effect:\n\n{e}"
                ));
            };
        });
        self.open_extra_panel(editor);
    }

    fn edit_advanced_settings(&self) {
        let session = self.session.clone();
        self.edit_yaml(
            |m| m.advanced_settings().cloned(),
            move |m, yaml| {
                let session = session.upgrade().expect("session gone");
                let result = session.borrow_mut().change_mapping_with_closure(
                    m,
                    None,
                    Rc::downgrade(&session),
                    |ctx| ctx.mapping.set_advanced_settings(yaml),
                );
                result
            },
        );
    }

    pub fn handle_matched_mapping(self: SharedView<Self>) {
        self.source_match_indicator_control().enable();
        self.view
            .require_window()
            .set_timer(SOURCE_MATCH_INDICATOR_TIMER_ID, Duration::from_millis(50));
    }

    pub fn handle_target_control_event(self: SharedView<Self>, event: TargetControlEvent) {
        self.invoke_programmatically(|| {
            let body = format!("Control: {} ({})", event.log_entry, event.log_context);
            self.set_simple_help_text(Side::Left, ACTIVITY_INFO, &body);
        });
    }

    pub fn handle_source_feedback_event(self: SharedView<Self>, event: SourceFeedbackEvent) {
        self.invoke_programmatically(|| {
            let body = format!("Feedback: {}", event.log_entry);
            self.set_simple_help_text(Side::Right, ACTIVITY_INFO, &body);
        });
    }

    fn set_simple_help_text(&self, side: Side, title: &str, body: &str) {
        self.view
            .require_control(root::ID_MAPPING_HELP_LEFT_SUBJECT_LABEL)
            .set_text(title);
        let label_control_id = match side {
            Side::Left => root::ID_MAPPING_HELP_LEFT_CONTENT_LABEL,
            Side::Right => root::ID_MAPPING_HELP_RIGHT_CONTENT_LABEL,
        };
        self.view.require_control(label_control_id).set_text(body);
    }

    pub fn handle_changed_target_value(
        self: SharedView<Self>,
        targets: &[CompoundMappingTarget],
        new_value: AbsoluteValue,
    ) {
        self.invoke_programmatically(|| {
            let session = self.session();
            let session = session.borrow();
            invalidate_target_controls_free(
                // We use the target only to derive some characteristics. When having multiple
                // targets, they should all share the same characteristics, so we can just take
                // the first one.
                targets.first(),
                self.view
                    .require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL),
                self.view
                    .require_control(root::ID_TARGET_VALUE_EDIT_CONTROL),
                self.view.require_control(root::ID_TARGET_VALUE_TEXT),
                new_value,
                None,
                root::ID_TARGET_VALUE_EDIT_CONTROL,
                true,
                false,
                self.displayed_mapping()
                    .map(|m| m.borrow().target_model.unit())
                    .unwrap_or_default(),
                session.control_context(),
            );
        });
    }

    pub fn handle_changed_conditions(self: SharedView<Self>) {
        self.clone().invoke_programmatically(|| {
            let _ = self.read(|view| {
                // These changes can happen because of removals (e.g. project close, FX deletions,
                // track deletions etc.). We want to update whatever is possible. But if the own
                // project is missing, this was a project close and we don't need to do anything
                // at all.
                if !view.target_with_context().project().is_available() {
                    return;
                }
                view.invalidate_target_controls(None);
                view.invalidate_mode_controls();
            });
        });
    }

    pub fn notify_parameters_changed(
        self: SharedView<Self>,
        session: &UnitModel,
    ) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping")?;
        let mapping = mapping.borrow();
        self.invoke_programmatically(|| {
            invalidate_target_line_2_expression_result(
                &mapping.target_model,
                session.extended_context(),
                self.view.require_control(root::ID_TARGET_LINE_2_LABEL_3),
                mapping.compartment(),
            );
            invalidat_target_line_3_expression_result(
                &mapping.target_model,
                session.extended_context(),
                self.view.require_control(root::ID_TARGET_LINE_3_LABEL_3),
                mapping.compartment(),
            );
            invalidate_target_line_4_expression_result(
                &mapping.target_model,
                session.extended_context(),
                self.view.require_control(root::ID_TARGET_LINE_4_LABEL_3),
                mapping.compartment(),
            );
        });
        Ok(())
    }

    pub fn hide(&self) {
        self.stop_party();
        self.view.require_window().hide();
        self.mapping.replace(None);
        self.close_open_child_windows();
        self.mapping_header_panel.clear_item();
    }

    pub fn navigate_in_mappings(
        self: SharedView<Self>,
        direction: isize,
    ) -> Result<(), &'static str> {
        let current_id = self.qualified_mapping_id().ok_or("no current mapping")?;
        let session = self.session();
        let session = session.borrow();
        let main_panel = self.main_panel();
        let main_state = main_panel.state();
        let main_state = main_state.borrow();
        let mappings: Vec<&SharedMapping> = MappingRowsPanel::filtered_mappings(
            &session,
            &main_state,
            current_id.compartment,
            false,
        )
        .collect();
        let current_index = mappings
            .iter()
            .position(|m| m.borrow().id() == current_id.id)
            .ok_or("current mapping not found in filtered list")?;
        let new_index =
            (current_index as isize + direction).rem_euclid(mappings.len() as isize) as usize;
        let new_mapping = mappings.get(new_index).ok_or("new mapping not found")?;
        self.show((**new_mapping).clone());
        Ok(())
    }

    pub fn show(self: SharedView<Self>, mapping: SharedMapping) {
        self.invoke_programmatically(|| {
            self.stop_party();
            self.mapping.replace(Some(mapping.clone()));
            self.clone().start_party();
            self.mapping_header_panel.clone().set_item(mapping);
            self.bring_to_foreground();
        });
    }

    pub fn bring_to_foreground(&self) {
        let window = self.view.require_window();
        window.hide();
        window.show();
    }

    /// If you know a function in this view can be invoked by something else than the dialog
    /// process, wrap your function body with this. Basically all pub functions!
    ///
    /// This prevents edit control text change events fired by windows to be processed.
    /// It also prevents the function from being executed if the window is not open anymore,
    /// which can happen when closing things.
    fn invoke_programmatically(&self, f: impl FnOnce()) {
        if self.view.window().is_none() {
            return;
        }
        self.set_invoked_programmatically(true);
        scopeguard::defer! { self.set_invoked_programmatically(false); }
        f()
    }

    /// Unregisters listeners.
    fn stop_party(&self) {
        self.party_is_over_subject.borrow_mut().next(());
    }

    /// Invalidates everything and registers listeners.
    fn start_party(self: SharedView<Self>) {
        self.read(|p| {
            p.clear_help();
            p.fill_all_controls();
            p.invalidate_all_controls();
            p.register_listeners();
        })
        .expect("mapping must be filled at this point");
    }

    fn session(&self) -> SharedUnitModel {
        self.session.upgrade().expect("session gone")
    }

    pub fn displayed_mapping(&self) -> Option<SharedMapping> {
        self.mapping.borrow().clone()
    }

    fn mapping(&self) -> SharedMapping {
        self.displayed_mapping().expect("mapping not filled")
    }

    // TODO-low I think MappingHeaderPanel has a better solution. Maybe refactor.
    fn read<R>(
        self: SharedView<Self>,
        op: impl Fn(&ImmutableMappingPanel) -> R,
    ) -> Result<R, &'static str> {
        let shared_session = self.session();
        let session = shared_session.borrow();
        let shared_mapping = self.mapping.borrow();
        let shared_mapping = shared_mapping.as_ref().ok_or("mapping not filled")?;
        let mapping = shared_mapping.borrow();
        let p = ImmutableMappingPanel {
            session: &session,
            mapping: &mapping,
            source: &mapping.source_model,
            mode: &mapping.mode_model,
            target: &mapping.target_model,
            view: &self.view,
            panel: &self,
        };
        Ok(op(&p))
    }

    fn open_target_menu(self: SharedView<Self>) {
        enum MenuAction {
            SetTarget(Box<ReaperTarget>),
            GoToTarget,
        }
        let menu = {
            use swell_ui::menu_tree::*;
            let mapping = self.mapping();
            let session = self.session();
            let session = session.borrow();
            let compartment = mapping.borrow().compartment();
            let context = session.extended_context();
            let recently_touched_items = Backbone::get()
                .extract_last_touched_targets()
                .into_iter()
                .rev()
                .map(|t| {
                    let mut target_model = TargetModel::default_for_compartment(compartment);
                    let _ = target_model.apply_from_target(&t, context, compartment);
                    // let target_type_label = ReaperTargetType::from_target(&t);
                    let target_label = TargetModelFormatVeryShort(&target_model);
                    // let label = format!("{target_type_label} / {target_label}");
                    item(target_label.to_string(), MenuAction::SetTarget(Box::new(t)))
                })
                .collect();
            anonymous_menu(vec![
                menu(
                    "Pick recently touched target (by type)",
                    recently_touched_items,
                ),
                item("Go to target (if supported)", MenuAction::GoToTarget),
            ])
        };
        let menu_action = self
            .view
            .require_window()
            .open_popup_menu(menu, Window::cursor_pos());
        let Some(menu_action) = menu_action else {
            return;
        };
        self.write(|panel| match &menu_action {
            MenuAction::SetTarget(t) => {
                panel.set_mapping_target(t);
            }
            MenuAction::GoToTarget => {
                panel.go_to_target();
            }
        });
    }

    fn show_target_type_menu(&self) {
        let window = self.view.require_window();
        let (target_category, target_type, control_element_type) = {
            let mapping = self.mapping();
            let mapping = mapping.borrow();
            (
                mapping.target_model.category(),
                mapping.target_model.target_type(),
                mapping.target_model.control_element_character(),
            )
        };
        match target_category {
            TargetCategory::Reaper => {
                let menu = menus::reaper_target_type_menu(target_type);
                if let Some(t) = window.open_popup_menu(menu, Window::cursor_pos()) {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTargetType(t),
                    ));
                }
            }
            TargetCategory::Virtual => {
                let menu = menus::virtual_control_element_character_menu(control_element_type);
                if let Some(t) = window.open_popup_menu(menu, Window::cursor_pos()) {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetControlElementCharacter(t),
                    ));
                }
            }
        }
    }

    fn write<R>(self: SharedView<Self>, op: impl Fn(&mut MutableMappingPanel) -> R) -> R {
        let shared_session = self.session();
        let mut session = shared_session.borrow_mut();
        let mut shared_mapping = self.mapping.borrow_mut();
        let shared_mapping = shared_mapping.as_mut().expect("mapping not filled");
        let mut mapping = shared_mapping.borrow_mut();
        let mut p = MutableMappingPanel {
            session: &mut session,
            mapping: &mut mapping,
            panel: &self,
            view: &self.view,
        };
        op(&mut p)
    }

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }

    fn init_controls(&self) {
        let view = &self.view;
        let sliders = WindowCache {
            mode_min_target_value: view
                .require_control(root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL),
            mode_max_target_value: view
                .require_control(root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL),
            mode_min_source_value: view
                .require_control(root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL),
            mode_max_source_value: view
                .require_control(root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL),
            mode_min_step_size: view
                .require_control(root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL),
            mode_max_step_size: view
                .require_control(root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL),
            mode_fire_line_2: view.require_control(root::ID_MODE_FIRE_LINE_2_SLIDER_CONTROL),
            mode_fire_line_3: view.require_control(root::ID_MODE_FIRE_LINE_3_SLIDER_CONTROL),
            target_value: view.require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL),
        };
        self.window_cache.replace(Some(sliders));
        let indicator = self
            .view
            .require_control(root::IDC_MAPPING_MATCHED_INDICATOR_TEXT);
        self.view
            .require_control(root::ID_MAPPING_PANEL_PREVIOUS_BUTTON)
            .set_text(symbols::arrow_left_symbol());
        self.view
            .require_control(root::ID_MAPPING_PANEL_NEXT_BUTTON)
            .set_text(symbols::arrow_right_symbol());
        indicator.set_text(symbols::indicator_symbol());
        indicator.disable();
    }

    fn party_is_over(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn when<I: Send + Sync + Clone + 'static>(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = I, Err = ()> + 'static,
        reaction: impl Fn(&ImmutableMappingPanel, I) + 'static + Copy,
    ) {
        when(event.take_until(self.party_is_over()))
            .with(Rc::downgrade(self))
            .do_sync(decorate_reaction(reaction));
    }

    /// Returns self if not handled.
    fn handle_potential_min_max_edit_control_change(
        self: SharedView<Self>,
        resource_id: u32,
    ) -> Option<SharedView<Self>> {
        match resource_id {
            root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_target_value_from_edit_control());
            }
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_target_value_from_edit_control());
            }
            root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_source_value_from_edit_control());
            }
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_source_value_from_edit_control());
            }
            root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_step_from_edit_control());
            }
            root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_step_from_edit_control());
            }
            root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL => {
                self.write(|p| p.handle_mode_fire_line_2_edit_control_change());
            }
            root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL => {
                self.write(|p| p.handle_mode_fire_line_3_edit_control_change());
            }
            _ => return Some(self),
        };
        None
    }
}

impl Drop for MappingPanel {
    fn drop(&mut self) {
        self.close_open_child_windows();
    }
}

fn decorate_reaction<I: Send + Sync + Clone + 'static>(
    reaction: impl Fn(&ImmutableMappingPanel, I) + 'static + Copy,
) -> impl Fn(Rc<MappingPanel>, I) + Copy {
    move |view, item| {
        let view_mirror = view.clone();
        view_mirror.invoke_programmatically(|| {
            // If the reaction can't be displayed anymore because the mapping is not filled anymore,
            // so what.
            let _ = view.read(move |p| reaction(p, item.clone()));
        });
    }
}

impl<'a> MutableMappingPanel<'a> {
    fn resolved_targets(&self) -> Vec<CompoundMappingTarget> {
        self.target_with_context().resolve().unwrap_or_default()
    }

    fn first_resolved_target(&self) -> Option<CompoundMappingTarget> {
        self.resolved_targets().into_iter().next()
    }

    fn set_mapping_target(&mut self, target: &ReaperTarget) {
        self.change_target_with_closure(None, |ctx| {
            ctx.mapping.target_model.apply_from_target(
                target,
                ctx.extended_context,
                ctx.mapping.compartment(),
            )
        });
    }

    fn go_to_target(&self) {
        if let Some(t) = self.first_resolved_target() {
            let session = self.panel.session();
            Global::task_support()
                .do_later_in_main_thread_from_main_thread_asap(move || {
                    let session = session.borrow();
                    t.open(session.control_context())
                })
                .unwrap();
        }
    }

    fn handle_target_line_2_button_press_internal(&mut self) {
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let project = self
                        .session
                        .processor_context()
                        .project_or_current_project();
                    let current_bookmark_data = project.current_bookmark();
                    let (bookmark_type, bookmark_index) = match (
                        current_bookmark_data.marker_index,
                        current_bookmark_data.region_index,
                    ) {
                        (None, None) => return,
                        (Some(i), None) => (BookmarkType::Marker, i),
                        (None, Some(i)) => (BookmarkType::Region, i),
                        (Some(mi), Some(ri)) => match self.mapping.target_model.bookmark_type() {
                            BookmarkType::Marker => (BookmarkType::Marker, mi),
                            BookmarkType::Region => (BookmarkType::Region, ri),
                        },
                    };
                    let bookmark_id = project
                        .find_bookmark_by_index(bookmark_index)
                        .unwrap()
                        .basic_info()
                        .id;
                    self.change_target_with_closure(None, |ctx| {
                        let target = &mut ctx.mapping.target_model;
                        target.change(TargetCommand::SetBookmarkAnchorType(BookmarkAnchorType::Id));
                        target.change(TargetCommand::SetBookmarkType(bookmark_type));
                        target.change(TargetCommand::SetBookmarkRef(bookmark_id.get()));
                        Some(Affected::Multiple)
                    });
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn update_mapping_feedback_send_behavior(&mut self) {
        let behavior = self
            .view
            .require_control(root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid feedback send behavior");
        self.change_mapping(MappingCommand::SetFeedbackSendBehavior(behavior));
    }

    fn change_mapping(&mut self, val: MappingCommand) {
        self.change_mapping_with_initiator(val, None);
    }

    fn change_target_with_closure(
        &mut self,
        initiator: Option<u32>,
        f: impl FnOnce(MappingChangeContext) -> Option<Affected<TargetProp>>,
    ) {
        self.session.change_target_with_closure(
            self.mapping,
            initiator,
            self.panel.session.clone(),
            f,
        );
    }

    fn change_mapping_with_initiator(&mut self, val: MappingCommand, initiator: Option<u32>) {
        self.session.change_mapping_from_ui_expert(
            self.mapping,
            val,
            initiator,
            self.panel.session.clone(),
        );
    }

    fn update_beep_on_success(&mut self) {
        let checked = self
            .view
            .require_control(root::IDC_BEEP_ON_SUCCESS_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::SetBeepOnSuccess(checked));
    }

    fn update_mapping_is_enabled(&mut self) {
        let checked = self
            .view
            .require_control(root::IDC_MAPPING_ENABLED_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::SetIsEnabled(checked));
    }

    fn update_mapping_is_visible_in_projection(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::SetVisibleInProjection(checked));
    }

    fn handle_source_line_4_check_box_change(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeSource(
            SourceCommand::SetIsRegistered(Some(checked)),
        ));
    }

    fn handle_source_check_box_2_change(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SOURCE_14_BIT_CHECK_BOX)
            .is_checked();
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetIs14Bit(
                    Some(checked),
                )));
            }
            Osc => {
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetOscArgIsRelative(checked),
                ));
            }
            Reaper | Virtual | Never | Keyboard | StreamDeck => {}
        };
    }

    fn handle_source_line_3_combo_box_change(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => match self.mapping.source_model.midi_source_type() {
                MidiSourceType::ClockTransport => {
                    let i = b.selected_combo_box_item_index();
                    let msg_type = i.try_into().expect("invalid MTC message type");
                    self.change_mapping(MappingCommand::ChangeSource(
                        SourceCommand::SetMidiClockTransportMessage(msg_type),
                    ));
                }
                MidiSourceType::Display => {
                    let i = b.selected_combo_box_item_index();
                    let display_type = i.try_into().expect("invalid display type");
                    self.change_mapping(MappingCommand::ChangeSource(
                        SourceCommand::SetDisplayType(display_type),
                    ));
                }
                MidiSourceType::Script => {
                    let i = b.selected_combo_box_item_index();
                    let kind = i.try_into().expect("invalid source script kind");
                    self.change_mapping(MappingCommand::ChangeSource(
                        SourceCommand::SetMidiScriptKind(kind),
                    ));
                }
                t if t.supports_channel() => {
                    let value = match b.selected_combo_box_item_data() {
                        -1 => None,
                        id => Some(Channel::new(id as _)),
                    };
                    self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetChannel(
                        value,
                    )));
                }
                _ => {}
            },
            Reaper => match self.mapping.source_model.reaper_source_type() {
                ReaperSourceType::RealearnParameter => {
                    let index = b.selected_combo_box_item_index() as u32;
                    self.change_mapping(MappingCommand::ChangeSource(
                        SourceCommand::SetParameterIndex(
                            index.try_into().expect("invalid param index"),
                        ),
                    ));
                }
                _ => {}
            },
            StreamDeck => {
                let index = b.selected_combo_box_item_index() as u32;
                self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetButtonIndex(
                    index,
                )));
            }
            _ => {}
        };
    }

    fn handle_source_line_4_combo_box_1_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_SOURCE_LINE_4_COMBO_BOX_1);
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Osc => {
                let index = get_osc_arg_index_from_combo(combo);
                self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetOscArgIndex(
                    index,
                )));
            }
            _ => {}
        }
    }

    fn handle_source_line_4_combo_box_2_change(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                use MidiSourceType::*;
                match self.mapping.source_model.midi_source_type() {
                    Display => {
                        use DisplayType::*;
                        let value = match self.mapping.source_model.display_type() {
                            MackieLcd | MackieXtLcd | XTouchMackieLcd | XTouchMackieXtLcd
                            | SiniConE24 | SlKeyboardDisplay => {
                                match b.selected_combo_box_item_data() {
                                    -1 => None,
                                    id => Some(id as u8),
                                }
                            }
                            MackieSevenSegmentDisplay => {
                                Some(b.selected_combo_box_item_index() as u8)
                            }
                            LaunchpadProScrollingText => return,
                        };
                        self.change_mapping(MappingCommand::ChangeSource(
                            SourceCommand::SetDisplayId(value),
                        ));
                    }
                    t if t.supports_midi_message_number() => {
                        let value = match b.selected_combo_box_item_data() {
                            -1 => None,
                            id => Some(U7::new(id as _)),
                        };
                        self.change_mapping(MappingCommand::ChangeSource(
                            SourceCommand::SetMidiMessageNumber(value),
                        ));
                    }
                    _ => {}
                }
            }
            Osc => {
                let tag = get_osc_type_tag_from_combo(b);
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetOscArgTypeTag(tag),
                ));
            }
            _ => {}
        }
    }

    fn handle_source_line_5_combo_box_change(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                use MidiSourceType::*;
                match self.mapping.source_model.midi_source_type() {
                    Display => {
                        let value = match b.selected_combo_box_item_data() {
                            -1 => None,
                            id => Some(id as u8),
                        };
                        self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetLine(
                            value,
                        )));
                    }
                    t if t.supports_custom_character() => {
                        let i = b.selected_combo_box_item_index();
                        let character = i.try_into().expect("invalid source character");
                        self.change_mapping(MappingCommand::ChangeSource(
                            SourceCommand::SetCustomCharacter(character),
                        ));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn update_source_category(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX);
        let category = b
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid source category");
        self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetCategory(
            category,
        )));
    }

    fn handle_source_line_2_combo_box_change(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        let i = b.selected_combo_box_item_index();
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                let source_type = i.try_into().expect("invalid MIDI source type");
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetMidiSourceType(source_type),
                ));
            }
            Reaper => {
                let source_type = i.try_into().expect("invalid REAPER source type");
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetReaperSourceType(source_type),
                ));
            }
            Virtual => {
                let element_type = i.try_into().expect("invalid virtual source type");
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetControlElementCharacter(element_type),
                ));
            }
            _ => {}
        };
    }

    fn handle_source_line_4_edit_control_change(&mut self) {
        let edit_control_id = root::ID_SOURCE_NUMBER_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        let text = c.text().unwrap_or_default();
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                let value = text.parse().ok();
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetParameterNumberMessageNumber(
                        value,
                    )),
                    Some(edit_control_id),
                );
            }
            StreamDeck => {
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetButtonBackgroundImagePath(
                        text.into(),
                    )),
                    Some(edit_control_id),
                );
            }
            Virtual => {
                let value = text.parse().unwrap_or_default();
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetControlElementId(value)),
                    Some(edit_control_id),
                );
            }
            Reaper | Never | Keyboard | Osc => {}
        };
    }

    fn handle_source_line_5_edit_control_change(&mut self) {
        let edit_control_id = root::ID_SOURCE_LINE_5_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        let text = c.text().unwrap_or_default();
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            StreamDeck => {
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetButtonForegroundImagePath(
                        text.into(),
                    )),
                    Some(edit_control_id),
                );
            }
            Osc => {
                let v = parse_osc_arg_value_range(&text);
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetOscArgValueRange(v)),
                    Some(edit_control_id),
                );
            }
            _ => {}
        };
    }

    fn handle_source_line_3_edit_control_change(&mut self) {
        let edit_control_id = root::ID_SOURCE_LINE_3_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        if let Ok(value) = c.text() {
            use SourceCategory::*;
            match self.mapping.source_model.category() {
                Osc => {
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeSource(SourceCommand::SetOscAddressPattern(value)),
                        Some(edit_control_id),
                    );
                }
                Reaper => match self.mapping.source_model.reaper_source_type() {
                    ReaperSourceType::Timer => {
                        let value = value.parse().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeSource(SourceCommand::SetTimerMillis(value)),
                            Some(edit_control_id),
                        )
                    }
                    _ => {}
                },
                Midi | Virtual | Never | Keyboard | StreamDeck => {}
            }
        }
    }

    fn handle_source_line_7_edit_control_change(&mut self) {
        let edit_control_id = root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        let value = c.text().unwrap_or_default();
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => match self.mapping.source_model.midi_source_type() {
                MidiSourceType::Raw => {
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeSource(SourceCommand::SetRawMidiPattern(value)),
                        Some(edit_control_id),
                    );
                }
                MidiSourceType::Script => {
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeSource(SourceCommand::SetMidiScript(value)),
                        Some(edit_control_id),
                    );
                }
                _ => {}
            },
            Osc => {
                let args = parse_osc_feedback_args(&value);
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetOscFeedbackArgs(args)),
                    Some(edit_control_id),
                )
            }
            StreamDeck => self.change_mapping_with_initiator(
                MappingCommand::ChangeSource(SourceCommand::SetButtonStaticText(value)),
                Some(edit_control_id),
            ),
            _ => {}
        }
    }

    fn update_mode_rotate(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetRotate(checked)));
    }

    fn update_mode_feedback_type(&mut self) {
        let index = self
            .view
            .require_control(root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX)
            .selected_combo_box_item_index();
        let feedback_type = index.try_into().expect("unknown feedback type");
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetFeedbackType(
            feedback_type,
        )));
    }

    fn update_mode_make_absolute(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMakeAbsolute(
            checked,
        )));
    }

    fn update_mode_out_of_range_behavior(&mut self) {
        let behavior = self
            .view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid out-of-range behavior");
        self.change_mapping(MappingCommand::ChangeMode(
            ModeCommand::SetOutOfRangeBehavior(behavior),
        ));
    }

    fn update_mode_group_interaction(&mut self) {
        let interaction = self
            .view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid group interaction");
        self.change_mapping(MappingCommand::ChangeMode(
            ModeCommand::SetGroupInteraction(interaction),
        ));
    }

    fn update_mode_fire_mode(&mut self) {
        let mode = self
            .view
            .require_control(root::ID_MODE_FIRE_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid fire mode");
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetFireMode(mode)));
    }

    fn update_mode_round_target_value(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeMode(
            ModeCommand::SetRoundTargetValue(checked),
        ));
    }

    fn update_takeover_mode(&mut self) {
        let mode = self
            .view
            .require_control(root::ID_MODE_TAKEOVER_MODE)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid takeover mode");
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetTakeoverMode(
            mode,
        )));
    }

    fn update_button_usage(&mut self) {
        let mode = self
            .view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid button usage");
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetButtonUsage(
            mode,
        )));
    }

    fn update_encoder_usage(&mut self) {
        let mode = self
            .view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid encoder usage");
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetEncoderUsage(
            mode,
        )));
    }

    fn update_mode_reverse(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetReverse(checked)));
    }

    fn reset_mode(&mut self) {
        let _ = self.session.change_mapping_with_closure(
            self.mapping,
            None,
            self.panel.session.clone(),
            |ctx| Ok(ctx.mapping.reset_mode(ctx.extended_context)),
        );
    }

    fn update_mode_type(&mut self) {
        let b = self.view.require_control(root::ID_SETTINGS_MODE_COMBO_BOX);
        let mode = b
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid mode type");
        let _ = self.session.change_mapping_with_closure(
            self.mapping,
            None,
            self.panel.session.clone(),
            |ctx| {
                Ok(ctx
                    .mapping
                    .set_absolute_mode_and_preferred_values(ctx.extended_context, mode))
            },
        );
    }

    fn update_mode_min_target_value_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL;
        let value = self
            .get_value_from_target_edit_control(control_id)
            .unwrap_or(UnitValue::MIN);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMinTargetValue(value)),
            Some(control_id),
        );
    }

    fn get_value_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.first_resolved_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        let control_context = self.session.control_context();
        match self.mapping.target_model.unit() {
            TargetUnit::Native => target.parse_as_value(text.as_str(), control_context).ok(),
            TargetUnit::Percent => parse_unit_value_from_percentage(&text).ok(),
        }
    }

    fn get_step_size_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.first_resolved_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        let control_context = self.session.control_context();
        match self.mapping.target_model.unit() {
            TargetUnit::Native => target
                .parse_as_step_size(text.as_str(), control_context)
                .ok(),
            TargetUnit::Percent => parse_unit_value_from_percentage(&text).ok(),
        }
    }

    fn update_mode_max_target_value_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL;
        let value = self
            .get_value_from_target_edit_control(control_id)
            .unwrap_or(UnitValue::MAX);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMaxTargetValue(value)),
            Some(control_id),
        );
    }

    fn update_mode_min_source_value_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL;
        let value = self
            .get_value_from_source_edit_control(control_id)
            .unwrap_or(UnitValue::MIN);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMinSourceValue(value)),
            Some(control_id),
        );
    }

    fn get_value_from_source_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let text = self.view.require_control(edit_control_id).text().ok()?;
        self.mapping
            .source_model
            .parse_control_value(text.as_str())
            .ok()
    }

    fn update_mode_max_source_value_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL;
        let value = self
            .get_value_from_source_edit_control(control_id)
            .unwrap_or(UnitValue::MAX);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMaxSourceValue(value)),
            Some(control_id),
        );
    }

    fn update_mode_min_step_from_edit_control(&mut self) {
        self.update_mode_step_from_edit_control(
            root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
            ModeCommand::SetMinStepFactor,
            DiscreteIncrement::new(1),
            ModeCommand::SetMinStepSize,
            UnitValue::MIN,
        );
    }

    fn update_mode_step_from_edit_control(
        &mut self,
        control_id: u32,
        factor_command: impl FnOnce(DiscreteIncrement) -> ModeCommand,
        default_factor: DiscreteIncrement,
        size_command: impl FnOnce(UnitValue) -> ModeCommand,
        default_size: UnitValue,
    ) {
        if self.mapping_uses_step_factors() {
            let value = self
                .get_value_from_step_factor_edit_control(control_id)
                .unwrap_or(default_factor);
            self.change_mapping_with_initiator(
                MappingCommand::ChangeMode(factor_command(value)),
                Some(control_id),
            );
        } else {
            let value = self
                .get_value_from_step_size_edit_control(control_id)
                .unwrap_or(default_size);
            self.change_mapping_with_initiator(
                MappingCommand::ChangeMode(size_command(value)),
                Some(control_id),
            );
        }
    }

    fn handle_mode_fire_line_2_edit_control_change(&mut self) {
        let control_id = root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL;
        let value = self
            .get_value_from_duration_edit_control(control_id)
            .unwrap_or_else(|| Duration::from_millis(0));
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMinPressDuration(value)),
            Some(control_id),
        );
    }

    fn get_value_from_duration_edit_control(&self, edit_control_id: u32) -> Option<Duration> {
        let text = self.view.require_control(edit_control_id).text().ok()?;
        text.parse::<u64>().ok().map(Duration::from_millis)
    }

    fn get_value_from_step_factor_edit_control(
        &self,
        edit_control_id: u32,
    ) -> Option<DiscreteIncrement> {
        let text = self.view.require_control(edit_control_id).text().ok()?;
        let number: i32 = text.parse().ok()?;
        number.try_into().ok()
    }

    fn get_value_from_step_size_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        self.get_step_size_from_target_edit_control(edit_control_id)
    }

    fn update_mode_max_step_from_edit_control(&mut self) {
        self.update_mode_step_from_edit_control(
            root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
            ModeCommand::SetMaxStepFactor,
            DiscreteIncrement::new(100),
            ModeCommand::SetMaxStepSize,
            UnitValue::MAX,
        );
    }

    fn handle_mode_fire_line_3_edit_control_change(&mut self) {
        let control_id = root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL;
        let value = self
            .get_value_from_duration_edit_control(control_id)
            .unwrap_or_else(|| Duration::from_millis(0));
        self.handle_mode_fire_line_3_duration_change(value, Some(control_id));
    }

    fn update_mode_target_value_sequence(&mut self) {
        let control_id = root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL;
        let text = self
            .view
            .require_control(control_id)
            .text()
            .unwrap_or_else(|_| "".to_string());
        let sequence = match self.mapping.target_model.unit() {
            TargetUnit::Native => {
                if let Some(t) = self.first_resolved_target() {
                    let t = WithControlContext::new(self.session.control_context(), &t);
                    ValueSequence::parse(&t, &text)
                } else {
                    ValueSequence::parse(&PercentIo, &text)
                }
            }
            TargetUnit::Percent => ValueSequence::parse(&PercentIo, &text),
        };
        let sequence = sequence.unwrap_or_default();
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetTargetValueSequence(sequence)),
            Some(control_id),
        );
    }

    fn update_mode_eel_control_transformation(&mut self) {
        let control_id = root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL;
        let value = self
            .view
            .require_control(control_id)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetEelControlTransformation(value)),
            Some(control_id),
        );
    }

    fn update_mode_feedback_transformation(&mut self) {
        let control_id = root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL;
        let value = self
            .view
            .require_control(control_id)
            .text()
            .unwrap_or_else(|_| "".to_string());
        let cmd = if self.mapping.mode_model.feedback_type().is_textual() {
            ModeCommand::SetTextualFeedbackExpression(value)
        } else {
            ModeCommand::SetEelFeedbackTransformation(value)
        };
        self.change_mapping_with_initiator(MappingCommand::ChangeMode(cmd), Some(control_id));
    }

    fn update_mode_min_target_value_from_slider(&mut self, slider: Window) {
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMinTargetValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_max_target_value_from_slider(&mut self, slider: Window) {
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMaxTargetValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_min_source_value_from_slider(&mut self, slider: Window) {
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMinSourceValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_max_source_value_from_slider(&mut self, slider: Window) {
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMaxSourceValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_min_step_from_slider(&mut self, slider: Window) {
        self.update_mode_step_from_slider(
            slider,
            ModeCommand::SetMinStepFactor,
            ModeCommand::SetMinStepSize,
        )
    }

    fn update_mode_max_step_from_slider(&mut self, slider: Window) {
        self.update_mode_step_from_slider(
            slider,
            ModeCommand::SetMaxStepFactor,
            ModeCommand::SetMaxStepSize,
        )
    }

    fn update_mode_step_from_slider(
        &mut self,
        slider: Window,
        factor_command: impl FnOnce(DiscreteIncrement) -> ModeCommand,
        size_command: impl FnOnce(UnitValue) -> ModeCommand,
    ) {
        if self.mapping_uses_step_factors() {
            let value = slider.slider_discrete_increment();
            self.change_mapping(MappingCommand::ChangeMode(factor_command(value)));
        } else {
            let value = slider.slider_unit_value();
            self.change_mapping(MappingCommand::ChangeMode(size_command(value)));
        };
    }

    fn handle_mode_fire_line_2_slider_change(&mut self, slider: Window) {
        self.change_mapping(MappingCommand::ChangeMode(
            ModeCommand::SetMinPressDuration(slider.slider_duration()),
        ));
    }

    fn handle_mode_fire_line_3_slider_change(&mut self, slider: Window) {
        let value = slider.slider_duration();
        self.handle_mode_fire_line_3_duration_change(value, None);
    }

    fn handle_mode_fire_line_3_duration_change(&mut self, value: Duration, initiator: Option<u32>) {
        match self.mapping.mode_model.fire_mode() {
            FireMode::Normal | FireMode::OnSinglePress | FireMode::OnDoublePress => {
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeMode(ModeCommand::SetMaxPressDuration(value)),
                    initiator,
                );
            }
            FireMode::AfterTimeout => {}
            FireMode::AfterTimeoutKeepFiring => {
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeMode(ModeCommand::SetTurboRate(value)),
                    initiator,
                );
            }
        }
    }

    fn mapping_uses_step_factors(&self) -> bool {
        self.mapping
            .with_context(self.session.extended_context())
            .uses_step_factors()
    }

    fn handle_target_check_box_1_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_1)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => {
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetWithTrack(
                        is_checked,
                    )));
                }
                ReaperTargetType::GoToBookmark => {
                    let bookmark_type = if is_checked {
                        BookmarkType::Region
                    } else {
                        BookmarkType::Marker
                    };
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetBookmarkType(bookmark_type),
                    ));
                }
                ReaperTargetType::PlaytimeSlotTransportAction => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetStopColumnIfSlotEmpty(is_checked),
                    ));
                }
                _ if self.mapping.target_model.supports_fx_chain() => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetFxIsInputFx(is_checked),
                    ));
                }
                t if t.supports_track_scrolling() => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetScrollArrangeView(is_checked),
                    ));
                }
                ReaperTargetType::Seek => {
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetSeekPlay(
                        is_checked,
                    )));
                }
                _ => {}
            },
            TargetCategory::Virtual => {
                self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetLearnable(
                    is_checked,
                )));
            }
        }
    }

    fn handle_target_check_box_2_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_2)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                _ if self.mapping.target_model.supports_track_must_be_selected() => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetEnableOnlyIfTrackSelected(is_checked),
                    ));
                }
                t if t.supports_track_scrolling() => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetScrollMixer(is_checked),
                    ));
                }
                ReaperTargetType::Seek => {
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetMoveView(
                        is_checked,
                    )));
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetActiveMappingsOnly(is_checked),
                    ));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_check_box_3_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_3)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetEnableOnlyIfFxHasFocus(is_checked),
                    ));
                }
                ReaperTargetType::Seek => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetUseProject(is_checked),
                    ));
                }
                t if self.mapping.target_model.supports_gang_selected() => {
                    let gang_behavior = TrackGangBehavior::from_bools(
                        t.definition(),
                        is_checked,
                        self.mapping
                            .target_model
                            .fixed_gang_behavior()
                            .use_track_grouping(),
                    );
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetGangBehavior(gang_behavior),
                    ));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_check_box_4_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_4)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Seek => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetUseRegions(is_checked),
                    ));
                }
                t if t.supports_poll_for_feedback() => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPollForFeedback(is_checked),
                    ));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_check_box_5_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_5)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::FxParameterValue => self.change_mapping(
                    MappingCommand::ChangeTarget(TargetCommand::SetRetrigger(is_checked)),
                ),
                ReaperTargetType::Seek | ReaperTargetType::GoToBookmark => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetUseLoopPoints(is_checked),
                    ));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_check_box_6_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_6)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::FxParameterValue => self.change_mapping(
                    MappingCommand::ChangeTarget(TargetCommand::SetRealTime(is_checked)),
                ),
                ReaperTargetType::Seek | ReaperTargetType::GoToBookmark => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetUseTimeSelection(is_checked),
                    ));
                }
                t if self.mapping.target_model.supports_gang_grouping() => {
                    let gang_behavior = TrackGangBehavior::from_bools(
                        t.definition(),
                        self.mapping
                            .target_model
                            .fixed_gang_behavior()
                            .use_selection_ganging(),
                        is_checked,
                    );
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetGangBehavior(gang_behavior),
                    ));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_unit_button_press(&mut self) {
        use TargetUnit::*;
        let next_unit = match self.mapping.target_model.unit() {
            Native => Percent,
            Percent => Native,
        };
        self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetUnit(
            next_unit,
        )));
    }

    fn update_target_category(&mut self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX);
        let category = b
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid target category");
        self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetCategory(
            category,
        )));
    }

    fn handle_target_line_2_combo_box_1_change(&mut self) {
        let combo_id = root::ID_TARGET_LINE_2_COMBO_BOX_1;
        let combo = self.view.require_control(combo_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let bookmark_anchor_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetBookmarkAnchorType(bookmark_anchor_type),
                    ));
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    let snapshot_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMappingSnapshotTypeForLoad(snapshot_type),
                    ));
                }
                ReaperTargetType::TakeMappingSnapshot => {
                    let snapshot_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMappingSnapshotTypeForTake(snapshot_type),
                    ));
                }
                t if t.supports_feedback_resolution() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid feedback resolution");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetFeedbackResolution(v),
                    ));
                }
                _ if self.mapping.target_model.supports_track() => {
                    let track_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_target_with_closure(Some(combo_id), |ctx| {
                        ctx.mapping
                            .target_model
                            .set_track_type_from_ui(track_type, ctx.extended_context.context)
                    });
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_3_combo_box_1_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_3_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => {
                    let scope: ActionScope = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetActionScope(scope),
                    ));
                }
                ReaperTargetType::PlaytimeColumnAction => {
                    let kind = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    let desc = PlaytimeColumnDescriptor::from_kind(kind);
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeColumn(desc),
                    ));
                }
                ReaperTargetType::PlaytimeRowAction => {
                    let kind = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    let desc = PlaytimeRowDescriptor::from_kind(kind);
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeRow(desc),
                    ));
                }
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotSeek
                | ReaperTargetType::PlaytimeSlotVolume => {
                    let kind = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    let desc = PlaytimeSlotDescriptor::from_kind(kind);
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeSlot(desc),
                    ));
                }
                t if t.supports_fx() => {
                    let fx_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_target_with_closure(None, |ctx| {
                        ctx.mapping.target_model.set_fx_type_from_ui(
                            fx_type,
                            ctx.extended_context,
                            ctx.mapping.compartment(),
                        )
                    });
                }
                t if t.supports_send() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid route type");
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetRouteType(
                        v,
                    )));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_4_combo_box_1_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_4_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc => {
                    let index = get_osc_arg_index_from_combo(combo);
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetOscArgIndex(index),
                    ));
                }
                ReaperTargetType::BrowseGroup => {
                    let exclusivity: SimpleExclusivity = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetExclusivity(exclusivity.into()),
                    ));
                }
                ReaperTargetType::TrackTool => {
                    let action: TrackToolAction = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTrackToolAction(action),
                    ));
                }
                ReaperTargetType::FxTool => {
                    let action: FxToolAction = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetFxToolAction(action),
                    ));
                }
                t if t.supports_fx_parameter() => {
                    let param_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetParamType(
                        param_type,
                    )));
                }
                t if t.supports_exclusivity() => {
                    let exclusivity = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetExclusivity(exclusivity),
                    ));
                }
                t if t.supports_send() => {
                    let selector_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetRouteSelectorType(selector_type),
                    ));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_2_combo_box_2_change(&mut self) {
        let combo_id = root::ID_TARGET_LINE_2_COMBO_BOX_2;
        let combo = self.view.require_control(combo_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let value: u32 = combo.selected_combo_box_item_data() as _;
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetBookmarkRef(value),
                    ));
                }
                ReaperTargetType::AutomationModeOverride => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid automation mode override type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetAutomationModeOverrideType(v),
                    ));
                }
                ReaperTargetType::Mouse => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid mouse action type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMouseActionType(v),
                    ));
                }
                ReaperTargetType::Transport => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid transport action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTransportAction(v),
                    ));
                }
                ReaperTargetType::AnyOn => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid any-on parameter");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetAnyOnParameter(v),
                    ));
                }
                ReaperTargetType::BrowseGroup => {
                    let i = combo.selected_combo_box_item_index();
                    let group_id = self
                        .session
                        .find_group_by_index_sorted(self.mapping.compartment(), i)
                        .expect("group not existing")
                        .borrow()
                        .id();
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetGroupId(
                        group_id,
                    )));
                }
                ReaperTargetType::SendMidi => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid send MIDI destination");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetSendMidiDestinationType(v),
                    ));
                }
                ReaperTargetType::SendOsc => {
                    let dev_id = match combo.selected_combo_box_item_data() {
                        -1 => None,
                        i if i >= 0 => BackboneShell::get()
                            .osc_device_manager()
                            .borrow()
                            .find_device_by_index(i as usize)
                            .map(|dev| *dev.id()),
                        _ => None,
                    };
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetOscDevId(
                        dev_id,
                    )));
                }
                ReaperTargetType::BrowseTracks => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid track indexing policy");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetBrowseTracksMode(v),
                    ));
                }
                ReaperTargetType::BrowsePotFilterItems => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid pot filter item kind");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPotFilterItemKind(v),
                    ));
                }
                ReaperTargetType::ModifyMapping => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid mapping feature");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMappingModificationKind(v),
                    ));
                }
                ReaperTargetType::PlaytimeMatrixAction => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid matrix action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeMatrixAction(v),
                    ));
                }
                ReaperTargetType::PlaytimeColumnAction => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid column action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeColumnAction(v),
                    ));
                }
                ReaperTargetType::PlaytimeRowAction => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid row action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeRowAction(v),
                    ));
                }
                ReaperTargetType::PlaytimeSlotManagementAction => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid slot management action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeSlotManagementAction(v),
                    ));
                }
                ReaperTargetType::PlaytimeSlotTransportAction => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid slot transport action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetPlaytimeSlotTransportAction(v),
                    ));
                }
                _ if self.mapping.target_model.supports_track() => {
                    let project = self
                        .session
                        .processor_context()
                        .project_or_current_project();
                    let i = combo.selected_combo_box_item_index();
                    if let Some(track) = project.track_by_index(i as _) {
                        self.change_target_with_closure(Some(combo_id), |ctx| {
                            ctx.mapping.target_model.set_concrete_track(
                                ConcreteTrackInstruction::ByIdWithTrack(track),
                                false,
                                true,
                            )
                        });
                    }
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_3_combo_box_2_change(&mut self) {
        let combo_id = root::ID_TARGET_LINE_3_COMBO_BOX_2;
        let combo = self.view.require_control(combo_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => {
                    if let Ok(track) = self.target_with_context().first_effective_track() {
                        let chain = if self.mapping.target_model.fx_is_input_fx() {
                            track.input_fx_chain()
                        } else {
                            track.normal_fx_chain()
                        };
                        let i = combo.selected_combo_box_item_index();
                        if let Some(fx) = chain.fx_by_index(i as _) {
                            self.change_target_with_closure(None, |ctx| {
                                ctx.mapping.target_model.set_concrete_fx(
                                    ConcreteFxInstruction::ByIdWithFx(fx),
                                    false,
                                    true,
                                )
                            });
                        }
                    }
                }
                t if t.supports_seek_behavior() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid seek behavior");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetSeekBehavior(v),
                    ));
                }
                ReaperTargetType::Action => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid action invocation type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetActionInvocationType(v),
                    ));
                }
                ReaperTargetType::TrackSolo => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid solo behavior");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetSoloBehavior(v),
                    ));
                }
                ReaperTargetType::TrackShow => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid track area");
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetTrackArea(
                        v,
                    )));
                }
                ReaperTargetType::TrackAutomationMode
                | ReaperTargetType::AutomationModeOverride
                | ReaperTargetType::RouteAutomationMode => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid automation mode");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetAutomationMode(v),
                    ));
                }
                ReaperTargetType::TrackMonitoringMode => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid monitoring mode");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMonitoringMode(v),
                    ));
                }
                ReaperTargetType::TrackTouchState => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid touched track parameter type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTouchedTrackParameterType(v),
                    ));
                }
                ReaperTargetType::RouteTouchState => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid touched route parameter type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTouchedRouteParameterType(v),
                    ));
                }
                _ if self.mapping.target_model.supports_axis() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid axis type");
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetAxis(v)));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_4_combo_box_2_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_4_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc => {
                    let v = get_osc_type_tag_from_combo(combo);
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetOscArgTypeTag(v),
                    ));
                }
                ReaperTargetType::Mouse if self.mapping.target_model.supports_mouse_button() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid mouse button type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetMouseButton(v),
                    ));
                }
                t if t.supports_fx_parameter() => {
                    let fx = get_relevant_target_fx(self.mapping, self.session);
                    if let Some(fx) = fx {
                        let i = combo.selected_combo_box_item_index();
                        let param = fx.parameter_by_index(i as _);
                        self.change_mapping(MappingCommand::ChangeTarget(
                            TargetCommand::SetParamIndex(i as _),
                        ));
                        // We also set name so that we can easily switch between types.
                        // Parameter names are not reliably UTF-8-encoded (e.g. "JS: Stereo Width")
                        let param_name = param
                            .name()
                            .unwrap()
                            .into_inner()
                            .to_string_lossy()
                            .to_string();
                        self.change_mapping(MappingCommand::ChangeTarget(
                            TargetCommand::SetParamName(param_name),
                        ));
                        if self.mapping.target_model.fx_type() == VirtualFxType::Focused {
                            self.panel.set_simple_help_text(
                                Side::Right,
                                "Target warning",
                                r#"ATTENTION: You just picked a parameter for the last focused FX. This is okay but you should know that as soon as you focus another type of FX, the parameter list will change and your mapping will control a completely different parameter! You probably want to use this in combination with the "Auto-load" feature, which lets you link FX types to mapping presets. See https://docs.helgoboss.org/realearn/goto#auto-load"#,
                            );
                        }
                    }
                }
                t if t.supports_track_exclusivity() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid track exclusivity");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTrackExclusivity(v),
                    ));
                }
                t if t.supports_fx_display_type() => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid FX display type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetFxDisplayType(v),
                    ));
                }
                t if t.supports_send() => {
                    if let Ok(track) = self.target_with_context().first_effective_track() {
                        let i = combo.selected_combo_box_item_index();
                        let route_type = self.mapping.target_model.route_type();
                        if let Ok(route) = resolve_track_route_by_index(&track, route_type, i as _)
                        {
                            if let Some(TrackRoutePartner::Track(t)) = route.partner() {
                                // Track send/receive. We use the partner track ID as stable ID!
                                self.change_mapping(MappingCommand::ChangeTarget(
                                    TargetCommand::SetRouteId(Some(*t.guid())),
                                ));
                            }
                            // We also set index and name. First because hardware output relies on
                            // the index as "ID", but also so we can easily switch between
                            // selector types.
                            self.change_mapping(MappingCommand::ChangeTarget(
                                TargetCommand::SetRouteIndex(i as _),
                            ));
                            let route_name = route.name().into_string();
                            self.change_mapping(MappingCommand::ChangeTarget(
                                TargetCommand::SetRouteName(route_name),
                            ));
                        }
                    }
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_2_edit_control_change(&mut self) {
        let edit_control_id = root::ID_TARGET_LINE_2_EDIT_CONTROL;
        let control = self.view.require_control(edit_control_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let human_value: u32 = control
                        .text()
                        .unwrap_or_default()
                        .parse()
                        .unwrap_or_default();
                    let internal_value = human_value.saturating_sub(1);
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetBookmarkRef(internal_value)),
                        Some(edit_control_id),
                    );
                }
                _ if self.mapping.target_model.supports_mapping_snapshot_id() => {
                    let id = control.text().unwrap_or_default().parse().ok();
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetMappingSnapshotId(id)),
                        Some(edit_control_id),
                    );
                }
                _ if self.mapping.target_model.supports_track() => {
                    match self.mapping.target_model.track_type() {
                        t if t.is_dynamic() => {
                            let expression = control.text().unwrap_or_default();
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetTrackExpression(
                                    expression,
                                )),
                                Some(edit_control_id),
                            );
                        }
                        VirtualTrackType::ByName | VirtualTrackType::AllByName => {
                            let name = control.text().unwrap_or_default();
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetTrackName(name)),
                                Some(edit_control_id),
                            );
                        }
                        t if t.is_by_index() => {
                            let index = parse_position_as_index(control);
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetTrackIndex(index)),
                                Some(edit_control_id),
                            );
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            TargetCategory::Virtual => {
                let text = control.text().unwrap_or_default();
                let v = text.parse().unwrap_or_default();
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeTarget(TargetCommand::SetControlElementId(v)),
                    Some(edit_control_id),
                );
            }
        }
    }

    fn handle_target_line_3_edit_control_change(&mut self) {
        let edit_control_id = root::ID_TARGET_LINE_3_EDIT_CONTROL;
        let control = self.view.require_control(edit_control_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc => {
                    let pattern = control.text().unwrap_or_default();
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetOscAddressPattern(pattern)),
                        Some(edit_control_id),
                    );
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    let text = control.text().unwrap_or_default();
                    let value = parse_unit_value_from_percentage(&text)
                        .map(AbsoluteValue::Continuous)
                        .ok();
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(
                            TargetCommand::SetMappingSnapshotDefaultValue(value),
                        ),
                        Some(edit_control_id),
                    );
                }
                ReaperTargetType::PlaytimeColumnAction => {
                    let text = control.text().unwrap_or_default();
                    match self.mapping.target_model.playtime_column() {
                        PlaytimeColumnDescriptor::Active => {}
                        PlaytimeColumnDescriptor::ByIndex(_) => {
                            let position: usize = text.parse().unwrap_or_default();
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeColumn(
                                    PlaytimeColumnDescriptor::ByIndex(ColumnAddress {
                                        index: position.saturating_sub(1),
                                    }),
                                )),
                                Some(edit_control_id),
                            );
                        }
                        PlaytimeColumnDescriptor::Dynamic { .. } => {
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeColumn(
                                    PlaytimeColumnDescriptor::Dynamic { expression: text },
                                )),
                                Some(edit_control_id),
                            );
                        }
                    }
                }
                ReaperTargetType::PlaytimeRowAction => {
                    let text = control.text().unwrap_or_default();
                    match self.mapping.target_model.playtime_row() {
                        PlaytimeRowDescriptor::Active => {}
                        PlaytimeRowDescriptor::ByIndex(_) => {
                            let position: usize = text.parse().unwrap_or_default();
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeRow(
                                    PlaytimeRowDescriptor::ByIndex(RowAddress {
                                        index: position.saturating_sub(1),
                                    }),
                                )),
                                Some(edit_control_id),
                            );
                        }
                        PlaytimeRowDescriptor::Dynamic { .. } => {
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeRow(
                                    PlaytimeRowDescriptor::Dynamic { expression: text },
                                )),
                                Some(edit_control_id),
                            );
                        }
                    }
                }
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotVolume
                | ReaperTargetType::PlaytimeSlotSeek => {
                    let text = control.text().unwrap_or_default();
                    match self.mapping.target_model.playtime_slot() {
                        PlaytimeSlotDescriptor::Active => {}
                        PlaytimeSlotDescriptor::ByIndex(address) => {
                            let position: usize = text.parse().unwrap_or_default();
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeSlot(
                                    PlaytimeSlotDescriptor::ByIndex(SlotAddress::new(
                                        position.saturating_sub(1),
                                        address.row_index,
                                    )),
                                )),
                                Some(edit_control_id),
                            );
                        }
                        PlaytimeSlotDescriptor::Dynamic { row_expression, .. } => {
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeSlot(
                                    PlaytimeSlotDescriptor::Dynamic {
                                        column_expression: text,
                                        row_expression: row_expression.clone(),
                                    },
                                )),
                                Some(edit_control_id),
                            );
                        }
                    }
                }
                t if t.supports_fx() => match self.mapping.target_model.fx_type() {
                    VirtualFxType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetFxExpression(
                                expression,
                            )),
                            Some(edit_control_id),
                        );
                    }
                    VirtualFxType::ByName | VirtualFxType::AllByName => {
                        let name = control.text().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetFxName(name)),
                            Some(edit_control_id),
                        );
                    }
                    VirtualFxType::ByIndex => {
                        let index = parse_position_as_index(control);
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetFxIndex(index)),
                            Some(edit_control_id),
                        );
                    }
                    _ => {}
                },
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_4_edit_control_change(&mut self) {
        let edit_control_id = root::ID_TARGET_LINE_4_EDIT_CONTROL;
        let control = self.view.require_control(edit_control_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx_parameter() => match self.mapping.target_model.param_type() {
                    VirtualFxParameterType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetParamExpression(
                                expression,
                            )),
                            Some(edit_control_id),
                        );
                    }
                    VirtualFxParameterType::ByName => {
                        let name = control.text().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetParamName(name)),
                            Some(edit_control_id),
                        );
                    }
                    VirtualFxParameterType::ByIndex => {
                        let index = parse_position_as_index(control);
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetParamIndex(index)),
                            Some(edit_control_id),
                        );
                    }
                    VirtualFxParameterType::ById => {}
                },
                ReaperTargetType::SendMidi => {
                    let text = control.text().unwrap_or_default();
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetRawMidiPattern(text)),
                        Some(edit_control_id),
                    );
                }
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotVolume
                | ReaperTargetType::PlaytimeSlotSeek => {
                    let text = control.text().unwrap_or_default();
                    match self.mapping.target_model.playtime_slot() {
                        PlaytimeSlotDescriptor::Active => {}
                        PlaytimeSlotDescriptor::ByIndex(address) => {
                            let position: usize = text.parse().unwrap_or_default();
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeSlot(
                                    PlaytimeSlotDescriptor::ByIndex(SlotAddress::new(
                                        address.column_index,
                                        position.saturating_sub(1),
                                    )),
                                )),
                                Some(edit_control_id),
                            );
                        }
                        PlaytimeSlotDescriptor::Dynamic {
                            column_expression, ..
                        } => {
                            self.change_mapping_with_initiator(
                                MappingCommand::ChangeTarget(TargetCommand::SetPlaytimeSlot(
                                    PlaytimeSlotDescriptor::Dynamic {
                                        column_expression: column_expression.clone(),
                                        row_expression: text,
                                    },
                                )),
                                Some(edit_control_id),
                            );
                        }
                    }
                }
                t if t.supports_send() => match self.mapping.target_model.route_selector_type() {
                    TrackRouteSelectorType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetRouteExpression(
                                expression,
                            )),
                            Some(edit_control_id),
                        );
                    }
                    TrackRouteSelectorType::ByName => {
                        let name = control.text().unwrap_or_default();
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetRouteName(name)),
                            Some(edit_control_id),
                        );
                    }
                    TrackRouteSelectorType::ByIndex => {
                        let index = parse_position_as_index(control);
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeTarget(TargetCommand::SetRouteIndex(index)),
                            Some(edit_control_id),
                        );
                    }
                    _ => {}
                },
                t if t.supports_tags() => {
                    let text = control.text().unwrap_or_default();
                    let v = parse_tags_from_csv(&text);
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetTags(v)),
                        Some(edit_control_id),
                    );
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_5_edit_control_change(&mut self) {
        let edit_control_id = root::ID_TARGET_LINE_5_EDIT_CONTROL;
        let control = self.view.require_control(edit_control_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc => {
                    let text = control.text().unwrap_or_default();
                    let v = parse_osc_arg_value_range(&text);
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetOscArgValueRange(v)),
                        Some(edit_control_id),
                    );
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn target_category(&self) -> TargetCategory {
        self.mapping.target_model.category()
    }

    fn reaper_target_type(&self) -> ReaperTargetType {
        self.mapping.target_model.target_type()
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.extended_context(), self.mapping.compartment())
    }
}

const NO_HELP_ELEMENTS: &[u32] = &[
    0,
    root::ID_MODE_BUTTON_GROUP_BOX,
    root::ID_MODE_RELATIVE_GROUP_BOX,
    root::ID_MODE_KNOB_FADER_GROUP_BOX,
];
const SOURCE_MIN_MAX_ELEMENTS: &[u32] = &[
    root::ID_SETTINGS_SOURCE_LABEL,
    #[cfg(not(target_os = "macos"))]
    root::ID_SETTINGS_SOURCE_GROUP,
    root::ID_SETTINGS_SOURCE_MIN_LABEL,
    root::ID_SETTINGS_SOURCE_MAX_LABEL,
    root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL,
    root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL,
    root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
    root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
];
const OUT_OF_RANGE_ELEMENTS: &[u32] = &[
    root::ID_MODE_OUT_OF_RANGE_LABEL_TEXT,
    root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX,
];
const GROUP_INTERACTION_ELEMENTS: &[u32] = &[
    root::ID_MODE_GROUP_INTERACTION_LABEL_TEXT,
    root::ID_MODE_GROUP_INTERACTION_COMBO_BOX,
];
const VALUE_SEQUENCE_ELEMENTS: &[u32] = &[
    root::ID_SETTINGS_TARGET_SEQUENCE_LABEL_TEXT,
    root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL,
];
const TARGET_MIN_MAX_ELEMENTS: &[u32] = &[
    root::ID_SETTINGS_TARGET_LABEL_TEXT,
    #[cfg(not(target_os = "macos"))]
    root::ID_SETTINGS_TARGET_GROUP,
    root::ID_SETTINGS_MIN_TARGET_LABEL_TEXT,
    root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL,
    root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL,
    root::ID_SETTINGS_MIN_TARGET_VALUE_TEXT,
    root::ID_SETTINGS_MAX_TARGET_LABEL_TEXT,
    root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
    root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
    root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
];
const FEEDBACK_TYPE_ELEMENTS: &[u32] = &[root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX];
const ROUND_TARGET_VALUE_ELEMENTS: &[u32] = &[root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX];
const TAKEOVER_MODE_ELEMENTS: &[u32] = &[root::ID_MODE_TAKEOVER_LABEL, root::ID_MODE_TAKEOVER_MODE];
const CONTROL_TRANSFORMATION_ELEMENTS: &[u32] = &[
    root::ID_MODE_EEL_CONTROL_TRANSFORMATION_LABEL,
    root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL,
    root::ID_MODE_EEL_CONTROL_TRANSFORMATION_DETAIL_BUTTON,
];
const ABSOLUTE_MODE_ELEMENTS: &[u32] = &[
    root::ID_SETTINGS_MODE_COMBO_BOX,
    root::ID_SETTINGS_MODE_LABEL,
];
const STEP_MIN_ELEMENTS: &[u32] = &[
    root::ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT,
    #[cfg(not(target_os = "macos"))]
    root::ID_SETTINGS_STEP_SIZE_GROUP,
    root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
    root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
    root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
];
const STEP_MAX_ELEMENTS: &[u32] = &[
    root::ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT,
    root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
    root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
    root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
];
const MAKE_ABSOLUTE_ELEMENTS: &[u32] = &[root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX];
const RELATIVE_FILTER_ELEMENTS: &[u32] = &[root::ID_MODE_RELATIVE_FILTER_COMBO_BOX];
const FIRE_MODE_ELEMENTS: &[u32] = &[
    root::ID_MODE_FIRE_COMBO_BOX,
    root::ID_MODE_FIRE_LINE_2_LABEL_1,
    root::ID_MODE_FIRE_LINE_2_SLIDER_CONTROL,
    root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL,
    root::ID_MODE_FIRE_LINE_2_LABEL_2,
    root::ID_MODE_FIRE_LINE_3_LABEL_1,
    root::ID_MODE_FIRE_LINE_3_SLIDER_CONTROL,
    root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL,
    root::ID_MODE_FIRE_LINE_3_LABEL_2,
];
const REVERSE_ELEMENTS: &[u32] = &[root::ID_SETTINGS_REVERSE_CHECK_BOX];
const FEEDBACK_TRANSFORMATION_ELEMENTS: &[u32] = &[
    root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL,
    root::IDC_MODE_FEEDBACK_TYPE_BUTTON,
];
const ROTATE_ELEMENTS: &[u32] = &[root::ID_SETTINGS_ROTATE_CHECK_BOX];
const BUTTON_FILTER_ELEMENTS: &[u32] = &[root::ID_MODE_BUTTON_FILTER_COMBO_BOX];

impl<'a> ImmutableMappingPanel<'a> {
    // For hitting target with on/off or -/+ depending on target character.
    fn hit_target_special(&self, state: bool) -> Result<(), &'static str> {
        let target = self
            .first_resolved_target()
            .ok_or("couldn't resolve to target")?;
        let target_is_relative = target
            .control_type(self.session.control_context())
            .is_relative();
        let value = if target_is_relative {
            let amount = if state { 1 } else { -1 };
            ControlValue::RelativeDiscrete(DiscreteIncrement::new(amount))
        } else {
            let value = if state {
                UnitValue::MAX
            } else {
                UnitValue::MIN
            };
            ControlValue::AbsoluteContinuous(value)
        };
        self.session.hit_target(self.mapping.qualified_id(), value);
        Ok(())
    }

    fn hit_target(&self, value: UnitValue) {
        self.session.hit_target(
            self.mapping.qualified_id(),
            ControlValue::AbsoluteContinuous(value),
        );
    }

    fn fill_all_controls(&self) {
        self.fill_mapping_feedback_send_behavior_combo_box();
        self.fill_source_category_combo_box();
        self.fill_mode_out_of_range_behavior_combo_box();
        self.fill_mode_group_interaction_combo_box();
        self.fill_mode_takeover_mode_combo_box();
        self.fill_mode_button_usage_combo_box();
        self.fill_mode_encoder_usage_combo_box();
        self.fill_mode_fire_mode_combo_box();
        self.fill_mode_feedback_type_combo_box();
        self.fill_target_category_combo_box();
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_window_title();
        self.panel.mapping_header_panel.invalidate_controls();
        self.invalidate_beep_on_success_checkbox();
        self.invalidate_mapping_enabled_check_box();
        self.invalidate_mapping_feedback_send_behavior_combo_box();
        self.invalidate_mapping_visible_in_projection_check_box();
        self.invalidate_mapping_advanced_settings_button();
        self.invalidate_source_controls();
        self.invalidate_target_controls(None);
        self.invalidate_mode_controls();
    }

    fn invalidate_help(&self) {
        if self.invalidate_help_internal().is_none() {
            self.clear_help();
        }
    }

    fn invalidate_help_internal(&self) -> Option<()> {
        let topic = self.panel.active_help_topic.borrow().get()?;
        let details = topic.get_details()?;
        const HAS_NO_EFFECT: &str = "has no effect";
        let left_label = self
            .view
            .require_control(root::ID_MAPPING_HELP_LEFT_SUBJECT_LABEL);
        let left_content = self
            .view
            .require_control(root::ID_MAPPING_HELP_LEFT_CONTENT_LABEL);
        let right_label = self
            .view
            .require_control(root::ID_MAPPING_HELP_RIGHT_SUBJECT_LABEL);
        let right_content = self
            .view
            .require_control(root::ID_MAPPING_HELP_RIGHT_CONTENT_LABEL);
        match details.description {
            HelpTopicDescription::General(desc) => {
                left_label.set_text(format!("{topic}:"));
                left_content.set_multi_line_text(format!("This {desc}."));
                right_label.set_text("");
                right_content.set_multi_line_text("");
            }
            HelpTopicDescription::DirectionDependent {
                control_desc,
                feedback_desc,
            } => {
                left_label.set_text(format!("In control direction, \"{topic}\" ..."));
                left_content
                    .set_multi_line_text(format!("... {}.", control_desc.unwrap_or(HAS_NO_EFFECT)));
                right_label.set_text("In feedback direction, ... (Press F1 to learn more)");
                right_content.set_multi_line_text(format!(
                    "... it {}.",
                    feedback_desc.unwrap_or(HAS_NO_EFFECT)
                ));
            }
        }
        Some(())
    }

    fn clear_help(&self) {
        self.view
            .require_control(root::ID_MAPPING_HELP_LEFT_SUBJECT_LABEL)
            .set_text("");
        for id in [
            root::ID_MAPPING_HELP_RIGHT_SUBJECT_LABEL,
            root::ID_MAPPING_HELP_LEFT_CONTENT_LABEL,
            root::ID_MAPPING_HELP_RIGHT_CONTENT_LABEL,
        ] {
            self.view.require_control(id).set_text("");
        }
    }

    fn invalidate_window_title(&self) {
        let mapping_is_on = self
            .session
            .unit()
            .borrow()
            .mapping_is_on(self.mapping.qualified_id());
        let suffix = if mapping_is_on { "" } else { " (inactive)" };
        let text = format!("Mapping \"{}\"{}", self.mapping.effective_name(), suffix);
        self.view.require_window().set_text(text);
    }

    fn invalidate_mapping_feedback_send_behavior_combo_box(&self) {
        let combo = self
            .view
            .require_control(root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX);
        combo.select_combo_box_item_by_index(self.mapping.feedback_send_behavior().into());
    }

    fn invalidate_beep_on_success_checkbox(&self) {
        self.view
            .require_control(root::IDC_BEEP_ON_SUCCESS_CHECK_BOX)
            .set_checked(self.mapping.beep_on_success());
    }

    fn invalidate_mapping_enabled_check_box(&self) {
        self.view
            .require_control(root::IDC_MAPPING_ENABLED_CHECK_BOX)
            .set_checked(self.mapping.is_enabled());
    }

    fn invalidate_mapping_visible_in_projection_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX);
        cb.set_checked(self.mapping.visible_in_projection());
    }

    fn invalidate_mapping_advanced_settings_button(&self) {
        let cb = self.view.require_control(root::ID_MAPPING_ADVANCED_BUTTON);
        let suffix = if let Some(m) = self.mapping.advanced_settings() {
            format!(" ({})", m.len())
        } else {
            "".to_owned()
        };
        let text = format!("Advanced settings {suffix}");
        cb.set_text(text);
    }

    fn invalidate_source_controls(&self) {
        self.invalidate_source_control_visibilities();
        self.invalidate_source_learn_button();
        self.invalidate_source_category_combo_box();
        self.invalidate_source_line_2();
        self.invalidate_source_line_3(None);
        self.invalidate_source_line_4(None);
        self.invalidate_source_line_5(None);
        self.invalidate_source_check_box_2();
        self.invalidate_source_line_7(None);
    }

    fn invalidate_source_control_visibilities(&self) {
        let source = self.source;
        // Show/hide stuff
        self.enable_if(
            source.supports_type(),
            &[
                root::ID_SOURCE_TYPE_LABEL_TEXT,
                root::ID_SOURCE_TYPE_COMBO_BOX,
            ],
        );
    }

    fn enable_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            if let Some(control) = self.view.require_window().find_control(*id) {
                self.panel
                    .ui_element_container
                    .borrow_mut()
                    .set_visible(*id, condition);
                control.set_visible(condition);
            }
        }
    }

    fn invalidate_source_category_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.source.category().into());
    }

    fn invalidate_target_category_combo_box(&self) {
        // Don't allow main mappings to have virtual target
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .set_visible(self.mapping.compartment() != CompartmentKind::Main);
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.target.category().into());
    }

    fn invalidate_source_line_2(&self) {
        self.invalidate_source_line_2_combo_box();
    }

    fn invalidate_source_line_2_combo_box(&self) {
        self.fill_source_type_combo_box();
        self.invalidate_source_type_combo_box_value();
    }

    fn invalidate_source_type_combo_box_value(&self) {
        use SourceCategory::*;
        let item_index = match self.source.category() {
            Midi => self.source.midi_source_type().into(),
            Reaper => self.source.reaper_source_type().into(),
            Virtual => self.source.control_element_character().into(),
            _ => return,
        };
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        b.select_combo_box_item_by_index(item_index);
    }

    fn invalidate_source_learn_button(&self) {
        self.invalidate_learn_button(
            self.session
                .unit()
                .borrow()
                .mapping_is_learning_source(self.mapping.qualified_id()),
            root::ID_SOURCE_LEARN_BUTTON,
        );
    }

    fn invalidate_learn_button(&self, is_learning: bool, control_resource_id: u32) {
        let text = if is_learning {
            "Stop learning"
        } else {
            "Learn"
        };
        self.view
            .require_control(control_resource_id)
            .set_text(text);
    }

    fn invalidate_source_line_3(&self, initiator: Option<u32>) {
        self.invalidate_source_line_3_label_1();
        self.invalidate_source_line_3_label_2();
        self.invalidate_source_line_3_combo_box();
        self.invalidate_source_line_3_edit_control(initiator);
    }

    fn invalidate_source_line_3_label_1(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::Script => Some("Kind"),
                t if t.supports_channel() => Some("Channel"),
                _ => None,
            },
            Osc => Some("Address"),
            Reaper => match self.source.reaper_source_type() {
                ReaperSourceType::Timer => Some("Millis"),
                ReaperSourceType::RealearnParameter => Some("Param"),
                _ => None,
            },
            Keyboard => Some("Key"),
            StreamDeck => Some("Button"),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_CHANNEL_LABEL)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_3_label_2(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::ClockTransport => Some("Message"),
                MidiSourceType::Display => Some("Protocol"),
                _ => None,
            },
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_MIDI_MESSAGE_TYPE_LABEL_TEXT)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_3_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::ClockTransport => {
                    b.show();
                    b.fill_combo_box_indexed(MidiClockTransportMessage::iter());
                    b.select_combo_box_item_by_index(
                        self.source.midi_clock_transport_message().into(),
                    );
                }
                MidiSourceType::Display => {
                    b.show();
                    b.fill_combo_box_indexed(DisplayType::iter());
                    b.select_combo_box_item_by_index(self.source.display_type().into());
                }
                MidiSourceType::Script => {
                    b.fill_combo_box_indexed(MidiScriptKind::iter());
                    b.show();
                    b.select_combo_box_item_by_index(self.source.midi_script_kind().into());
                }
                t if t.supports_channel() => {
                    b.fill_combo_box_with_data_small(
                        iter::once((-1isize, "<Any> (no feedback)".to_string()))
                            .chain((0..16).map(|i| (i as isize, (i + 1).to_string()))),
                    );
                    b.show();
                    match self.source.channel() {
                        None => {
                            b.select_combo_box_item_by_data(-1).unwrap();
                        }
                        Some(ch) => {
                            b.select_combo_box_item_by_data(ch.get() as _).unwrap();
                        }
                    };
                }
                _ => b.hide(),
            },
            Reaper => match self.source.reaper_source_type() {
                ReaperSourceType::RealearnParameter => {
                    let contents = compartment_parameter_dropdown_contents(
                        self.session,
                        self.mapping.compartment(),
                    );
                    b.fill_combo_box_with_data_small(contents);
                    b.show();
                    b.select_combo_box_item_by_index(self.source.parameter_index().get() as usize);
                }
                _ => b.hide(),
            },
            StreamDeck => {
                b.fill_combo_box_indexed((0..32).map(|i| (i + 1).to_string()));
                b.select_combo_box_item_by_index(self.source.button_index() as _);
                b.show();
            }
            _ => {
                b.hide();
            }
        };
    }

    fn invalidate_source_check_box_2(&self) {
        use SourceCategory::*;
        let state = match self.source.category() {
            Midi => {
                match self.source.midi_source_type() {
                    t if t.supports_14_bit() => {
                        Some((
                            "14-bit values",
                            self.source
                                .is_14_bit()
                                // 14-bit == None not yet supported
                                .unwrap_or(false),
                        ))
                    }
                    _ => None,
                }
            }
            Osc => Some(("Is relative", self.source.osc_arg_is_relative())),
            _ => None,
        };
        self.invalidate_check_box(root::ID_SOURCE_14_BIT_CHECK_BOX, state);
    }

    fn invalidate_source_line_4_check_box(&self) {
        use SourceCategory::*;
        let state = match self.source.category() {
            Midi => {
                match self.source.midi_source_type() {
                    t if t.supports_is_registered() => Some((
                        "RPN",
                        self.source
                            .is_registered()
                            // registered == None not yet supported
                            .unwrap_or(false),
                    )),
                    _ => None,
                }
            }
            _ => None,
        };
        self.invalidate_check_box(root::ID_SOURCE_RPN_CHECK_BOX, state);
    }

    fn invalidate_source_line_4(&self, initiator: Option<u32>) {
        self.invalidate_source_line_4_label();
        self.invalidate_source_line_4_button();
        self.invalidate_source_line_4_check_box();
        self.invalidate_source_line_4_combo_box_1();
        self.invalidate_source_line_4_combo_box_2();
        self.invalidate_source_line_4_edit_control(initiator);
    }

    fn invalidate_source_line_4_button(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            StreamDeck => Some(self.source.button_background_type().to_string()),
            Virtual => Some("Pick!".to_string()),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_LINE_4_BUTTON)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_4_label(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => {
                use DisplayType::*;
                use MidiSourceType::*;
                match self.source.midi_source_type() {
                    Display => {
                        if matches!(self.source.display_type(), LaunchpadProScrollingText) {
                            None
                        } else {
                            Some("Display")
                        }
                    }
                    t if t.supports_midi_message_number()
                        || t.supports_parameter_number_message_number() =>
                    {
                        Some(t.number_label())
                    }
                    _ => None,
                }
            }
            Virtual => Some("ID"),
            Osc => Some("Argument"),
            StreamDeck => Some("Back"),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_4_combo_box_1(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_LINE_4_COMBO_BOX_1);
        use SourceCategory::*;
        match self.source.category() {
            Osc => {
                invalidate_with_osc_arg_index(b, self.source.osc_arg_index());
            }
            _ => {
                b.hide();
            }
        };
    }

    fn invalidate_source_line_4_combo_box_2(&self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => {
                use MidiSourceType::*;
                match self.source.midi_source_type() {
                    Display => {
                        use DisplayType::*;
                        match self.source.display_type() {
                            MackieLcd | MackieXtLcd | XTouchMackieLcd | XTouchMackieXtLcd
                            | SiniConE24 | SlKeyboardDisplay => {
                                b.show();
                                let display_count = self.source.display_count() as isize;
                                b.fill_combo_box_with_data_vec(
                                    iter::once((-1isize, "<All>".to_string()))
                                        .chain((0..display_count).map(|i| (i, (i + 1).to_string())))
                                        .collect(),
                                );
                                let data = match self.source.mackie_lcd_scope().channel {
                                    None => -1,
                                    Some(n) => n as _,
                                };
                                b.select_combo_box_item_by_data(data).unwrap();
                            }
                            MackieSevenSegmentDisplay => {
                                b.show();
                                b.fill_combo_box_indexed(MackieSevenSegmentDisplayScope::iter());
                                b.select_combo_box_item_by_index(
                                    self.source.mackie_7_segment_display_scope().into(),
                                );
                            }
                            LaunchpadProScrollingText => {
                                b.hide();
                            }
                        }
                    }
                    t if t.supports_midi_message_number() => {
                        b.fill_combo_box_with_data_vec(
                            iter::once((-1isize, "<Any> (no feedback)".to_string()))
                                .chain((0..128).map(|i| (i as isize, i.to_string())))
                                .collect(),
                        );
                        b.show();
                        let data = match self.source.midi_message_number() {
                            None => -1,
                            Some(n) => n.get() as _,
                        };
                        b.select_combo_box_item_by_data(data).unwrap();
                    }
                    _ => {
                        b.hide();
                    }
                }
            }
            Osc if self.source.osc_arg_index().is_some() => {
                let tag = self.source.osc_arg_type_tag();
                invalidate_with_osc_arg_type_tag(b, tag);
            }
            _ => {
                b.hide();
            }
        };
    }

    fn invalidate_source_line_4_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_SOURCE_NUMBER_EDIT_CONTROL) {
            return;
        }
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                t if t.supports_parameter_number_message_number() => {
                    match self.source.parameter_number_message_number() {
                        None => Some("".to_owned()),
                        Some(n) => Some(n.to_string()),
                    }
                }
                _ => None,
            },
            StreamDeck if self.source.button_background_type().wants_image() => {
                Some(self.source.button_background_image_path().to_string())
            }
            Virtual => Some(self.source.control_element_id().to_string()),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL)
            .set_text_or_hide(text)
    }

    fn invalidate_source_line_7(&self, initiator: Option<u32>) {
        self.invalidate_source_line_7_label();
        self.invalidate_source_line_7_edit_control(initiator);
        self.invalidate_source_line_7_button();
    }

    fn invalidate_source_line_7_button(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi if matches!(
                self.source.midi_source_type(),
                MidiSourceType::Raw | MidiSourceType::Script
            ) =>
            {
                Some("...")
            }
            Osc | StreamDeck => Some("..."),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_SCRIPT_DETAIL_BUTTON)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_7_label(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::Raw => Some("Pattern"),
                MidiSourceType::Script => Some("Script"),
                _ => None,
            },
            Osc => Some("Feedback arguments"),
            StreamDeck => Some("Default text"),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_OSC_ADDRESS_LABEL_TEXT)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_3_edit_control(&self, initiator: Option<u32>) {
        let control_id = root::ID_SOURCE_LINE_3_EDIT_CONTROL;
        if initiator == Some(control_id) {
            return;
        }
        let c = self.view.require_control(control_id);
        use SourceCategory::*;
        let content = match self.source.category() {
            Osc => Some((self.source.osc_address_pattern().to_owned(), true)),
            Reaper => match self.source.reaper_source_type() {
                ReaperSourceType::Timer => Some((self.source.timer_millis().to_string(), true)),
                _ => None,
            },
            Keyboard => {
                let text = self
                    .source
                    .create_key_source()
                    .map(|s| {
                        let comment = match s.stroke().portability() {
                            Some(KeyStrokePortability::Portable) => "",
                            Some(KeyStrokePortability::NonPortable(issue)) => match issue {
                                PortabilityIssue::OperatingSystemRelated => {
                                    " (not cross-platform!)"
                                }
                                PortabilityIssue::KeyboardLayoutRelated => {
                                    " (not portable, special character!)"
                                }
                                PortabilityIssue::Other => " (not portable!)",
                                PortabilityIssue::NotNormalized => " (not normalized!)",
                            },
                            None => " (probably not portable)",
                        };
                        format!("{}{}", s.stroke(), comment)
                    })
                    .unwrap_or_else(|| KEY_UNDEFINED_LABEL.to_string());
                Some((text, false))
            }
            _ => None,
        };
        if let Some((value_text, enabled)) = content {
            c.show();
            c.set_enabled(enabled);
            c.set_text(value_text);
        } else {
            c.hide();
        }
    }

    fn invalidate_source_line_7_edit_control(&self, initiator: Option<u32>) {
        let control_id = root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL;
        if initiator == Some(control_id) {
            return;
        }
        let c = self.view.require_control(control_id);
        use SourceCategory::*;
        let (value_text, read_only) = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::Raw => {
                    let text = self.source.raw_midi_pattern();
                    (Some(text.to_owned()), text.chars().count() > 30)
                }
                MidiSourceType::Script => {
                    let text = self.source.midi_script();
                    (
                        Some(extract_first_line(text).to_owned()),
                        has_multiple_lines(text),
                    )
                }
                _ => (None, false),
            },
            Osc => {
                let text = format_osc_feedback_args(self.source.osc_feedback_args());
                (Some(text), false)
            }
            StreamDeck => {
                let text = self.source.button_static_text();
                (Some(text.to_string()), has_multiple_lines(text))
            }
            _ => (None, false),
        };
        c.set_text_or_hide(value_text);
        c.set_enabled(!read_only);
    }

    fn invalidate_source_line_5(&self, initiator: Option<u32>) {
        self.invalidate_source_line_5_label();
        self.invalidate_source_line_5_button();
        self.invalidate_source_line_5_combo_box();
        self.invalidate_source_line_5_edit_control(initiator);
    }

    fn invalidate_source_line_5_label(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => {
                use MidiSourceType::*;
                match self.source.midi_source_type() {
                    Display => {
                        if self.source.display_type().line_count() > 1 {
                            Some("Line")
                        } else {
                            None
                        }
                    }
                    t if t.supports_custom_character() => Some("Character"),
                    _ => None,
                }
            }
            Osc if self.source.supports_osc_arg_value_range() => Some("Range"),
            StreamDeck => Some("Front"),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_LABEL_TEXT)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_5_button(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            StreamDeck => Some(self.source.button_foreground_type().to_string()),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_LINE_5_BUTTON)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_5_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => {
                use MidiSourceType::*;
                match self.source.midi_source_type() {
                    Display => {
                        b.show();
                        let line_count = self.source.display_type().line_count();
                        if line_count > 1 {
                            b.fill_combo_box_with_data_vec(
                                iter::once((-1isize, "<Multiline>".to_string()))
                                    .chain(
                                        (0..line_count).map(|i| (i as isize, (i + 1).to_string())),
                                    )
                                    .collect(),
                            );
                            let data = match self.source.line() {
                                None => -1,
                                Some(n) => n.min(line_count) as _,
                            };
                            b.select_combo_box_item_by_data(data).unwrap();
                        } else {
                            b.hide();
                        }
                    }
                    t if t.supports_custom_character() => {
                        b.show();
                        b.fill_combo_box_indexed(SourceCharacter::iter());
                        b.select_combo_box_item_by_index(self.source.custom_character().into());
                    }
                    _ => {
                        b.hide();
                    }
                }
            }
            _ => {
                b.hide();
            }
        };
    }

    fn invalidate_source_line_5_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_SOURCE_LINE_5_EDIT_CONTROL) {
            return;
        }
        use SourceCategory::*;
        let text = match self.source.category() {
            Osc if self.source.supports_osc_arg_value_range() => {
                let text = format_osc_arg_value_range(
                    self.source.osc_arg_value_range(),
                    self.source.osc_arg_type_tag(),
                );
                Some(text)
            }
            StreamDeck if self.source.button_foreground_type().wants_image() => {
                Some(self.source.button_foreground_image_path().to_string())
            }
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_LINE_5_EDIT_CONTROL)
            .set_text_or_hide(text);
    }

    fn invalidate_target_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_category_combo_box();
        self.invalidate_target_type_button();
        self.invalidate_target_line_2(initiator);
        self.invalidate_target_line_3(initiator);
        self.invalidate_target_line_4(initiator);
        self.invalidate_target_line_5(initiator);
        self.invalidate_target_value_controls();
        self.invalidate_target_learn_button();
        self.invalidate_target_check_boxes();
    }

    fn invalidate_target_check_boxes(&self) {
        self.invalidate_target_check_box_1();
        self.invalidate_target_check_box_2();
        self.invalidate_target_check_box_3();
        self.invalidate_target_check_box_4();
        self.invalidate_target_check_box_5();
        self.invalidate_target_check_box_6();
    }

    fn invalidate_target_type_button(&self) {
        use TargetCategory::*;
        let (label, hint) = match self.target.category() {
            Reaper => {
                let label = self.target.target_type().to_string();
                let hint = match self.target.target_type().definition() {
                    d if d.lua_only => "Use Lua to configure this target!",
                    d if d.supports_real_time_control => "Supports MIDI real-time control!",
                    d => d.hint,
                };
                (label, hint.to_string())
            }
            Virtual => {
                let label = self.target.control_element_character().to_string();
                (label, "".to_owned())
            }
        };
        self.view
            .require_control(root::ID_TARGET_TYPE_BUTTON)
            .set_text(label);
        self.view
            .require_control(root::ID_TARGET_HINT)
            .set_text(hint);
    }

    fn target_category(&self) -> TargetCategory {
        self.target.category()
    }

    fn reaper_target_type(&self) -> ReaperTargetType {
        self.target.target_type()
    }

    fn invalidate_target_line_2_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::LastTouched => Some("Targets"),
                ReaperTargetType::BrowsePotFilterItems => Some("Kind"),
                ReaperTargetType::CompartmentParameterValue => Some("Parameter"),
                ReaperTargetType::Mouse => Some("Action"),
                ReaperTargetType::Transport => Some("Action"),
                ReaperTargetType::AnyOn => Some("Parameter"),
                ReaperTargetType::AutomationModeOverride => Some("Behavior"),
                ReaperTargetType::GoToBookmark => match self.target.bookmark_type() {
                    BookmarkType::Marker => Some("Marker"),
                    BookmarkType::Region => Some("Region"),
                },
                ReaperTargetType::SendMidi => Some("Output"),
                ReaperTargetType::SendOsc => Some("Output"),
                ReaperTargetType::LoadMappingSnapshot => Some("Snapshot"),
                ReaperTargetType::TakeMappingSnapshot => Some("Snapshot ID"),
                ReaperTargetType::BrowseGroup => Some("Group"),
                ReaperTargetType::BrowseTracks => Some("Scope"),
                ReaperTargetType::ModifyMapping => Some("Kind"),
                ReaperTargetType::PlaytimeMatrixAction
                | ReaperTargetType::PlaytimeRowAction
                | ReaperTargetType::PlaytimeColumnAction
                | ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction => Some("Action"),
                t if t.supports_feedback_resolution() => Some("Feedback"),
                _ if self.target.supports_track() => Some("Track"),
                _ => None,
            },
            TargetCategory::Virtual => Some("ID"),
        };
        self.view
            .require_control(root::ID_TARGET_LINE_2_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_2_label_2(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::LastTouched => {
                    let enabled_count = self.target.included_targets().len();
                    let total_count = LearnableTargetKind::iter().count();
                    Some(format!("{enabled_count} of {total_count} targets enabled"))
                }
                ReaperTargetType::CompartmentParameterValue => {
                    let param_index = self.target.compartment_param_index();
                    let unit = self.session.unit().borrow();
                    let params = unit.parameter_manager().params();
                    let compartment_params = params.compartment_params(self.mapping.compartment());
                    Some(get_param_name(compartment_params, param_index))
                }
                _ => None,
            },
            _ => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_2_LABEL_2)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_2_label_3(&self) {
        invalidate_target_line_2_expression_result(
            self.target,
            self.session.extended_context(),
            self.view.require_control(root::ID_TARGET_LINE_2_LABEL_3),
            self.mapping.compartment(),
        );
    }

    fn invalidate_target_line_2_combo_box_1(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_2_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.target.target_type() {
                _ if self.target.supports_track() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualTrackType::iter());
                    combo.select_combo_box_item_by_index(self.target.track_type().into());
                }
                ReaperTargetType::GoToBookmark => {
                    combo.show();
                    combo.fill_combo_box_indexed(BookmarkAnchorType::iter());
                    combo.select_combo_box_item_by_index(self.target.bookmark_anchor_type().into());
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    combo.show();
                    combo.fill_combo_box_indexed(MappingSnapshotTypeForLoad::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping
                            .target_model
                            .mapping_snapshot_type_for_load()
                            .into(),
                    );
                }
                ReaperTargetType::TakeMappingSnapshot => {
                    combo.show();
                    combo.fill_combo_box_indexed(MappingSnapshotTypeForTake::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping
                            .target_model
                            .mapping_snapshot_type_for_take()
                            .into(),
                    );
                }
                t if t.supports_feedback_resolution() => {
                    combo.show();
                    combo.fill_combo_box_indexed(FeedbackResolution::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.feedback_resolution().into(),
                    );
                }
                _ => {
                    combo.hide();
                }
            },
            TargetCategory::Virtual => {
                combo.hide();
            }
        }
    }

    fn invalidate_target_line_2_combo_box_2(&self, initiator: Option<u32>) {
        let combo_id = root::ID_TARGET_LINE_2_COMBO_BOX_2;
        if initiator == Some(combo_id) {
            return;
        }
        let combo = self.view.require_control(combo_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Mouse => {
                    combo.show();
                    combo.fill_combo_box_indexed(MouseActionType::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.mouse_action_type().into(),
                    );
                }
                ReaperTargetType::Transport => {
                    combo.show();
                    combo.fill_combo_box_indexed(TransportAction::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.transport_action().into(),
                    );
                }
                ReaperTargetType::AnyOn => {
                    combo.show();
                    combo.fill_combo_box_indexed(AnyOnParameter::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.any_on_parameter().into(),
                    );
                }
                ReaperTargetType::AutomationModeOverride => {
                    combo.show();
                    combo.fill_combo_box_indexed(AutomationModeOverrideType::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping
                            .target_model
                            .automation_mode_override_type()
                            .into(),
                    );
                }
                ReaperTargetType::GoToBookmark
                    if self.target.bookmark_anchor_type() == BookmarkAnchorType::Id =>
                {
                    combo.show();
                    let project = self.target_with_context().project();
                    let bookmark_type = self.target.bookmark_type();
                    let bookmarks = bookmark_combo_box_entries(project, bookmark_type);
                    combo.fill_combo_box_with_data_vec(bookmarks.collect());
                    select_bookmark_in_combo_box(
                        combo,
                        self.target.bookmark_anchor_type(),
                        self.target.bookmark_ref(),
                    );
                }
                ReaperTargetType::BrowseGroup => {
                    combo.show();
                    let compartment = self.mapping.compartment();
                    // Fill box
                    combo.fill_combo_box_with_data_small(
                        self.session
                            .groups_sorted(compartment)
                            .enumerate()
                            .map(|(i, g)| (i as isize, g.borrow().to_string())),
                    );
                    // Select value
                    let group_id = self.target.group_id();
                    match self
                        .session
                        .find_group_index_by_id_sorted(compartment, group_id)
                    {
                        None => {
                            combo.select_new_combo_box_item(format!("<Not present> ({group_id})"));
                        }
                        Some(i) => {
                            combo.select_combo_box_item_by_data(i as isize).unwrap();
                        }
                    }
                }
                ReaperTargetType::SendMidi => {
                    combo.show();
                    combo.fill_combo_box_indexed(SendMidiDestinationType::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping
                            .target_model
                            .send_midi_destination_type()
                            .into(),
                    );
                }
                ReaperTargetType::SendOsc => {
                    combo.show();
                    let osc_device_manager = BackboneShell::get().osc_device_manager();
                    let osc_device_manager = osc_device_manager.borrow();
                    let osc_devices = osc_device_manager.devices();
                    combo.fill_combo_box_with_data_small(
                        std::iter::once((-1isize, "<Feedback output>".to_string())).chain(
                            osc_devices
                                .enumerate()
                                .map(|(i, dev)| (i as isize, dev.get_list_label(true))),
                        ),
                    );
                    if let Some(dev_id) = self.mapping.target_model.osc_dev_id() {
                        match osc_device_manager.find_index_by_id(&dev_id) {
                            None => {
                                combo
                                    .select_new_combo_box_item(format!("<Not present> ({dev_id})"));
                            }
                            Some(i) => combo.select_combo_box_item_by_data(i as isize).unwrap(),
                        }
                    } else {
                        combo.select_combo_box_item_by_data(-1).unwrap();
                    };
                }
                ReaperTargetType::BrowseTracks => {
                    combo.show();
                    combo.fill_combo_box_indexed(BrowseTracksMode::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.browse_tracks_mode().into(),
                    );
                }
                ReaperTargetType::BrowsePotFilterItems => {
                    combo.show();
                    combo.fill_combo_box_indexed(PotFilterKind::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.pot_filter_item_kind().into(),
                    );
                }
                ReaperTargetType::ModifyMapping => {
                    combo.show();
                    combo.fill_combo_box_indexed(MappingModificationKind::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.mapping_modification_kind().into(),
                    );
                }
                ReaperTargetType::PlaytimeMatrixAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeMatrixAction::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.playtime_matrix_action().into(),
                    );
                }
                ReaperTargetType::PlaytimeColumnAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeColumnAction::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.playtime_column_action().into(),
                    );
                }
                ReaperTargetType::PlaytimeRowAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeRowAction::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping.target_model.playtime_row_action().into(),
                    );
                }
                ReaperTargetType::PlaytimeSlotManagementAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeSlotManagementAction::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping
                            .target_model
                            .playtime_slot_management_action()
                            .into(),
                    );
                }
                ReaperTargetType::PlaytimeSlotTransportAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeSlotTransportAction::iter());
                    combo.select_combo_box_item_by_index(
                        self.mapping
                            .target_model
                            .playtime_slot_transport_action()
                            .into(),
                    );
                }
                _ if self.target.supports_track() => {
                    if matches!(
                        self.target.track_type(),
                        VirtualTrackType::ById | VirtualTrackType::ByIdOrName
                    ) {
                        combo.show();
                        let context = self.session.extended_context();
                        let project = context.context().project_or_current_project();
                        // Fill
                        combo.fill_combo_box_indexed(track_combo_box_entries(project));
                        // Set
                        if let Some(virtual_track) = self.target.virtual_track() {
                            if let Some(resolved_track) = virtual_track
                                .resolve(context, self.mapping.compartment())
                                .ok()
                                .and_then(|tracks| tracks.into_iter().next())
                            {
                                let i = resolved_track.index().unwrap();
                                combo.select_combo_box_item_by_index(i as _);
                            } else {
                                combo.select_new_combo_box_item(
                                    get_non_present_virtual_track_label(&virtual_track),
                                );
                            }
                        } else {
                            combo.select_new_combo_box_item("<None>");
                        }
                    } else {
                        combo.hide();
                    }
                }
                _ => {
                    combo.hide();
                }
            },
            TargetCategory::Virtual => {
                combo.hide();
            }
        }
    }

    fn invalidate_target_line_2_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_TARGET_LINE_2_EDIT_CONTROL) {
            return;
        }
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_2_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                _ if self.target.supports_track() => {
                    control.show();
                    let text = match self.target.track_type() {
                        t if t.is_dynamic() => self.target.track_expression().to_owned(),
                        t if t.is_by_index() => {
                            let index = self.target.track_index();
                            (index + 1).to_string()
                        }
                        VirtualTrackType::ByName | VirtualTrackType::AllByName => {
                            self.target.track_name().to_owned()
                        }
                        _ => {
                            control.hide();
                            return;
                        }
                    };
                    control.set_text(text);
                }
                ReaperTargetType::GoToBookmark
                    if self.target.bookmark_anchor_type() == BookmarkAnchorType::Index =>
                {
                    control.show();
                    let text = (self.target.bookmark_ref() + 1).to_string();
                    control.set_text(text);
                }
                _ if self.mapping.target_model.supports_mapping_snapshot_id() => {
                    control.show();
                    let text = self
                        .target
                        .mapping_snapshot_id()
                        .map(|id| id.to_string())
                        .unwrap_or_default();
                    control.set_text(text);
                }
                _ => {
                    control.hide();
                }
            },
            TargetCategory::Virtual => {
                let text = self.target.control_element_id().to_string();
                control.set_text(text);
                control.show();
            }
        }
    }

    fn invalidate_target_line_2(&self, initiator: Option<u32>) {
        self.invalidate_target_line_2_label_1();
        self.invalidate_target_line_2_label_2();
        self.invalidate_target_line_2_label_3();
        self.invalidate_target_line_2_combo_box_1();
        self.invalidate_target_line_2_combo_box_2(initiator);
        self.invalidate_target_line_2_edit_control(initiator);
        self.invalidate_target_line_2_button();
    }

    fn invalidate_target_line_2_button(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => Some("Now!"),
                ReaperTargetType::LastTouched => Some("Pick!"),
                ReaperTargetType::CompartmentParameterValue => Some("Pick!"),
                ReaperTargetType::ModifyMapping => match self.target.mapping_modification_kind() {
                    MappingModificationKind::LearnTarget
                    | MappingModificationKind::SetTargetToLastTouched => Some("..."),
                },
                _ => None,
            },
            TargetCategory::Virtual => Some("Pick!"),
        };
        self.view
            .require_control(root::ID_TARGET_LINE_2_BUTTON)
            .set_text_or_hide(text);
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.extended_context(), self.mapping.compartment())
    }

    fn invalidate_target_line_3(&self, initiator: Option<u32>) {
        self.invalidate_target_line_3_label_1();
        self.invalidate_target_line_3_label_2();
        self.invalidate_target_line_3_label_3();
        self.invalidate_target_line_3_combo_box_1();
        self.invalidate_target_line_3_combo_box_2();
        self.invalidate_target_line_3_edit_control(initiator);
        self.invalidate_target_line_3_button();
    }

    fn invalidate_target_line_4(&self, initiator: Option<u32>) {
        self.invalidate_target_line_4_label_1();
        self.invalidate_target_line_4_label_2();
        self.invalidate_target_line_4_label_3();
        self.invalidate_target_line_4_combo_box_1();
        self.invalidate_target_line_4_combo_box_2();
        self.invalidate_target_line_4_edit_control(initiator);
        self.invalidate_target_line_4_button();
    }

    fn invalidate_target_line_5(&self, initiator: Option<u32>) {
        self.invalidate_target_line_5_label_1();
        self.invalidate_target_line_5_edit_control(initiator);
    }

    fn invalidate_target_line_3_button(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::ModifyMapping => Some("Pick!"),
                ReaperTargetType::SendMidi
                    if self.mapping.target_model.send_midi_destination_type()
                        == SendMidiDestinationType::InputDevice =>
                {
                    Some("Pick!")
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_3_BUTTON)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_4_button(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some("Pick!"),
                ReaperTargetType::LoadFxSnapshot => Some("Take!"),
                ReaperTargetType::ModifyMapping => Some("Pick!"),
                ReaperTargetType::SendMidi => Some("..."),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_4_BUTTON)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_3_label_3(&self) {
        invalidat_target_line_3_expression_result(
            self.target,
            self.session.extended_context(),
            self.view.require_control(root::ID_TARGET_LINE_3_LABEL_3),
            self.mapping.compartment(),
        );
    }

    fn invalidate_target_line_4_label_3(&self) {
        invalidate_target_line_4_expression_result(
            self.target,
            self.session.extended_context(),
            self.view.require_control(root::ID_TARGET_LINE_4_LABEL_3),
            self.mapping.compartment(),
        );
    }

    fn invalidate_target_line_5_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_TARGET_LINE_5_EDIT_CONTROL) {
            return;
        }
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc if self.target.supports_osc_arg_value_range() => {
                    let text = format_osc_arg_value_range(
                        self.target.osc_arg_value_range(),
                        self.target.osc_arg_type_tag(),
                    );
                    Some(text)
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_5_EDIT_CONTROL)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_4_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_TARGET_LINE_4_EDIT_CONTROL) {
            return;
        }
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_4_EDIT_CONTROL);
        let (text, read_only) = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendMidi => {
                    let text = self.target.raw_midi_pattern().to_owned();
                    let read_only = text.chars().count() > 30;
                    (Some(text), read_only)
                }
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotSeek
                | ReaperTargetType::PlaytimeSlotVolume => {
                    let text = match self.target.playtime_slot() {
                        PlaytimeSlotDescriptor::Active => None,
                        PlaytimeSlotDescriptor::ByIndex(a) => Some((a.row_index + 1).to_string()),
                        PlaytimeSlotDescriptor::Dynamic { row_expression, .. } => {
                            Some(row_expression.clone())
                        }
                    };
                    (text, false)
                }
                t if t.supports_fx_parameter() => {
                    let text = match self.target.param_type() {
                        VirtualFxParameterType::Dynamic => {
                            Some(self.target.param_expression().to_owned())
                        }
                        VirtualFxParameterType::ByName => Some(self.target.param_name().to_owned()),
                        VirtualFxParameterType::ByIndex => {
                            let index = self.target.param_index();
                            Some((index + 1).to_string())
                        }
                        VirtualFxParameterType::ById => None,
                    };
                    (text, false)
                }
                t if t.supports_send() => {
                    let text = match self.target.route_selector_type() {
                        TrackRouteSelectorType::Dynamic => {
                            Some(self.target.route_expression().to_owned())
                        }
                        TrackRouteSelectorType::ByName => Some(self.target.route_name().to_owned()),
                        TrackRouteSelectorType::ByIndex => {
                            let index = self.target.route_index();
                            Some((index + 1).to_string())
                        }
                        _ => None,
                    };
                    (text, false)
                }
                t if t.supports_tags() => {
                    let text = format_tags_as_csv(self.target.tags());
                    (Some(text), false)
                }
                _ => (None, false),
            },
            TargetCategory::Virtual => (None, false),
        };
        control.set_text_or_hide(text);
        control.set_enabled(!read_only);
    }

    fn invalidate_target_line_3_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_TARGET_LINE_3_EDIT_CONTROL) {
            return;
        }
        let c = self
            .view
            .require_control(root::ID_TARGET_LINE_3_EDIT_CONTROL);
        let (value_text, read_only) = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc => {
                    let text = self.target.osc_address_pattern().to_owned();
                    (Some(text), false)
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    let text = self
                        .target
                        .mapping_snapshot_default_value()
                        .map(|v| {
                            format!("{} %", format_as_percentage_without_unit(v.to_unit_value()))
                        })
                        .unwrap_or_default();
                    (Some(text), false)
                }
                ReaperTargetType::PlaytimeColumnAction => {
                    let text = match self.target.playtime_column() {
                        PlaytimeColumnDescriptor::Active => None,
                        PlaytimeColumnDescriptor::ByIndex(a) => Some((a.index + 1).to_string()),
                        PlaytimeColumnDescriptor::Dynamic { expression } => {
                            Some(expression.clone())
                        }
                    };
                    (text, false)
                }
                ReaperTargetType::PlaytimeRowAction => {
                    let text = match self.target.playtime_row() {
                        PlaytimeRowDescriptor::Active => None,
                        PlaytimeRowDescriptor::ByIndex(a) => Some((a.index + 1).to_string()),
                        PlaytimeRowDescriptor::Dynamic { expression } => Some(expression.clone()),
                    };
                    (text, false)
                }
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotSeek
                | ReaperTargetType::PlaytimeSlotVolume => {
                    let text = match self.target.playtime_slot() {
                        PlaytimeSlotDescriptor::Active => None,
                        PlaytimeSlotDescriptor::ByIndex(a) => {
                            Some((a.column_index + 1).to_string())
                        }
                        PlaytimeSlotDescriptor::Dynamic {
                            column_expression, ..
                        } => Some(column_expression.clone()),
                    };
                    (text, false)
                }
                t if t.supports_fx() => {
                    let text = match self.target.fx_type() {
                        VirtualFxType::Dynamic => Some(self.target.fx_expression().to_owned()),
                        VirtualFxType::ByIndex => {
                            let index = self.target.fx_index();
                            Some((index + 1).to_string())
                        }
                        VirtualFxType::ByName | VirtualFxType::AllByName => {
                            Some(self.target.fx_name().to_owned())
                        }
                        _ => None,
                    };
                    (text, false)
                }
                _ => (None, false),
            },
            TargetCategory::Virtual => (None, false),
        };
        c.set_text_or_hide(value_text);
        c.set_enabled(!read_only);
    }

    fn invalidate_target_line_3_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some("Invoke"),
                ReaperTargetType::TrackSolo => Some("Behavior"),
                ReaperTargetType::TrackShow => Some("Area"),
                ReaperTargetType::TrackTouchState => Some("Type"),
                ReaperTargetType::SendOsc => Some("Address"),
                ReaperTargetType::TrackMonitoringMode => Some("Mode"),
                ReaperTargetType::LoadMappingSnapshot => Some("Default"),
                ReaperTargetType::ModifyMapping => Some("Unit"),
                ReaperTargetType::SendMidi
                    if self.mapping.target_model.send_midi_destination_type()
                        == SendMidiDestinationType::InputDevice =>
                {
                    Some("Device")
                }
                ReaperTargetType::PlaytimeColumnAction => Some("Column"),
                ReaperTargetType::PlaytimeRowAction => Some("Row"),
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotSeek
                | ReaperTargetType::PlaytimeSlotVolume => Some("Slot"),
                _ if self.target.supports_automation_mode() => Some("Mode"),
                _ if self.mapping.target_model.supports_axis() => Some("Axis"),
                t if t.supports_fx() => Some("FX"),
                t if t.supports_seek_behavior() => Some("Behavior"),
                t if t.supports_send() => Some("Kind"),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_3_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_5_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc if self.target.supports_osc_arg_value_range() => {
                    Some("Range")
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_5_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_4_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Mouse if self.mapping.target_model.supports_mouse_button() => {
                    Some("Button")
                }
                ReaperTargetType::Action => Some("Action"),
                ReaperTargetType::LoadFxSnapshot => Some("Snapshot"),
                ReaperTargetType::SendOsc => Some("Argument"),
                ReaperTargetType::TrackTool | ReaperTargetType::FxTool => Some("Act/Tags"),
                ReaperTargetType::ModifyMapping => Some("Mapping"),
                ReaperTargetType::SendMidi => Some("Pattern"),
                t if t.supports_fx_parameter() => Some("Parameter"),
                t if t.supports_track_exclusivity() => Some("Exclusive"),
                t if t.supports_fx_display_type() => Some("Display"),
                t if t.supports_tags() => Some("Tags"),
                t if t.supports_exclusivity() => Some("Exclusivity"),
                t if t.supports_send() => match self.target.route_type() {
                    TrackRouteType::Send => Some("Send"),
                    TrackRouteType::Receive => Some("Receive"),
                    TrackRouteType::HardwareOutput => Some("Output"),
                },
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_4_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_3_label_2(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendMidi
                    if self.mapping.target_model.send_midi_destination_type()
                        == SendMidiDestinationType::InputDevice =>
                {
                    let text = if let Some(dev_id) = self.target.midi_input_device() {
                        let dev = Reaper::get().midi_input_device_by_id(dev_id);
                        get_midi_input_device_list_label(dev)
                    } else {
                        SAME_AS_INPUT_DEV.to_string()
                    };
                    Some(text)
                }
                ReaperTargetType::ModifyMapping => match self.target.mapping_ref() {
                    MappingRefModel::OwnMapping { .. } => Some("<This>".to_string()),
                    MappingRefModel::ForeignMapping { session_id, .. } => {
                        if let Some(session) =
                            BackboneShell::get().find_unit_model_by_key(session_id)
                        {
                            Some(session.borrow().to_string())
                        } else {
                            Some("<Unit doesn't exist>".to_string())
                        }
                    }
                },
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_3_LABEL_2)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_4_label_2(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some(self.target.action_name_label().to_string()),
                ReaperTargetType::LoadFxSnapshot => {
                    let label = if let Some(snapshot) = self.target.fx_snapshot() {
                        snapshot.to_string()
                    } else {
                        "<Empty>".to_owned()
                    };
                    Some(label)
                }
                ReaperTargetType::ModifyMapping => {
                    const NONE: &str = "<None>";
                    const MAPPING_DOESNT_EXIST: &str = "<Mapping doesn't exist>";
                    let compartment = self.mapping.compartment();
                    let label = match self.target.mapping_ref() {
                        MappingRefModel::OwnMapping { mapping_id } => match mapping_id {
                            None => NONE.to_string(),
                            Some(id) => match self.session.find_mapping_by_id(compartment, *id) {
                                None => MAPPING_DOESNT_EXIST.to_string(),
                                Some(mapping) => mapping.borrow().effective_name(),
                            },
                        },
                        MappingRefModel::ForeignMapping {
                            session_id,
                            mapping_key,
                        } => match mapping_key {
                            None => NONE.to_string(),
                            Some(mapping_key) => {
                                if let Some(other_session) = BackboneShell::get()
                                    .find_session_by_id_ignoring_borrowed_ones(session_id)
                                {
                                    let other_session = other_session.borrow();
                                    match other_session
                                        .find_mapping_by_key(compartment, mapping_key)
                                    {
                                        None => MAPPING_DOESNT_EXIST.to_string(),
                                        Some(mapping) => mapping.borrow().effective_name(),
                                    }
                                } else {
                                    "-".to_string()
                                }
                            }
                        },
                    };
                    Some(label)
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_4_LABEL_2)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_3_combo_box_1(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_3_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::Action => {
                    combo.show();
                    combo.fill_combo_box_indexed(ActionScope::iter());
                    combo.select_combo_box_item_by_index(self.target.action_scope().into());
                }
                ReaperTargetType::PlaytimeColumnAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeColumnDescriptorKind::iter());
                    combo.select_combo_box_item_by_index(
                        self.target.playtime_column().kind().into(),
                    );
                }
                ReaperTargetType::PlaytimeRowAction => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeRowDescriptorKind::iter());
                    combo.select_combo_box_item_by_index(self.target.playtime_row().kind().into());
                }
                ReaperTargetType::PlaytimeSlotManagementAction
                | ReaperTargetType::PlaytimeSlotTransportAction
                | ReaperTargetType::PlaytimeSlotSeek
                | ReaperTargetType::PlaytimeSlotVolume => {
                    combo.show();
                    combo.fill_combo_box_indexed(PlaytimeSlotDescriptorKind::iter());
                    combo.select_combo_box_item_by_index(self.target.playtime_slot().kind().into());
                }
                t if t.supports_fx() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxType::iter());
                    combo.select_combo_box_item_by_index(self.target.fx_type().into());
                }
                t if t.supports_send() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackRouteType::iter());
                    combo.select_combo_box_item_by_index(self.target.route_type().into());
                }
                _ => combo.hide(),
            },
            TargetCategory::Virtual => {
                combo.hide();
            }
        }
    }

    fn invalidate_target_line_4_combo_box_1(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_4_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::SendOsc => {
                    invalidate_with_osc_arg_index(combo, self.target.osc_arg_index());
                }
                ReaperTargetType::BrowseGroup => {
                    combo.show();
                    combo.fill_combo_box_indexed(SimpleExclusivity::iter());
                    let simple_exclusivity: SimpleExclusivity = self.target.exclusivity().into();
                    combo.select_combo_box_item_by_index(simple_exclusivity.into());
                }
                ReaperTargetType::TrackTool => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackToolAction::iter());
                    let action: TrackToolAction = self.target.track_tool_action();
                    combo.select_combo_box_item_by_index(action.into());
                }
                ReaperTargetType::FxTool => {
                    combo.show();
                    combo.fill_combo_box_indexed(FxToolAction::iter());
                    let action: FxToolAction = self.target.fx_tool_action();
                    combo.select_combo_box_item_by_index(action.into());
                }
                t if t.supports_fx_parameter() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxParameterType::iter());
                    combo.select_combo_box_item_by_index(self.target.param_type().into());
                }
                t if t.supports_exclusivity() => {
                    combo.show();
                    combo.fill_combo_box_indexed(Exclusivity::iter());
                    combo.select_combo_box_item_by_index(self.target.exclusivity().into());
                }
                t if t.supports_send() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackRouteSelectorType::iter());
                    combo.select_combo_box_item_by_index(self.target.route_selector_type().into());
                }
                _ => combo.hide(),
            },
            TargetCategory::Virtual => {
                combo.hide();
            }
        }
    }

    fn invalidate_target_line_3_combo_box_2(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_3_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => {
                    if matches!(
                        self.target.fx_type(),
                        VirtualFxType::ById | VirtualFxType::ByIdOrIndex
                    ) {
                        combo.show();
                        if self.target.track_type().is_sticky() {
                            let context = self.session.extended_context();
                            if let Ok(track) = self
                                .target
                                .with_context(context, self.mapping.compartment())
                                .first_effective_track()
                            {
                                // Fill
                                let chain = if self.target.fx_is_input_fx() {
                                    track.input_fx_chain()
                                } else {
                                    track.normal_fx_chain()
                                };
                                combo.fill_combo_box_indexed(fx_combo_box_entries(&chain));
                                // Set
                                if let Some(VirtualFx::ChainFx { chain_fx, .. }) =
                                    self.target.virtual_fx()
                                {
                                    if let Some(fx) = chain_fx
                                        .resolve(&[chain], context, self.mapping.compartment())
                                        .ok()
                                        .and_then(|fxs| fxs.into_iter().next())
                                    {
                                        combo.select_combo_box_item_by_index(fx.index() as _);
                                    } else {
                                        combo.select_new_combo_box_item(get_optional_fx_label(
                                            &chain_fx, None,
                                        ));
                                    }
                                } else {
                                    combo.select_new_combo_box_item("<None>");
                                }
                            } else {
                                combo.select_only_combo_box_item("<Requires track>");
                            }
                        } else {
                            combo.select_only_combo_box_item(
                                "Use 'Particular' only if track is particular as well!",
                            );
                        }
                    } else {
                        combo.hide();
                    }
                }
                t if t.supports_seek_behavior() => {
                    combo.show();
                    combo.fill_combo_box_indexed(SeekBehavior::iter());
                    combo.select_combo_box_item_by_index(self.target.seek_behavior().into());
                }
                ReaperTargetType::Action => {
                    combo.show();
                    combo.fill_combo_box_indexed(ActionInvocationType::iter());
                    combo.select_combo_box_item_by_index(
                        self.target.action_invocation_type().into(),
                    );
                }
                ReaperTargetType::TrackSolo => {
                    combo.show();
                    combo.fill_combo_box_indexed(SoloBehavior::iter());
                    combo.select_combo_box_item_by_index(self.target.solo_behavior().into());
                }
                ReaperTargetType::TrackShow => {
                    combo.show();
                    combo.fill_combo_box_indexed(RealearnTrackArea::iter());
                    combo.select_combo_box_item_by_index(self.target.track_area().into());
                }
                ReaperTargetType::RouteTouchState => {
                    combo.show();
                    combo.fill_combo_box_indexed(TouchedRouteParameterType::iter());
                    combo.select_combo_box_item_by_index(
                        self.target.touched_route_parameter_type().into(),
                    );
                }
                ReaperTargetType::TrackMonitoringMode => {
                    combo.show();
                    combo.fill_combo_box_indexed(MonitoringMode::iter());
                    combo.select_combo_box_item_by_index(self.target.monitoring_mode().into());
                }
                _ if self.target.supports_automation_mode() => {
                    combo.show();
                    combo.fill_combo_box_indexed(RealearnAutomationMode::iter());
                    combo.select_combo_box_item_by_index(self.target.automation_mode().into());
                }
                ReaperTargetType::TrackTouchState => {
                    combo.show();
                    combo.fill_combo_box_indexed(TouchedTrackParameterType::iter());
                    combo.select_combo_box_item_by_index(
                        self.target.touched_track_parameter_type().into(),
                    );
                }
                _ if self.mapping.target_model.supports_axis() => {
                    combo.show();
                    combo.fill_combo_box_indexed(Axis::iter());
                    combo.select_combo_box_item_by_index(self.target.axis().into());
                }
                _ => {
                    combo.hide();
                }
            },
            TargetCategory::Virtual => {
                combo.hide();
            }
        }
    }

    fn invalidate_target_line_4_combo_box_2(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_4_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc if self.target.osc_arg_index().is_some() => {
                    let tag = self.target.osc_arg_type_tag();
                    invalidate_with_osc_arg_type_tag(combo, tag);
                }
                ReaperTargetType::Mouse if self.mapping.target_model.supports_mouse_button() => {
                    combo.show();
                    combo.fill_combo_box_indexed(MouseButton::iter());
                    combo.select_combo_box_item_by_index(self.target.mouse_button().into());
                }
                t if t.supports_fx_parameter()
                    && self.target.param_type() == VirtualFxParameterType::ById =>
                {
                    combo.show();
                    let fx = get_relevant_target_fx(self.mapping, self.session);
                    if let Some(fx) = fx {
                        combo.fill_combo_box_indexed(fx_parameter_combo_box_entries(&fx));
                        let param_index = self.target.param_index();
                        combo
                            .select_combo_box_item_by_index_checked(param_index as _)
                            .unwrap_or_else(|_| {
                                let label = get_fx_param_label(None, param_index);
                                combo.select_new_combo_box_item(label.into_owned());
                            });
                    } else {
                        combo.select_only_combo_box_item("<Requires FX>");
                    }
                }
                t if t.supports_track_exclusivity() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackExclusivity::iter());
                    combo.select_combo_box_item_by_index(self.target.track_exclusivity().into());
                }
                t if t.supports_fx_display_type() => {
                    combo.show();
                    combo.fill_combo_box_indexed(FxDisplayType::iter());
                    combo.select_combo_box_item_by_index(self.target.fx_display_type().into());
                }
                t if t.supports_send() => {
                    if self.target.route_selector_type() == TrackRouteSelectorType::ById {
                        combo.show();
                        let context = self.session.extended_context();
                        let target_with_context = self
                            .target
                            .with_context(context, self.mapping.compartment());
                        if let Ok(track) = target_with_context.first_effective_track() {
                            // Fill
                            let route_type = self.target.route_type();
                            combo.fill_combo_box_indexed_vec(send_combo_box_entries(
                                &track, route_type,
                            ));
                            // Set
                            if route_type == TrackRouteType::HardwareOutput {
                                // Hardware output uses indexes, not IDs.
                                let i = self.target.route_index();
                                combo
                                    .select_combo_box_item_by_index_checked(i as _)
                                    .unwrap_or_else(|_| {
                                        let pity_label = format!("{}. <Not present>", i + 1);
                                        combo.select_new_combo_box_item(pity_label);
                                    });
                            } else {
                                // This is the real case. We use IDs.
                                if let Ok(virtual_route) = self.target.virtual_track_route() {
                                    if let Ok(route) = virtual_route.resolve(
                                        &track,
                                        context,
                                        self.mapping.compartment(),
                                    ) {
                                        let i = route.track_route_index().unwrap();
                                        combo.select_combo_box_item_by_index(i as _);
                                    } else {
                                        combo.select_new_combo_box_item(
                                            get_non_present_virtual_route_label(&virtual_route),
                                        );
                                    }
                                } else {
                                    combo.select_new_combo_box_item("<None>");
                                }
                            }
                        } else {
                            combo.select_only_combo_box_item("<Requires track>");
                        }
                    } else {
                        combo.hide();
                    }
                }
                _ => {
                    combo.hide();
                }
            },
            TargetCategory::Virtual => {
                combo.hide();
            }
        }
    }

    fn invalidate_target_check_box_1(&self) {
        let state = match self.target.category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::Action => Some(("With track", self.target.with_track())),
                ReaperTargetType::GoToBookmark => {
                    let is_regions = self.target.bookmark_type() == BookmarkType::Region;
                    Some(("Regions", is_regions))
                }
                ReaperTargetType::Seek => Some(("Seek play", self.target.seek_play())),
                ReaperTargetType::PlaytimeSlotTransportAction => {
                    Some(("Stop if empty", self.target.stop_column_if_slot_empty()))
                }
                _ if self.target.supports_fx_chain() => {
                    let is_input_fx = self.target.fx_is_input_fx();
                    let label = if self.target.track_type() == VirtualTrackType::Master {
                        "Monitoring FX"
                    } else {
                        "Input FX"
                    };
                    Some((label, is_input_fx))
                }
                t if t.supports_track_scrolling() => {
                    Some(("Scroll TCP", self.target.scroll_arrange_view()))
                }
                _ => None,
            },
            TargetCategory::Virtual => Some(("Learnable", self.target.learnable())),
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_1, state);
    }

    fn invalidate_target_check_box_2(&self) {
        let state = match self.target.category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::LoadMappingSnapshot => {
                    Some(("Active mappings only", self.target.active_mappings_only()))
                }
                _ if self.mapping.target_model.supports_track_must_be_selected() => {
                    if self
                        .target
                        .track_type()
                        .track_selected_condition_makes_sense()
                    {
                        Some((
                            "Track must be selected",
                            self.target.enable_only_if_track_selected(),
                        ))
                    } else {
                        None
                    }
                }
                t if t.supports_track_scrolling() => {
                    Some(("Scroll MCP", self.target.scroll_mixer()))
                }
                ReaperTargetType::Seek => Some(("Move view", self.target.move_view())),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_2, state);
    }

    fn invalidate_target_check_box_3(&self) {
        let state = match self.target.category() {
            TargetCategory::Reaper => match self.target.target_type() {
                t if t.supports_fx() => {
                    if self.target.fx_type() == VirtualFxType::Focused {
                        None
                    } else {
                        Some((
                            "FX must have focus",
                            self.target.enable_only_if_fx_has_focus(),
                        ))
                    }
                }
                ReaperTargetType::Seek => Some(("Use project", self.target.use_project())),
                _ if self.target.supports_gang_selected() => Some((
                    "Selection ganging",
                    self.target.fixed_gang_behavior().use_selection_ganging(),
                )),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_3, state);
    }

    fn invalidate_target_check_box_4(&self) {
        let state = match self.target.category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::Seek => Some(("Use regions", self.target.use_regions())),
                t if t.supports_poll_for_feedback() => {
                    Some(("Poll for feedback", self.target.poll_for_feedback()))
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_4, state);
    }

    fn invalidate_target_check_box_5(&self) {
        let checkbox_id = root::ID_TARGET_CHECK_BOX_5;
        let state = match self.target.category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::FxParameterValue => Some(("Retrigger", self.target.retrigger())),
                ReaperTargetType::Seek => Some(("Use loop points", self.target.use_loop_points())),
                ReaperTargetType::GoToBookmark => {
                    Some(("Set loop points", self.target.use_loop_points()))
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(checkbox_id, state);
    }

    fn invalidate_target_check_box_6(&self) {
        let state = match self.target.category() {
            TargetCategory::Reaper => match self.target.target_type() {
                ReaperTargetType::FxParameterValue => Some(("Real-time", self.target.real_time())),
                ReaperTargetType::Seek => {
                    Some(("Use time selection", self.target.use_time_selection()))
                }
                ReaperTargetType::GoToBookmark => {
                    Some(("Set time selection", self.target.use_time_selection()))
                }
                _ if self.target.supports_gang_grouping() => Some((
                    "Respect grouping",
                    self.target.fixed_gang_behavior().use_track_grouping(),
                )),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_6, state);
    }

    fn invalidate_check_box<'b>(
        &self,
        checkbox_id: u32,
        state: Option<(impl Into<SwellStringArg<'b>>, bool)>,
    ) {
        let b = self.view.require_control(checkbox_id);
        if let Some((label, is_checked)) = state {
            b.set_text(label);
            b.set_checked(is_checked);
            b.show();
        } else {
            b.hide();
        }
    }

    fn invalidate_target_value_controls(&self) {
        // TODO-low This might set the value slider to the wrong value because it only takes the
        //  first resolved target into account.
        let (error_msg, read_enabled, write_enabled, character, is_relative) =
            if let Some(t) = self.first_resolved_target() {
                let control_context = self.session.control_context();
                let (error_msg, read_enabled, write_enabled) = if t.is_virtual() {
                    // Makes no sense to display any value controls for virtual targets. They neither
                    // have a value nor would moving a slider make any difference.
                    (None, false, false)
                } else if t.can_report_current_value() {
                    let value = t.current_value(control_context).unwrap_or_default();
                    self.invalidate_target_value_controls_with_value(value);
                    (None, true, true)
                } else {
                    // Target is real but can't report values (e.g. load mapping snapshot)
                    (None, false, true)
                };
                let is_relative = t.control_type(control_context).is_relative();
                (
                    error_msg,
                    read_enabled,
                    write_enabled,
                    Some(t.character(control_context)),
                    is_relative,
                )
            } else {
                (Some("Target inactive!"), false, false, None, false)
            };
        self.enable_if(
            read_enabled,
            &[
                root::ID_TARGET_VALUE_LABEL_TEXT,
                root::ID_TARGET_VALUE_EDIT_CONTROL,
                root::ID_TARGET_UNIT_BUTTON,
            ],
        );
        // Slider or buttons
        let button_1 = self.view.require_control(root::ID_TARGET_VALUE_OFF_BUTTON);
        let button_2 = self.view.require_control(root::ID_TARGET_VALUE_ON_BUTTON);
        let slider_control = self
            .view
            .require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL);
        if write_enabled {
            if is_relative {
                slider_control.hide();
                button_1.show();
                button_2.show();
                button_1.set_text("-");
                button_2.set_text("+");
            } else {
                use TargetCharacter::*;
                match character {
                    Some(Trigger) => {
                        slider_control.hide();
                        button_1.hide();
                        button_2.show();
                        button_2.set_text("Trigger!");
                    }
                    Some(Switch) => {
                        slider_control.hide();
                        button_1.show();
                        button_2.show();
                        button_1.set_text("Off");
                        button_2.set_text("On");
                    }
                    _ => {
                        button_1.hide();
                        button_2.hide();
                        slider_control.show();
                    }
                }
            }
        } else {
            slider_control.hide();
            button_1.hide();
            button_2.hide();
        }
        // Maybe display grey error message instead of value text
        let value_text = self.view.require_control(root::ID_TARGET_VALUE_TEXT);
        if let Some(msg) = error_msg {
            value_text.show();
            value_text.disable();
            value_text.set_text(msg);
        } else if read_enabled {
            value_text.show();
            value_text.enable();
            // Value text already set above
        } else {
            value_text.hide();
        }
    }

    fn invalidate_target_value_controls_with_value(&self, value: AbsoluteValue) {
        self.invalidate_target_controls_internal(
            root::ID_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_TARGET_VALUE_EDIT_CONTROL,
            root::ID_TARGET_VALUE_TEXT,
            value,
            None,
            false,
        );
        self.invalidate_target_unit_button();
    }

    fn invalidate_target_unit_button(&self) {
        let unit = self.mapping.target_model.unit();
        let control_context = self.session.control_context();
        let (value_unit, step_size_unit) = match unit {
            TargetUnit::Native => self
                .first_resolved_target()
                .map(|t| {
                    let vu = t.value_unit(control_context);
                    let vu = if vu.is_empty() { None } else { Some(vu) };
                    let su = t.step_size_unit(control_context);
                    let su = if su.is_empty() { None } else { Some(su) };
                    (vu, su)
                })
                .unwrap_or((None, None)),
            TargetUnit::Percent => (Some("%"), Some("%")),
        };
        let text = format!(
            "{}. {} ({})",
            usize::from(unit) + 1,
            value_unit.unwrap_or("-"),
            step_size_unit.unwrap_or("-")
        );
        self.view
            .require_control(root::ID_TARGET_UNIT_BUTTON)
            .set_text(text);
    }

    fn invalidate_target_learn_button(&self) {
        self.invalidate_learn_button(
            self.session
                .unit()
                .borrow()
                .mapping_is_learning_target(self.mapping.qualified_id()),
            root::ID_TARGET_LEARN_BUTTON,
        );
    }

    fn register_listeners(&self) {
        self.register_session_listeners();
        self.register_help_listeners();
    }

    fn register_help_listeners(&self) {
        self.panel.when(
            self.panel.active_help_topic.borrow().changed(),
            |view, _| {
                view.invalidate_help();
            },
        );
    }

    fn register_session_listeners(&self) {
        self.panel.when(
            self.session.unit().borrow().on_mappings_changed(),
            |view, _| {
                view.invalidate_window_title();
            },
        );
        self.panel.when(
            self.session
                .unit()
                .borrow()
                .mapping_which_learns_source()
                .changed(),
            |view, _| {
                view.invalidate_source_learn_button();
            },
        );
        self.panel.when(
            self.session
                .unit()
                .borrow()
                .mapping_which_learns_target()
                .changed(),
            |view, _| {
                view.invalidate_target_learn_button();
            },
        );
    }

    fn invalidate_mode_controls(&self) {
        self.invalidate_mode_controls_internal(None);
    }

    fn invalidate_mode_controls_internal(&self, initiator: Option<u32>) {
        self.fill_mode_type_combo_box();
        self.invalidate_mode_type_combo_box();
        self.invalidate_mode_control_appearance();
        self.invalidate_mode_source_value_controls(initiator);
        self.invalidate_mode_target_value_controls(initiator);
        self.invalidate_mode_step_controls(initiator);
        self.invalidate_mode_fire_controls(initiator);
        self.invalidate_mode_rotate_check_box();
        self.invalidate_mode_make_absolute_check_box();
        self.invalidate_mode_feedback_type_combo_box();
        self.invalidate_mode_feedback_type_button();
        self.invalidate_mode_out_of_range_behavior_combo_box();
        self.invalidate_mode_group_interaction_combo_box();
        self.invalidate_mode_round_target_value_check_box();
        self.invalidate_mode_takeover_mode_combo_box();
        self.invalidate_mode_button_usage_combo_box();
        self.invalidate_mode_encoder_usage_combo_box();
        self.invalidate_mode_reverse_check_box();
        self.invalidate_mode_target_value_sequence_edit_control(initiator);
        self.invalidate_mode_eel_control_transformation_edit_control(initiator);
        self.invalidate_mode_eel_feedback_transformation_edit_control(initiator);
    }

    fn invalidate_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.absolute_mode().into());
    }

    fn invalidate_mode_control_appearance(&self) {
        self.invalidate_mode_control_labels();
        self.invalidate_mode_control_visibilities();
    }

    fn mapping_uses_step_factors(&self) -> bool {
        self.mapping
            .with_context(self.session.extended_context())
            .uses_step_factors()
    }

    fn invalidate_mode_control_labels(&self) {
        let step_label = if self.mapping_uses_step_factors() {
            "Speed"
        } else {
            "Step size"
        };
        self.view
            .require_control(root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT)
            .set_text(step_label);
    }

    #[allow(clippy::needless_bool)]
    fn invalidate_mode_control_visibilities(&self) {
        let relevant_source_characters = self.mapping.source_model.possible_detailed_characters();
        let base_input = self.mapping.base_mode_applicability_check_input();
        let is_relevant = |mode_parameter: ModeParameter| {
            self.mapping.mode_parameter_is_relevant(
                mode_parameter,
                base_input,
                &relevant_source_characters,
            )
        };
        let real_target = self.first_resolved_target();
        let target_can_report_current_value = real_target
            .as_ref()
            .map(|t| t.can_report_current_value())
            .unwrap_or_default();
        let target_wants_relative_control = real_target
            .as_ref()
            .map(|t| t.control_type(self.session.control_context()).is_relative())
            .unwrap_or_default();
        let feedback_is_on = self.mapping.feedback_is_enabled_and_supported();
        // For all source characters
        {
            let show_source_min_max = is_relevant(ModeParameter::SourceMinMax);
            self.enable_if(show_source_min_max, SOURCE_MIN_MAX_ELEMENTS);
            let show_reverse = is_relevant(ModeParameter::Reverse);
            self.enable_if(show_reverse, REVERSE_ELEMENTS);
            let show_out_of_range_behavior = is_relevant(ModeParameter::OutOfRangeBehavior);
            self.enable_if(show_out_of_range_behavior, OUT_OF_RANGE_ELEMENTS);
            let show_group_interaction = is_relevant(ModeParameter::GroupInteraction);
            self.enable_if(show_group_interaction, GROUP_INTERACTION_ELEMENTS);
            let target_controls_make_sense = if target_wants_relative_control
                && (!feedback_is_on || !target_can_report_current_value)
            {
                false
            } else {
                true
            };
            let show_target_value_sequence = target_controls_make_sense
                && is_relevant(ModeParameter::TargetValueSequence)
                && real_target.is_some();
            self.enable_if(show_target_value_sequence, VALUE_SEQUENCE_ELEMENTS);
            let show_target_min_max = target_controls_make_sense
                && is_relevant(ModeParameter::TargetMinMax)
                && real_target.is_some();
            self.enable_if(show_target_min_max, TARGET_MIN_MAX_ELEMENTS);
            let show_feedback_transformation = is_relevant(ModeParameter::FeedbackTransformation)
                || is_relevant(ModeParameter::TextualFeedbackExpression);
            self.enable_if(
                show_feedback_transformation,
                FEEDBACK_TRANSFORMATION_ELEMENTS,
            );
            let show_feedback_type = is_relevant(ModeParameter::FeedbackType);
            self.enable_if(show_feedback_type, FEEDBACK_TYPE_ELEMENTS);
        }
        // For knobs/faders and buttons
        {
            let show_round_controls = is_relevant(ModeParameter::RoundTargetValue)
                && self.target_with_context().is_known_to_be_roundable();
            self.enable_if(show_round_controls, ROUND_TARGET_VALUE_ELEMENTS);
            let show_takeover =
                target_can_report_current_value && is_relevant(ModeParameter::TakeoverMode);
            self.enable_if(show_takeover, TAKEOVER_MODE_ELEMENTS);
            let show_control_transformation = is_relevant(ModeParameter::ControlTransformation);
            self.enable_if(show_control_transformation, CONTROL_TRANSFORMATION_ELEMENTS);
            let show_absolute_mode = is_relevant(ModeParameter::AbsoluteMode);
            self.enable_if(show_absolute_mode, ABSOLUTE_MODE_ELEMENTS);
            self.enable_if(
                show_round_controls
                    || show_takeover
                    || show_control_transformation
                    || show_absolute_mode,
                &[root::ID_MODE_KNOB_FADER_GROUP_BOX],
            );
        }
        // For encoders and incremental buttons
        {
            let step_min_is_relevant = real_target.is_some()
                && (is_relevant(ModeParameter::StepSizeMin)
                    || is_relevant(ModeParameter::StepFactorMin));
            let step_max_is_relevant = real_target.is_some()
                && (is_relevant(ModeParameter::StepSizeMax)
                    || is_relevant(ModeParameter::StepFactorMax));
            self.enable_if(
                step_min_is_relevant || step_max_is_relevant,
                &[root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT],
            );
            self.enable_if(step_min_is_relevant, STEP_MIN_ELEMENTS);
            self.enable_if(step_max_is_relevant, STEP_MAX_ELEMENTS);
            let show_rotate = is_relevant(ModeParameter::Rotate);
            self.enable_if(show_rotate, ROTATE_ELEMENTS);
            let show_make_absolute = is_relevant(ModeParameter::MakeAbsolute);
            self.enable_if(show_make_absolute, MAKE_ABSOLUTE_ELEMENTS);
            let show_relative_filter = is_relevant(ModeParameter::RelativeFilter);
            self.enable_if(show_relative_filter, RELATIVE_FILTER_ELEMENTS);
            self.enable_if(
                step_min_is_relevant
                    || step_max_is_relevant
                    || show_rotate
                    || show_make_absolute
                    || show_relative_filter,
                &[root::ID_MODE_RELATIVE_GROUP_BOX],
            );
        }
        // For buttons
        {
            let show_button_filter = is_relevant(ModeParameter::ButtonFilter);
            self.enable_if(show_button_filter, BUTTON_FILTER_ELEMENTS);
            let show_fire_mode = is_relevant(ModeParameter::FireMode);
            self.enable_if(show_fire_mode, FIRE_MODE_ELEMENTS);
            self.enable_if(
                show_button_filter || show_fire_mode,
                &[root::ID_MODE_BUTTON_GROUP_BOX],
            );
        }
    }

    fn invalidate_mode_source_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_min_source_value_controls(initiator);
        self.invalidate_mode_max_source_value_controls(initiator);
    }

    fn invalidate_mode_target_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_min_target_value_controls(initiator);
        self.invalidate_mode_max_target_value_controls(initiator);
    }

    fn invalidate_mode_min_source_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval().min_val(),
            initiator,
        );
    }

    fn invalidate_mode_max_source_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval().max_val(),
            initiator,
        );
    }

    fn invalidate_mode_source_value_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value: UnitValue,
        initiator: Option<u32>,
    ) {
        let formatted_value = self
            .source
            .format_control_value(ControlValue::AbsoluteContinuous(value))
            .unwrap_or_else(|_| "".to_string());
        if initiator != Some(edit_control_id) {
            self.view
                .require_control(edit_control_id)
                .set_text(formatted_value);
        }
        self.view
            .require_control(slider_control_id)
            .set_slider_unit_value(value);
    }

    fn invalidate_mode_min_target_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.target_value_interval().min_val()),
            initiator,
            false,
        );
    }

    fn invalidate_mode_max_target_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.target_value_interval().max_val()),
            initiator,
            false,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn invalidate_target_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: AbsoluteValue,
        initiator: Option<u32>,
        use_step_sizes: bool,
    ) {
        invalidate_target_controls_free(
            // It's okay to use the first resolved target only because we use it solely to gather
            // some target characteristics, no the value.
            self.first_resolved_target().as_ref(),
            self.view.require_control(slider_control_id),
            self.view.require_control(edit_control_id),
            self.view.require_control(value_text_control_id),
            value,
            initiator,
            edit_control_id,
            false,
            use_step_sizes,
            self.target.unit(),
            self.session.control_context(),
        );
    }

    fn invalidate_mode_step_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_min_step_controls(initiator);
        self.invalidate_mode_max_step_controls(initiator);
    }

    fn invalidate_mode_fire_controls(&self, initiator: Option<u32>) {
        let base_input = self.mapping.base_mode_applicability_check_input();
        let possible_source_characters = self.mapping.source_model.possible_detailed_characters();
        if self.mapping.mode_parameter_is_relevant(
            ModeParameter::FireMode,
            base_input,
            &possible_source_characters,
        ) {
            self.invalidate_mode_fire_mode_combo_box();
            self.invalidate_mode_fire_line_2_controls(initiator);
            self.invalidate_mode_fire_line_3_controls(initiator);
        }
    }

    fn invalidate_mode_min_step_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_step_controls_internal(
            root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
            self.mode.step_size_interval().min_val(),
            self.mode.step_factor_interval().min_val(),
            initiator,
        );
    }

    fn invalidate_mode_fire_line_2_controls(&self, initiator: Option<u32>) {
        let label = match self.mapping.mode_model.fire_mode() {
            FireMode::Normal => Some("Min"),
            FireMode::AfterTimeout | FireMode::AfterTimeoutKeepFiring => Some("Timeout"),
            FireMode::OnDoublePress | FireMode::OnSinglePress => None,
        };
        self.view
            .require_control(root::ID_MODE_FIRE_LINE_2_LABEL_1)
            .set_text_or_hide(label);
        if label.is_some() {
            self.invalidate_mode_fire_controls_internal(
                root::ID_MODE_FIRE_LINE_2_SLIDER_CONTROL,
                root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL,
                root::ID_MODE_FIRE_LINE_2_LABEL_2,
                self.mode.press_duration_interval().min_val(),
                initiator,
            );
        }
        self.enable_if(
            label.is_some(),
            &[
                root::ID_MODE_FIRE_LINE_2_SLIDER_CONTROL,
                root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL,
                root::ID_MODE_FIRE_LINE_2_LABEL_1,
                root::ID_MODE_FIRE_LINE_2_LABEL_2,
            ],
        );
    }

    fn invalidate_mode_max_step_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_step_controls_internal(
            root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
            self.mode.step_size_interval().max_val(),
            self.mode.step_factor_interval().max_val(),
            initiator,
        );
    }

    fn invalidate_mode_fire_line_3_controls(&self, initiator: Option<u32>) {
        let option = match self.mapping.mode_model.fire_mode() {
            FireMode::Normal | FireMode::OnSinglePress => {
                Some(("Max", self.mode.press_duration_interval().max_val()))
            }
            FireMode::AfterTimeout | FireMode::OnDoublePress => None,
            FireMode::AfterTimeoutKeepFiring => Some(("Rate", self.mode.turbo_rate())),
        };
        if let Some((label, value)) = option {
            self.view
                .require_control(root::ID_MODE_FIRE_LINE_3_LABEL_1)
                .set_text(label);
            self.invalidate_mode_fire_controls_internal(
                root::ID_MODE_FIRE_LINE_3_SLIDER_CONTROL,
                root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL,
                root::ID_MODE_FIRE_LINE_3_LABEL_2,
                value,
                initiator,
            );
        }
        self.enable_if(
            option.is_some(),
            &[
                root::ID_MODE_FIRE_LINE_3_SLIDER_CONTROL,
                root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL,
                root::ID_MODE_FIRE_LINE_3_LABEL_1,
                root::ID_MODE_FIRE_LINE_3_LABEL_2,
            ],
        );
    }

    fn invalidate_mode_step_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        size_value: UnitValue,
        factor_value: DiscreteIncrement,
        initiator: Option<u32>,
    ) {
        enum Val {
            Abs(UnitValue),
            Rel(DiscreteIncrement),
        }
        let (val, edit_text, value_text) = match &self.first_resolved_target() {
            Some(target) => {
                if self.mapping_uses_step_factors() {
                    let edit_text = factor_value.to_string();
                    let val = Val::Rel(factor_value);
                    // "count {x}"
                    (val, edit_text, "x".to_string())
                } else {
                    // "{size} {unit}"
                    let control_context = self.session.control_context();
                    let edit_text =
                        target.format_step_size_without_unit(size_value, control_context);
                    let value_text = get_text_right_to_step_size_edit_control(
                        target,
                        size_value,
                        control_context,
                    );
                    (Val::Abs(size_value), edit_text, value_text)
                }
            }
            None => (Val::Abs(UnitValue::MIN), "".to_string(), "".to_string()),
        };
        match val {
            Val::Abs(v) => {
                self.view
                    .require_control(slider_control_id)
                    .set_slider_unit_value(v);
            }
            Val::Rel(v) => {
                self.view
                    .require_control(slider_control_id)
                    .set_slider_discrete_increment(v);
            }
        }
        if initiator != Some(edit_control_id) {
            self.view
                .require_control(edit_control_id)
                .set_text(edit_text);
        }
        self.view
            .require_control(value_text_control_id)
            .set_text(value_text)
    }

    fn invalidate_mode_fire_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        duration: Duration,
        initiator: Option<u32>,
    ) {
        self.view
            .require_control(slider_control_id)
            .set_slider_duration(duration);
        if initiator != Some(edit_control_id) {
            self.view
                .require_control(edit_control_id)
                .set_text(duration.as_millis().to_string());
        }
        self.view
            .require_control(value_text_control_id)
            .set_text("ms")
    }

    fn invalidate_mode_rotate_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
            .set_checked(self.mode.rotate());
    }

    fn invalidate_mode_feedback_type_button(&self) {
        let text = if self.mode.feedback_color().is_some()
            || self.mode.feedback_background_color().is_some()
        {
            "...*"
        } else {
            "..."
        };
        self.view
            .require_control(root::IDC_MODE_FEEDBACK_TYPE_BUTTON)
            .set_text(text);
    }

    fn invalidate_mode_feedback_type_combo_box(&self) {
        self.view
            .require_control(root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.feedback_type().into());
    }

    fn invalidate_mode_make_absolute_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX)
            .set_checked(self.mode.make_absolute());
    }

    fn invalidate_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .select_combo_box_item_by_index(self.mode.out_of_range_behavior().into());
    }

    fn invalidate_mode_group_interaction_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.group_interaction().into());
    }

    fn invalidate_mode_fire_mode_combo_box(&self) {
        let combo = self.view.require_control(root::ID_MODE_FIRE_COMBO_BOX);
        combo.set_enabled(self.target_category() != TargetCategory::Virtual);
        combo.select_combo_box_item_by_index(self.mapping.mode_model.fire_mode().into());
    }

    fn invalidate_mode_round_target_value_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
            .set_checked(self.mode.round_target_value());
    }

    fn invalidate_mode_takeover_mode_combo_box(&self) {
        let mode = self.mode.takeover_mode();
        self.view
            .require_control(root::ID_MODE_TAKEOVER_MODE)
            .select_combo_box_item_by_index(mode.into());
    }

    fn invalidate_mode_button_usage_combo_box(&self) {
        let usage = self.mode.button_usage();
        self.view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .select_combo_box_item_by_index(usage.into());
    }

    fn invalidate_mode_encoder_usage_combo_box(&self) {
        let usage = self.mode.encoder_usage();
        self.view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .select_combo_box_item_by_index(usage.into());
    }

    fn invalidate_mode_reverse_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
            .set_checked(self.mode.reverse());
    }

    fn invalidate_mode_target_value_sequence_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL) {
            return;
        }
        let sequence = self.mode.target_value_sequence();
        let formatted = match self.target.unit() {
            TargetUnit::Native => {
                if let Some(t) = self.first_resolved_target() {
                    let t = WithControlContext::new(self.session.control_context(), &t);
                    let displayable = sequence.displayable(&t);
                    displayable.to_string()
                } else {
                    sequence.displayable(&PercentIo).to_string()
                }
            }
            TargetUnit::Percent => sequence.displayable(&PercentIo).to_string(),
        };
        self.view
            .require_control(root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL)
            .set_text(formatted);
    }

    fn invalidate_mode_eel_control_transformation_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL) {
            return;
        }
        let control = self
            .view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL);
        let text = self.mode.eel_control_transformation();
        control.set_text(extract_first_line(text));
        control.set_enabled(!has_multiple_lines(text));
    }

    fn invalidate_mode_eel_feedback_transformation_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL) {
            return;
        }
        let control = self
            .view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL);
        let text = if self.mode.feedback_type().is_textual() {
            self.mode.textual_feedback_expression()
        } else {
            self.mode.eel_feedback_transformation()
        };
        control.set_text(extract_first_line(text));
        control.set_enabled(!has_multiple_lines(text));
    }

    fn fill_source_category_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX);
        let is_main_mapping = self.mapping.compartment() == CompartmentKind::Main;
        b.fill_combo_box_small(
            SourceCategory::iter()
                // Don't allow controller mappings to have virtual source
                .filter(|c| is_main_mapping || *c != SourceCategory::Virtual),
        );
    }

    fn fill_mapping_feedback_send_behavior_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX);
        b.fill_combo_box_indexed(FeedbackSendBehavior::iter());
    }

    fn fill_target_category_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX);
        b.fill_combo_box_indexed(TargetCategory::iter());
    }

    fn fill_source_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => b.fill_combo_box_indexed(MidiSourceType::iter()),
            Reaper => b.fill_combo_box_indexed(ReaperSourceType::iter()),
            Virtual => b.fill_combo_box_indexed(VirtualControlElementCharacter::iter()),
            Osc | Never | Keyboard | StreamDeck => {}
        };
    }

    fn fill_mode_type_combo_box(&self) {
        let base_input = self.mapping.base_mode_applicability_check_input();
        let relevant_source_characters = self.mapping.source_model.possible_detailed_characters();
        let items = AbsoluteMode::iter().map(|m| {
            let applicable = self.mapping.mode_model.mode_parameter_is_relevant(
                ModeParameter::SpecificAbsoluteMode(m),
                base_input,
                &relevant_source_characters,
                true,
                false,
            );
            let suffix = if applicable { "" } else { " (NOT APPLICABLE)" };
            format!("{m}{suffix}")
        });
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .fill_combo_box_indexed(items);
    }

    fn fill_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .fill_combo_box_indexed(OutOfRangeBehavior::iter());
    }

    fn fill_mode_group_interaction_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .fill_combo_box_indexed(GroupInteraction::iter());
    }

    fn fill_mode_fire_mode_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_FIRE_COMBO_BOX)
            .fill_combo_box_indexed(FireMode::iter());
    }

    fn fill_mode_feedback_type_combo_box(&self) {
        self.view
            .require_control(root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX)
            .fill_combo_box_indexed(FeedbackType::iter());
    }

    fn fill_mode_takeover_mode_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_TAKEOVER_MODE)
            .fill_combo_box_indexed(TakeoverMode::iter());
    }

    fn fill_mode_button_usage_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .fill_combo_box_indexed(ButtonUsage::iter());
    }

    fn fill_mode_encoder_usage_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .fill_combo_box_indexed(EncoderUsage::iter());
    }

    fn resolved_targets(&self) -> Vec<CompoundMappingTarget> {
        self.target_with_context().resolve().unwrap_or_default()
    }

    fn first_resolved_target(&self) -> Option<CompoundMappingTarget> {
        self.resolved_targets().into_iter().next()
    }
}

impl View for MappingPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        self.init_controls();
        self.mapping_header_panel.clone().open(window);
        if cfg!(unix) && BackboneShell::get().config().background_colors_enabled() {
            self.mapping_color_panel.clone().open(window);
            self.source_color_panel.clone().open(window);
            self.target_color_panel.clone().open(window);
            self.glue_color_panel.clone().open(window);
            self.help_color_panel.clone().open(window);
        }
        self.ui_element_container
            .borrow_mut()
            .fill_with_window_children(window);
        let dyn_self: Rc<dyn View> = self.clone();
        self.mapping_header_panel
            .set_parent_view(Rc::downgrade(&dyn_self));
        true
    }

    fn erase_background(self: SharedView<Self>, device_context: DeviceContext) -> bool {
        if cfg!(unix) {
            // On macOS/Linux we use color panels as real child windows.
            return false;
        }
        if !BackboneShell::get().config().background_colors_enabled() {
            return false;
        }
        let window = self.view.require_window();
        self.mapping_color_panel
            .paint_manually(device_context, window);
        self.source_color_panel
            .paint_manually(device_context, window);
        self.target_color_panel
            .paint_manually(device_context, window);
        self.glue_color_panel.paint_manually(device_context, window);
        self.help_color_panel.paint_manually(device_context, window);
        true
    }

    fn control_color_static(
        self: SharedView<Self>,
        device_context: DeviceContext,
        window: Window,
    ) -> Option<Hbrush> {
        if cfg!(target_os = "macos") {
            // On macOS, we fortunately don't need to do this nonsense. And it wouldn't be possible
            // anyway because SWELL macOS can't distinguish between different child controls.
            return None;
        }
        if !BackboneShell::get().config().background_colors_enabled() {
            return None;
        }
        device_context.set_bk_mode_to_transparent();
        let section = Section::from_resource_id(window.resource_id())?;
        view::get_brush_for_color_pair(section.color_pair())
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        self.hide();
        true
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        self.window_cache.replace(None);
        self.ui_element_container.replace(Default::default());
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Mapping
            root::IDC_BEEP_ON_SUCCESS_CHECK_BOX => {
                self.write(|p| p.update_beep_on_success());
            }
            root::IDC_MAPPING_ENABLED_CHECK_BOX => {
                self.write(|p| p.update_mapping_is_enabled());
            }
            root::ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX => {
                self.write(|p| p.update_mapping_is_visible_in_projection());
            }
            root::ID_MAPPING_ADVANCED_BUTTON => {
                self.edit_advanced_settings();
            }
            root::ID_MAPPING_FIND_IN_LIST_BUTTON => {
                self.force_scroll_to_mapping_in_main_panel();
            }
            // IDCANCEL is escape button
            root::ID_MAPPING_PANEL_OK | raw::IDCANCEL => {
                self.hide();
            }
            // When pressing ENTER in text fields
            raw::IDOK => {
                // Ideally we would do the same here as the tab key. But since SWELL doesn't support
                // GetNextDlgTabItem, doing this cross-platform would require a bit more effort. Whatever.
            }
            root::ID_MAPPING_PANEL_PREVIOUS_BUTTON => {
                let _ = self.navigate_in_mappings(-1);
            }
            root::ID_MAPPING_PANEL_NEXT_BUTTON => {
                let _ = self.navigate_in_mappings(1);
            }
            // Source
            root::ID_SOURCE_LEARN_BUTTON => self.toggle_learn_source(),
            root::ID_SOURCE_RPN_CHECK_BOX => {
                self.write(|p| p.handle_source_line_4_check_box_change())
            }
            root::ID_SOURCE_14_BIT_CHECK_BOX => {
                self.write(|p| p.handle_source_check_box_2_change())
            }
            root::ID_SOURCE_LINE_4_BUTTON => {
                let _ = self.handle_source_line_4_button_press();
            }
            root::ID_SOURCE_LINE_5_BUTTON => {
                let _ = self.handle_source_line_5_button_press();
            }
            root::ID_MODE_EEL_CONTROL_TRANSFORMATION_DETAIL_BUTTON => {
                self.edit_control_transformation()
            }
            root::ID_SOURCE_SCRIPT_DETAIL_BUTTON => self.handle_source_line_7_button_press(),
            // Mode
            root::ID_SETTINGS_ROTATE_CHECK_BOX => self.write(|p| p.update_mode_rotate()),
            root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX => {
                self.write(|p| p.update_mode_make_absolute())
            }
            root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX => {
                self.write(|p| p.update_mode_round_target_value())
            }
            root::ID_SETTINGS_REVERSE_CHECK_BOX => self.write(|p| p.update_mode_reverse()),
            root::ID_SETTINGS_RESET_BUTTON => self.write(|p| p.reset_mode()),
            root::IDC_MODE_FEEDBACK_TYPE_BUTTON => {
                let _ = self.feedback_type_button_pressed();
            }
            // Target
            root::ID_TARGET_TYPE_BUTTON => self.show_target_type_menu(),
            root::ID_TARGET_CHECK_BOX_1 => self.write(|p| p.handle_target_check_box_1_change()),
            root::ID_TARGET_CHECK_BOX_2 => self.write(|p| p.handle_target_check_box_2_change()),
            root::ID_TARGET_CHECK_BOX_3 => self.write(|p| p.handle_target_check_box_3_change()),
            root::ID_TARGET_CHECK_BOX_4 => self.write(|p| p.handle_target_check_box_4_change()),
            root::ID_TARGET_CHECK_BOX_5 => self.write(|p| p.handle_target_check_box_5_change()),
            root::ID_TARGET_CHECK_BOX_6 => self.write(|p| p.handle_target_check_box_6_change()),
            root::ID_TARGET_LEARN_BUTTON => self.toggle_learn_target(),
            root::ID_TARGET_MENU_BUTTON => self.open_target_menu(),
            root::ID_TARGET_LINE_2_BUTTON => {
                let _ = self.handle_target_line_2_button_press();
            }
            root::ID_TARGET_LINE_3_BUTTON => {
                let _ = self.handle_target_line_3_button_press();
            }
            root::ID_TARGET_LINE_4_BUTTON => {
                self.notify_user_on_anyhow_error(self.handle_target_line_4_button_press());
            }
            root::ID_TARGET_VALUE_OFF_BUTTON => {
                let _ = self.read(|p| p.hit_target_special(false));
            }
            root::ID_TARGET_VALUE_ON_BUTTON => {
                let _ = self.read(|p| p.hit_target_special(true));
            }
            root::ID_TARGET_UNIT_BUTTON => self.write(|p| p.handle_target_unit_button_press()),
            _ => {}
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Mapping
            root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX => {
                self.write(|p| p.update_mapping_feedback_send_behavior())
            }
            // Source
            root::ID_SOURCE_CATEGORY_COMBO_BOX => self.write(|p| p.update_source_category()),
            root::ID_SOURCE_TYPE_COMBO_BOX => {
                self.write(|p| p.handle_source_line_2_combo_box_change())
            }
            root::ID_SOURCE_CHANNEL_COMBO_BOX => {
                self.write(|p| p.handle_source_line_3_combo_box_change())
            }
            root::ID_SOURCE_NUMBER_COMBO_BOX => {
                self.write(|p| p.handle_source_line_4_combo_box_2_change())
            }
            root::ID_SOURCE_LINE_4_COMBO_BOX_1 => {
                self.write(|p| p.handle_source_line_4_combo_box_1_change())
            }
            root::ID_SOURCE_CHARACTER_COMBO_BOX => {
                self.write(|p| p.handle_source_line_5_combo_box_change())
            }
            // Mode
            root::ID_SETTINGS_MODE_COMBO_BOX => self.write(|p| p.update_mode_type()),
            root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX => {
                self.write(|p| p.update_mode_out_of_range_behavior())
            }
            root::ID_MODE_GROUP_INTERACTION_COMBO_BOX => {
                self.write(|p| p.update_mode_group_interaction())
            }
            root::ID_MODE_TAKEOVER_MODE => self.write(|p| p.update_takeover_mode()),
            root::ID_MODE_BUTTON_FILTER_COMBO_BOX => self.write(|p| p.update_button_usage()),
            root::ID_MODE_RELATIVE_FILTER_COMBO_BOX => self.write(|p| p.update_encoder_usage()),
            root::ID_MODE_FIRE_COMBO_BOX => self.write(|p| p.update_mode_fire_mode()),
            root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX => self.write(|p| p.update_mode_feedback_type()),
            // Target
            root::ID_TARGET_CATEGORY_COMBO_BOX => self.write(|p| p.update_target_category()),
            root::ID_TARGET_LINE_2_COMBO_BOX_1 => {
                self.write(|p| p.handle_target_line_2_combo_box_1_change())
            }
            root::ID_TARGET_LINE_2_COMBO_BOX_2 => {
                self.write(|p| p.handle_target_line_2_combo_box_2_change())
            }
            root::ID_TARGET_LINE_3_COMBO_BOX_1 => {
                self.write(|p| p.handle_target_line_3_combo_box_1_change());
            }
            root::ID_TARGET_LINE_3_COMBO_BOX_2 => {
                self.write(|p| p.handle_target_line_3_combo_box_2_change());
            }
            root::ID_TARGET_LINE_4_COMBO_BOX_1 => {
                self.write(|p| p.handle_target_line_4_combo_box_1_change())
            }
            root::ID_TARGET_LINE_4_COMBO_BOX_2 => {
                self.write(|p| p.handle_target_line_4_combo_box_2_change())
            }
            _ => {}
        }
    }

    fn slider_moved(self: SharedView<Self>, slider: Window) {
        let cloned_self = self.clone();
        let sliders = cloned_self.window_cache.borrow();
        let sliders = sliders.as_ref().expect("sliders not set");
        match slider {
            // Mode
            s if s == sliders.mode_min_target_value => {
                self.write(|p| p.update_mode_min_target_value_from_slider(s));
            }
            s if s == sliders.mode_max_target_value => {
                self.write(|p| p.update_mode_max_target_value_from_slider(s));
            }
            s if s == sliders.mode_min_source_value => {
                self.write(|p| p.update_mode_min_source_value_from_slider(s));
            }
            s if s == sliders.mode_max_source_value => {
                self.write(|p| p.update_mode_max_source_value_from_slider(s));
            }
            s if s == sliders.mode_min_step_size => {
                self.write(|p| p.update_mode_min_step_from_slider(s));
            }
            s if s == sliders.mode_max_step_size => {
                self.write(|p| p.update_mode_max_step_from_slider(s));
            }
            s if s == sliders.mode_fire_line_2 => {
                self.write(|p| p.handle_mode_fire_line_2_slider_change(s));
            }
            s if s == sliders.mode_fire_line_3 => {
                self.write(|p| p.handle_mode_fire_line_3_slider_change(s));
            }
            s if s == sliders.target_value => {
                let _ = self.read(|p| p.hit_target(s.slider_unit_value()));
            }
            _ => {}
        };
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        if self.is_invoked_programmatically() {
            // We don't want to continue if the edit control change was not caused by the user.
            // Although the edit control text is changed programmatically, it also triggers the
            // change handler. Ignore it! Most of those events are filtered out already
            // by the dialog proc reentrancy check, but this one is not because the
            // dialog proc is not reentered - we are just reacting (async) to a change.
            return false;
        }
        let view = if cfg!(target_os = "linux") {
            // On Linux we handle the change immediately because SWELL on Linux doesn't support
            // notification on focus kill.
            match self.handle_potential_min_max_edit_control_change(resource_id) {
                // Processed
                None => return true,
                // Not processed
                Some(v) => v,
            }
        } else {
            // On macOS and Windows we don't update the min/max values instantly but when leaving
            // the edit field. This prevents annoying too clever behavior where e.g. entering the
            // max value would "fix" a previously entered min value too soon.
            self
        };
        match resource_id {
            // Source
            root::ID_SOURCE_LINE_3_EDIT_CONTROL => {
                view.write(|p| p.handle_source_line_3_edit_control_change());
            }
            root::ID_SOURCE_NUMBER_EDIT_CONTROL => {
                view.write(|p| p.handle_source_line_4_edit_control_change());
            }
            root::ID_SOURCE_LINE_5_EDIT_CONTROL => {
                view.write(|p| p.handle_source_line_5_edit_control_change());
            }
            root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL => {
                view.write(|p| p.handle_source_line_7_edit_control_change());
            }
            // Mode
            root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL => {
                view.write(|p| p.update_mode_target_value_sequence());
            }
            root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL => {
                view.write(|p| p.update_mode_eel_control_transformation());
            }
            root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL => {
                view.write(|p| p.update_mode_feedback_transformation());
            }
            // Target
            root::ID_TARGET_LINE_2_EDIT_CONTROL => {
                view.write(|p| p.handle_target_line_2_edit_control_change())
            }
            root::ID_TARGET_LINE_3_EDIT_CONTROL => {
                view.write(|p| p.handle_target_line_3_edit_control_change())
            }
            root::ID_TARGET_LINE_4_EDIT_CONTROL => {
                view.write(|p| p.handle_target_line_4_edit_control_change())
            }
            root::ID_TARGET_LINE_5_EDIT_CONTROL => {
                view.write(|p| p.handle_target_line_5_edit_control_change())
            }
            root::ID_TARGET_VALUE_EDIT_CONTROL => {
                let value = view.clone().write(|p| {
                    p.get_value_from_target_edit_control(root::ID_TARGET_VALUE_EDIT_CONTROL)
                        .unwrap_or(UnitValue::MIN)
                });
                let _ = view.read(|p| p.hit_target(value));
            }
            _ => return false,
        };
        true
    }

    // This is not called on Linux anyway, so this guard is just for making sure that nothing breaks
    // or is done two times if SWELL supports focus kill notification at some point on Linux.
    #[cfg(not(target_os = "linux"))]
    fn edit_control_focus_killed(self: SharedView<Self>, resource_id: u32) -> bool {
        if self.is_invoked_programmatically() {
            return false;
        }
        if self.session().try_borrow_mut().is_err() {
            // This happens e.g. when the edit control had focus while removing the mapping.
            return false;
        }
        let view = self.clone();
        self.handle_potential_min_max_edit_control_change(resource_id);
        // This is also called when the window is hidden.
        // The edit control which is currently edited by the user doesn't get invalidated during
        // `edit_control_changed()`, for good reasons. But as soon as the edit control loses
        // focus, we should invalidate it. This is especially important if the user
        // entered an invalid value. Because we are lazy and edit controls are not
        // manipulated very frequently, we just invalidate all controls.
        // If this fails (because the mapping is not filled anymore), it's not a problem.
        let _ = view.read(|p| {
            p.invalidate_all_controls();
        });
        false
    }

    fn timer(&self, id: usize) -> bool {
        if id == SOURCE_MATCH_INDICATOR_TIMER_ID {
            self.view
                .require_window()
                .kill_timer(SOURCE_MATCH_INDICATOR_TIMER_ID);
            self.source_match_indicator_control().disable();
            true
        } else {
            false
        }
    }

    fn mouse_test(self: SharedView<Self>, position: Point<i32>) -> bool {
        let position = self.view.require_window().screen_to_client_point(position);
        let container = self.ui_element_container.borrow();
        let mut resource_ids = container.hit_test(Point::new(position.x, position.y));
        let resource_id = resource_ids.find(|id| !NO_HELP_ELEMENTS.contains(id));
        self.handle_hovered_ui_element(resource_id);
        false
    }

    fn help_requested(&self) -> bool {
        self.open_detailed_help_for_current_ui_element().is_some()
    }
}

const SOURCE_MATCH_INDICATOR_TIMER_ID: usize = 570;

trait WindowExt {
    fn slider_unit_value(&self) -> UnitValue;
    fn slider_discrete_increment(&self) -> DiscreteIncrement;
    fn slider_duration(&self) -> Duration;
    fn set_slider_unit_value(&self, value: UnitValue);
    fn set_slider_discrete_increment(&self, increment: DiscreteIncrement);
    fn set_slider_duration(&self, value: Duration);
}

impl WindowExt for Window {
    fn slider_unit_value(&self) -> UnitValue {
        let discrete_value = self.slider_value();
        UnitValue::new_clamped(discrete_value as f64 / 100.0)
    }

    fn slider_discrete_increment(&self) -> DiscreteIncrement {
        let discrete_value = self.slider_value() as i32 * 2 - 100;
        discrete_value
            .try_into()
            .unwrap_or(DiscreteIncrement::POSITIVE_MIN)
    }

    fn slider_duration(&self) -> Duration {
        let discrete_value = self.slider_value();
        Duration::from_millis((discrete_value * 50) as _)
    }

    fn set_slider_unit_value(&self, value: UnitValue) {
        // TODO-low Refactor that map_to_interval stuff to be more generic and less boilerplate
        self.set_slider_range(0, 100);
        let val = (value.get() * 100.0).round() as u32;
        self.set_slider_value(val);
    }

    fn set_slider_discrete_increment(&self, increment: DiscreteIncrement) {
        let val = cmp::max(0, (increment.get() + 100) / 2) as u32;
        self.set_slider_value(val);
    }

    fn set_slider_duration(&self, value: Duration) {
        // 0 = 0ms, 1 = 50ms, ..., 100 = 5s
        self.set_slider_range(0, 100);
        let val = (value.as_millis() / 50) as u32;
        self.set_slider_value(val);
    }
}

fn group_mappings_by_virtual_control_element<'a>(
    mappings: impl Iterator<Item = &'a SharedMapping>,
) -> NonCryptoHashMap<VirtualControlElement, Vec<&'a SharedMapping>> {
    let key_fn = |m: &SharedMapping| {
        let m = m.borrow();
        match m.target_model.category() {
            TargetCategory::Reaper => None,
            TargetCategory::Virtual => Some(m.target_model.create_control_element()),
        }
    };
    // Group by Option<VirtualControlElement>
    let grouped_by_option = mappings
        .sorted_by_key(|m| key_fn(m))
        .group_by(|m| key_fn(m));
    // Filter out None keys and collect to map with vector values
    grouped_by_option
        .into_iter()
        .filter_map(|(key, group)| key.map(|k| (k, group.collect())))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn invalidate_target_controls_free(
    real_target: Option<&CompoundMappingTarget>,
    slider_control: Window,
    edit_control: Window,
    value_text_control: Window,
    value: AbsoluteValue,
    initiator: Option<u32>,
    edit_control_id: u32,
    set_text_only_if_edit_control_not_focused: bool,
    // Whether to display/enter step sizes instead of absolute values.
    use_step_sizes: bool,
    unit: TargetUnit,
    control_context: ControlContext,
) {
    // TODO-high-discrete Handle discrete value in a better way.
    let value = value.to_unit_value();
    let (edit_text, value_text) = match real_target {
        Some(target) => match unit {
            TargetUnit::Native => {
                if target.character(control_context) == TargetCharacter::Discrete {
                    let edit_text = target
                        .convert_unit_value_to_discrete_value(value, control_context)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| "".to_string());
                    (
                        edit_text,
                        get_text_right_to_target_edit_control(target, value, control_context),
                    )
                } else if use_step_sizes {
                    (
                        target.format_step_size_without_unit(value, control_context),
                        get_text_right_to_step_size_edit_control(target, value, control_context),
                    )
                } else {
                    (
                        target.format_value_without_unit(value, control_context),
                        get_text_right_to_target_edit_control(target, value, control_context),
                    )
                }
            }
            TargetUnit::Percent => (format_percentage_without_unit(value.get()), "%".to_owned()),
        },
        None => ("".to_string(), "".to_string()),
    };
    slider_control.set_slider_unit_value(value);
    // Value edit control
    if initiator != Some(edit_control_id)
        && (!set_text_only_if_edit_control_not_focused || !edit_control.has_focus())
    {
        edit_control.set_text(edit_text);
    }
    // Value label
    value_text_control.set_text(value_text);
}

fn get_text_right_to_target_edit_control(
    t: &CompoundMappingTarget,
    value: UnitValue,
    control_context: ControlContext,
) -> String {
    if t.hide_formatted_value(control_context) {
        t.value_unit(control_context).to_string()
    } else if t.character(control_context) == TargetCharacter::Discrete {
        // Please note that discrete FX parameters can only show their *current* value,
        // unless they implement the REAPER VST extension functions.
        t.format_value(value, control_context)
    } else {
        format!(
            "{}  {}",
            t.value_unit(control_context),
            t.format_value(value, control_context)
        )
    }
}

fn get_text_right_to_step_size_edit_control(
    t: &CompoundMappingTarget,
    step_size: UnitValue,
    control_context: ControlContext,
) -> String {
    if t.hide_formatted_step_size(control_context) {
        t.step_size_unit(control_context).to_string()
    } else {
        format!(
            "{}  {}",
            t.step_size_unit(control_context),
            t.format_step_size_without_unit(step_size, control_context)
        )
    }
}

fn track_combo_box_entries(project: Project) -> impl ExactSizeIterator<Item = String> {
    let mut current_folder_level: i32 = 0;
    project.tracks().enumerate().map(move |(i, track)| {
        let indentation = ".".repeat(current_folder_level.unsigned_abs() as usize * 4);
        let space = if indentation.is_empty() { "" } else { " " };
        let name = track.name().expect("non-master track must have name");
        let label = format!("{}. {}{}{}", i + 1, indentation, space, name.to_str());
        current_folder_level += track.folder_depth_change();
        label
    })
}

fn fx_combo_box_entries(chain: &FxChain) -> impl ExactSizeIterator<Item = String> + '_ {
    chain
        .fxs()
        .enumerate()
        .map(|(i, fx)| get_fx_label(i as u32, &fx))
}

fn send_combo_box_entries(track: &Track, route_type: TrackRouteType) -> Vec<String> {
    match route_type {
        TrackRouteType::Send => track
            .typed_sends(SendPartnerType::Track)
            .map(|route| get_route_label(&route))
            .collect(),
        TrackRouteType::Receive => track
            .receives()
            .map(|route| get_route_label(&route))
            .collect(),
        TrackRouteType::HardwareOutput => track
            .typed_sends(SendPartnerType::HardwareOutput)
            .map(|route| get_route_label(&route))
            .collect(),
    }
}

fn fx_parameter_combo_box_entries(fx: &Fx) -> impl ExactSizeIterator<Item = String> + '_ {
    fx.parameters()
        .map(|param| get_fx_param_label(Some(&param), param.index()).to_string())
}

fn bookmark_combo_box_entries(
    project: Project,
    bookmark_type: BookmarkType,
) -> impl Iterator<Item = (isize, String)> {
    project
        .bookmarks()
        .map(|b| (b, b.basic_info()))
        .filter(move |(_, info)| info.bookmark_type() == bookmark_type)
        .map(|(b, info)| {
            let name = b.name();
            let label = get_bookmark_label_by_id(info.bookmark_type(), info.id, &name);
            (info.id.get() as isize, label)
        })
}

fn select_bookmark_in_combo_box(combo: Window, anchor_type: BookmarkAnchorType, bookmark_ref: u32) {
    let successful = match anchor_type {
        BookmarkAnchorType::Id => combo
            .select_combo_box_item_by_data(bookmark_ref as _)
            .is_ok(),
        BookmarkAnchorType::Index => combo
            .select_combo_box_item_by_index_checked(bookmark_ref as _)
            .is_ok(),
    };
    if !successful {
        combo.select_new_combo_box_item(
            get_non_present_bookmark_label(anchor_type, bookmark_ref).as_str(),
        );
    }
}

fn invalidate_target_line_2_expression_result(
    target: &TargetModel,
    context: ExtendedProcessorContext,
    label: Window,
    compartment: CompartmentKind,
) {
    let text = match target.category() {
        TargetCategory::Reaper => {
            if target.target_type().supports_track() && target.track_type().is_dynamic() {
                target
                    .virtual_track()
                    .and_then(|t| t.calculated_track_index(context, compartment))
                    .map(|i| i.to_string())
            } else {
                None
            }
        }
        TargetCategory::Virtual => None,
    };
    label.set_text_or_hide(text);
}

fn invalidat_target_line_3_expression_result(
    target: &TargetModel,
    context: ExtendedProcessorContext,
    label: Window,
    compartment: CompartmentKind,
) {
    let text = match target.category() {
        TargetCategory::Reaper => {
            if target.target_type().supports_fx() && target.fx_type() == VirtualFxType::Dynamic {
                target
                    .with_context(context, compartment)
                    .first_fx_chain()
                    .ok()
                    .and_then(|chain| {
                        target
                            .virtual_chain_fx()
                            .and_then(|fx| fx.calculated_fx_index(context, compartment, &chain))
                            .map(|i| i.to_string())
                    })
            } else {
                None
            }
        }
        TargetCategory::Virtual => None,
    };
    label.set_text_or_hide(text);
}

fn invalidate_target_line_4_expression_result(
    target: &TargetModel,
    context: ExtendedProcessorContext,
    label: Window,
    compartment: CompartmentKind,
) {
    let text = match target.category() {
        TargetCategory::Reaper => match target.target_type() {
            t if t.supports_fx_parameter()
                && target.param_type() == VirtualFxParameterType::Dynamic =>
            {
                target
                    .with_context(context, compartment)
                    .first_fx()
                    .ok()
                    .and_then(|fx| {
                        target
                            .virtual_fx_parameter()
                            .and_then(|p| {
                                p.calculated_fx_parameter_index(context, compartment, &fx)
                            })
                            .map(|i| i.to_string())
                    })
            }
            t if t.supports_send()
                && target.route_selector_type() == TrackRouteSelectorType::Dynamic =>
            {
                target
                    .track_route_selector()
                    .and_then(|p| p.calculated_route_index(context, compartment))
                    .map(|i| i.to_string())
            }
            _ => None,
        },
        TargetCategory::Virtual => None,
    };
    label.set_text_or_hide(text);
}

fn parse_position_as_index(edit_control: Window) -> u32 {
    let position: i32 = edit_control
        .text()
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(1);
    std::cmp::max(position - 1, 0) as u32
}

fn chunked_number_menu<R>(
    count: u32,
    batch_size: u32,
    format_one_rooted: bool,
    f: impl Fn(u32) -> swell_ui::menu_tree::Entry<R>,
) -> Vec<swell_ui::menu_tree::Entry<R>> {
    use swell_ui::menu_tree::*;
    (0..count / batch_size)
        .map(|batch_index| {
            let offset = batch_index * batch_size;
            let range = offset..(offset + batch_size);
            menu(
                format!(
                    "{} - {}",
                    if format_one_rooted {
                        range.start + 1
                    } else {
                        range.start
                    },
                    if format_one_rooted {
                        range.end
                    } else {
                        range.end - 1
                    }
                ),
                range.map(&f).collect(),
            )
        })
        .collect()
}

fn channel_menu<R>(f: impl Fn(u8) -> R) -> Vec<R> {
    (0..16).map(&f).collect()
}

fn invalidate_with_osc_arg_type_tag(b: Window, tag: OscTypeTag) {
    b.fill_combo_box_indexed(OscTypeTag::iter());
    b.show();
    b.select_combo_box_item_by_index(tag.into());
}

fn invalidate_with_osc_arg_index(combo: Window, index: Option<u32>) {
    combo.fill_combo_box_with_data_small(osc_arg_indexes());
    combo.show();
    let data = index.map(|i| i as isize).unwrap_or(-1);
    combo.select_combo_box_item_by_data(data).unwrap();
}

fn get_osc_type_tag_from_combo(combo: Window) -> OscTypeTag {
    let i = combo.selected_combo_box_item_index();
    i.try_into().expect("invalid OSC type tag")
}

fn get_osc_arg_index_from_combo(combo: Window) -> Option<u32> {
    match combo.selected_combo_box_item_data() {
        -1 => None,
        i => Some(i as u32),
    }
}

fn osc_arg_indexes() -> impl Iterator<Item = (isize, String)> {
    iter::once((-1isize, "-".to_string())).chain((0..9).map(|i| (i as isize, (i + 1).to_string())))
}

fn pick_virtual_control_element(
    window: Window,
    character: VirtualControlElementCharacter,
    grouped_mappings: &NonCryptoHashMap<VirtualControlElement, Vec<&SharedMapping>>,
    for_control: bool,
    for_feedback: bool,
) -> Option<String> {
    let pure_menu = {
        use swell_ui::menu_tree::*;
        let daw_control_names: Vec<_> =
            control_element_domains::daw::PREDEFINED_VIRTUAL_BUTTON_NAMES
                .iter()
                .chain(control_element_domains::daw::PREDEFINED_VIRTUAL_MULTI_NAMES.iter())
                .copied()
                .collect();
        let grid_control_names: Vec<_> =
            control_element_domains::grid::PREDEFINED_VIRTUAL_BUTTON_NAMES
                .iter()
                .chain(control_element_domains::grid::PREDEFINED_VIRTUAL_MULTI_NAMES.iter())
                .copied()
                .collect();

        let include_actual_element = |mappings: &[&SharedMapping]| -> bool {
            mappings.iter().any(|m| {
                let m = m.borrow();
                if !m.is_enabled() {
                    return false;
                }
                for_control && m.control_is_enabled() || for_feedback && m.feedback_is_enabled()
            })
        };
        let currently_available_elements: Vec<_> = grouped_mappings
            .iter()
            .filter(|(e, mappings)| match e {
                VirtualControlElement::Indexed { character: ch, .. } => {
                    *ch == character && include_actual_element(mappings)
                }
                VirtualControlElement::Named { .. } => include_actual_element(mappings),
            })
            .map(|(e, _)| e.id().to_string())
            .collect();
        let entries = vec![
            menu(
                "<Currently available>",
                build_slash_menu_entries(&currently_available_elements, ""),
            ),
            menu(
                "DAW control",
                build_slash_menu_entries(&daw_control_names, ""),
            ),
            menu("Grid", build_slash_menu_entries(&grid_control_names, "")),
            menu(
                "Numbered",
                chunked_number_menu(100, 10, true, |i| {
                    let label = {
                        let pos = i + 1;
                        let element = VirtualControlElement::new(
                            VirtualControlElementId::Indexed(i),
                            character,
                        );
                        match grouped_mappings.get(&element) {
                            None => pos.to_string(),
                            Some(mappings) => {
                                let first_mapping = mappings[0].borrow();
                                let first_mapping_name = first_mapping.effective_name();
                                if mappings.len() == 1 {
                                    format!("{pos} ({first_mapping_name})")
                                } else {
                                    format!(
                                        "{} ({} + {})",
                                        pos,
                                        first_mapping_name,
                                        mappings.len() - 1
                                    )
                                }
                            }
                        }
                    };
                    item(label, (i + 1).to_string())
                }),
            ),
        ];
        anonymous_menu(entries)
    };
    window.open_popup_menu(pure_menu, Window::cursor_pos())
}

#[derive(Copy, Clone, Display)]
enum ColorTarget {
    #[display(fmt = "Color (if supported)")]
    Color,
    #[display(fmt = "Background color (if supported)")]
    BackgroundColor,
}

enum FeedbackPopupMenuResult {
    EditMultiLine,
    ChangeColor(ChangeColorInstruction),
}

struct ChangeColorInstruction {
    target: ColorTarget,
    color: Option<VirtualColor>,
}

impl ChangeColorInstruction {
    fn new(target: ColorTarget, color: Option<VirtualColor>) -> Self {
        Self { target, color }
    }
}

fn show_feedback_popup_menu(
    window: Window,
    current_color: Option<VirtualColor>,
    current_background_color: Option<VirtualColor>,
) -> Result<FeedbackPopupMenuResult, &'static str> {
    enum MenuAction {
        ControllerDefault(ColorTarget),
        OpenColorPicker(ColorTarget),
        UseColorProp(ColorTarget, &'static str),
        EditMultiLine,
    }
    let pure_menu = {
        use swell_ui::menu_tree::*;
        use MenuAction::*;
        let create_color_target_menu = |color_target: ColorTarget| {
            let relevant_color = match color_target {
                ColorTarget::Color => &current_color,
                ColorTarget::BackgroundColor => &current_background_color,
            };
            menu(
                color_target.to_string(),
                [
                    item_with_opts(
                        "<Default color>",
                        ItemOpts {
                            enabled: true,
                            checked: relevant_color.is_none(),
                        },
                        ControllerDefault(color_target),
                    ),
                    item_with_opts(
                        "<Pick color...>",
                        ItemOpts {
                            enabled: true,
                            checked: matches!(relevant_color, Some(VirtualColor::Rgb(_))),
                        },
                        OpenColorPicker(color_target),
                    ),
                ].into_iter()
                    .chain(["target.track.color", "target.bookmark.color", "target.slot.color"].into_iter().map(|key| {
                        item_with_opts(
                            key,
                            ItemOpts {
                                enabled: true,
                                checked: {
                                    matches!(relevant_color, Some(VirtualColor::Prop{prop}) if key == prop)
                                },
                            },
                            UseColorProp(color_target, key),
                        )
                    }))
                    .collect(),
            )
        };
        let entries = vec![
            item("Edit multi-line...", MenuAction::EditMultiLine),
            create_color_target_menu(ColorTarget::Color),
            create_color_target_menu(ColorTarget::BackgroundColor),
        ];
        anonymous_menu(entries)
    };
    let item = window
        .open_popup_menu(pure_menu, Window::cursor_pos())
        .ok_or("color selection cancelled")?;
    let result = match item {
        MenuAction::EditMultiLine => FeedbackPopupMenuResult::EditMultiLine,
        MenuAction::ControllerDefault(target) => {
            let instruction = ChangeColorInstruction::new(target, None);
            FeedbackPopupMenuResult::ChangeColor(instruction)
        }
        MenuAction::UseColorProp(target, key) => {
            let instruction = ChangeColorInstruction::new(
                target,
                Some(VirtualColor::Prop {
                    prop: key.to_string(),
                }),
            );
            FeedbackPopupMenuResult::ChangeColor(instruction)
        }
        MenuAction::OpenColorPicker(color_target) => {
            let relevant_color = match color_target {
                ColorTarget::Color => &current_color,
                ColorTarget::BackgroundColor => &current_background_color,
            };
            let reaper = Reaper::get().medium_reaper();
            let current_color = if let Some(VirtualColor::Rgb(c)) = relevant_color {
                reaper.color_to_native(reaper_medium::RgbColor::rgb(c.r(), c.g(), c.b()))
            } else {
                NativeColor::default()
            };
            if let Some(native_color) =
                reaper.gr_select_color(WindowContext::Win(window.raw_hwnd()), current_color)
            {
                let reaper_medium::RgbColor { r, g, b } = reaper.color_from_native(native_color);
                let instruction = ChangeColorInstruction::new(
                    color_target,
                    Some(VirtualColor::Rgb(RgbColor::new(r, g, b))),
                );
                FeedbackPopupMenuResult::ChangeColor(instruction)
            } else {
                return Err("color picking cancelled");
            }
        }
    };
    Ok(result)
}

enum SendMidiMenuAction {
    EditMultiLine,
    SingleDataBytePreset {
        channel: u8,
        msg_type: ShortMessageType,
        last_byte: u8,
    },
    DoubleDataBytePreset {
        channel: u8,
        msg_type: ShortMessageType,
        i: u32,
    },
    PitchBendChangePreset {
        channel: u8,
    },
}

fn open_send_midi_menu(window: Window) -> Option<SendMidiMenuAction> {
    fn fmt_ch(ch: u8) -> String {
        format!("Channel {}", ch + 1)
    }
    fn double_data_byte_msg_menu(
        source_type: MidiSourceType,
        msg_type: ShortMessageType,
        label: &str,
    ) -> swell_ui::menu_tree::Entry<SendMidiMenuAction> {
        use swell_ui::menu_tree::*;
        menu(
            source_type.to_string(),
            channel_menu(|ch| {
                menu(
                    fmt_ch(ch),
                    chunked_number_menu(128, 8, false, |i| {
                        item(
                            format!("{label} {i}"),
                            SendMidiMenuAction::DoubleDataBytePreset {
                                channel: ch,
                                msg_type,
                                i,
                            },
                        )
                    }),
                )
            }),
        )
    }

    fn single_data_byte_msg_menu(
        source_type: MidiSourceType,
        msg_type: ShortMessageType,
        last_byte: u8,
    ) -> swell_ui::menu_tree::Entry<SendMidiMenuAction> {
        use swell_ui::menu_tree::*;
        menu(
            source_type.to_string(),
            channel_menu(|ch| {
                item(
                    fmt_ch(ch),
                    SendMidiMenuAction::SingleDataBytePreset {
                        channel: ch,
                        msg_type,
                        last_byte,
                    },
                )
            }),
        )
    }

    let pure_menu = {
        use swell_ui::menu_tree::*;

        use SendMidiMenuAction::*;
        let entries = vec![
            item("Edit multi-line...", EditMultiLine),
            double_data_byte_msg_menu(
                MidiSourceType::ControlChangeValue,
                ShortMessageType::ControlChange,
                "CC",
            ),
            double_data_byte_msg_menu(
                MidiSourceType::NoteVelocity,
                ShortMessageType::NoteOn,
                "Note",
            ),
            single_data_byte_msg_menu(MidiSourceType::NoteKeyNumber, ShortMessageType::NoteOn, 127),
            menu(
                MidiSourceType::PitchBendChangeValue.to_string(),
                channel_menu(|ch| {
                    item(
                        fmt_ch(ch),
                        SendMidiMenuAction::PitchBendChangePreset { channel: ch },
                    )
                }),
            ),
            single_data_byte_msg_menu(
                MidiSourceType::ChannelPressureAmount,
                ShortMessageType::ChannelPressure,
                0,
            ),
            single_data_byte_msg_menu(
                MidiSourceType::ProgramChangeNumber,
                ShortMessageType::ProgramChange,
                0,
            ),
            double_data_byte_msg_menu(
                MidiSourceType::PolyphonicKeyPressureAmount,
                ShortMessageType::PolyphonicKeyPressure,
                "Note",
            ),
        ];
        anonymous_menu(entries)
    };
    window.open_popup_menu(pure_menu, Window::cursor_pos())
}

fn build_slash_menu_entries(
    names: &[impl AsRef<str>],
    prefix: &str,
) -> Vec<swell_ui::menu_tree::Entry<String>> {
    use swell_ui::menu_tree::*;
    let mut entries = Vec::new();
    for (key, group) in &names
        .iter()
        .map(|e| e.as_ref())
        .sorted()
        .group_by(|name| extract_first_segment(name))
    {
        if key.is_empty() {
            // All non-nested entries on this level (items).
            for name in group {
                let full_name = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{prefix}/{name}")
                };
                entries.push(item(name, full_name));
            }
        } else {
            // A nested entry (menu).
            let remaining_names: Vec<_> = group.map(extract_remaining_name).collect();
            let new_prefix = if prefix.is_empty() {
                key.to_string()
            } else {
                format!("{prefix}/{key}")
            };
            let inner_entries = build_slash_menu_entries(&remaining_names, &new_prefix);
            entries.push(menu(key, inner_entries));
        }
    }
    entries
}

fn extract_first_segment(text: &str) -> &str {
    if let Some(slash_index) = text.find('/') {
        &text[0..slash_index]
    } else {
        ""
    }
}

fn extract_remaining_name(text: &str) -> &str {
    if let Some(slash_index) = text.find('/') {
        &text[slash_index + 1..]
    } else {
        text
    }
}

fn format_osc_arg_value_range(range: Interval<f64>, type_tag: OscTypeTag) -> String {
    if type_tag.is_discrete() {
        format!("{:.0} - {:.0}", range.min_val(), range.max_val())
    } else {
        format!("{:.4} - {:.4}", range.min_val(), range.max_val())
    }
}

fn parse_osc_arg_value_range(text: &str) -> Interval<f64> {
    use nom::character::complete::space0;
    use nom::number::complete::double;
    use nom::sequence::separated_pair;
    use nom::{character::complete::char, sequence::tuple, IResult};
    fn parse_range(input: &str) -> IResult<&str, Interval<f64>> {
        let mut parser = separated_pair(double, tuple((space0, char('-'), space0)), double);
        let (remainder, (from, to)) = parser(input)?;
        Ok((remainder, Interval::new_auto(from, to)))
    }
    parse_range(text)
        .map(|r| r.1)
        .unwrap_or(DEFAULT_OSC_ARG_VALUE_RANGE)
}

fn extract_first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn has_multiple_lines(text: &str) -> bool {
    text.lines().count() > 1
}

fn get_relevant_target_fx(mapping: &MappingModel, session: &UnitModel) -> Option<Fx> {
    let is_focused_fx_type = {
        let fx_type = mapping.target_model.fx_type();
        if fx_type == VirtualFxType::Unit {
            session.instance_fx_descriptor() == &FxDescriptor::Focused
        } else {
            fx_type == VirtualFxType::Focused
        }
    };
    if is_focused_fx_type {
        // We need to choose the last focused FX, even if it's not focused anymore. See
        // https://github.com/helgoboss/helgobox/issues/778.
        let containing_fx = session.control_context().processor_context.containing_fx();
        Backbone::get().last_relevant_available_focused_fx(containing_fx)
    } else {
        // Choose whatever FX the selector currently resolves to
        mapping
            .target_model
            .with_context(session.extended_context(), mapping.compartment())
            .first_fx()
            .ok()
    }
}

const ACTIVITY_INFO: &str = "Activity info";

enum Side {
    Left,
    Right,
}

fn build_mapping_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 0,
        y: 0,
        width: 451,
        height: 67,
        color_pair: colors::mapping(),
        scaling: MAPPING_PANEL_SCALING,
    }
}

fn build_source_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 0,
        y: 67,
        width: 175,
        height: 165,
        color_pair: colors::source(),
        scaling: MAPPING_PANEL_SCALING,
    }
}

fn build_target_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 175,
        y: 67,
        width: 276,
        height: 165,
        color_pair: colors::target(),
        scaling: MAPPING_PANEL_SCALING,
    }
}

fn build_glue_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 0,
        y: 232,
        width: 451,
        height: 239,
        color_pair: colors::glue(),
        scaling: MAPPING_PANEL_SCALING,
    }
}

fn build_help_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 0,
        y: 471,
        width: 451,
        height: 61,
        color_pair: colors::help(),
        scaling: MAPPING_PANEL_SCALING,
    }
}

enum Section {
    Mapping,
    Source,
    Target,
    Glue,
    Help,
}

impl Section {
    pub fn from_resource_id(id: u32) -> Option<Self> {
        use root::*;
        let section = match id {
            ID_MAPPING_PANEL_MAPPING_LABEL
            | ID_MAPPING_PANEL_FEEDBACK_LABEL
            | ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX
            | ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX
            | ID_MAPPING_ADVANCED_BUTTON
            | ID_MAPPING_FIND_IN_LIST_BUTTON => Self::Mapping,
            ID_MAPPING_PANEL_SOURCE_LABEL
            | ID_SOURCE_LEARN_BUTTON
            | ID_MAPPING_PANEL_SOURCE_CATEGORY_LABEL
            | ID_SOURCE_CATEGORY_COMBO_BOX
            | ID_SOURCE_TYPE_LABEL_TEXT
            | ID_SOURCE_TYPE_COMBO_BOX
            | ID_SOURCE_MIDI_MESSAGE_TYPE_LABEL_TEXT
            | ID_SOURCE_CHANNEL_LABEL
            | ID_SOURCE_CHANNEL_COMBO_BOX
            | ID_SOURCE_LINE_3_EDIT_CONTROL
            | ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT
            | ID_SOURCE_RPN_CHECK_BOX
            | ID_SOURCE_LINE_4_COMBO_BOX_1
            | ID_SOURCE_NUMBER_EDIT_CONTROL
            | ID_SOURCE_NUMBER_COMBO_BOX
            | ID_SOURCE_LINE_4_BUTTON
            | ID_SOURCE_CHARACTER_LABEL_TEXT
            | ID_SOURCE_CHARACTER_COMBO_BOX
            | ID_SOURCE_LINE_5_EDIT_CONTROL
            | ID_SOURCE_14_BIT_CHECK_BOX
            | ID_SOURCE_OSC_ADDRESS_LABEL_TEXT
            | ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL
            | ID_SOURCE_SCRIPT_DETAIL_BUTTON => Self::Source,
            ID_MAPPING_PANEL_TARGET_LABEL
            | ID_TARGET_LEARN_BUTTON
            | ID_TARGET_MENU_BUTTON
            | ID_TARGET_HINT
            | ID_MAPPING_PANEL_TARGET_TYPE_LABEL
            | ID_TARGET_CATEGORY_COMBO_BOX
            | ID_TARGET_TYPE_BUTTON
            | ID_TARGET_LINE_2_LABEL_2
            | ID_TARGET_LINE_2_LABEL_3
            | ID_TARGET_LINE_2_LABEL_1
            | ID_TARGET_LINE_2_COMBO_BOX_1
            | ID_TARGET_LINE_2_EDIT_CONTROL
            | ID_TARGET_LINE_2_COMBO_BOX_2
            | ID_TARGET_LINE_2_BUTTON
            | ID_TARGET_LINE_3_LABEL_1
            | ID_TARGET_LINE_3_COMBO_BOX_1
            | ID_TARGET_LINE_3_EDIT_CONTROL
            | ID_TARGET_LINE_3_COMBO_BOX_2
            | ID_TARGET_LINE_3_LABEL_2
            | ID_TARGET_LINE_3_LABEL_3
            | ID_TARGET_LINE_3_BUTTON
            | ID_TARGET_LINE_4_LABEL_1
            | ID_TARGET_LINE_4_COMBO_BOX_1
            | ID_TARGET_LINE_4_EDIT_CONTROL
            | ID_TARGET_LINE_4_COMBO_BOX_2
            | ID_TARGET_LINE_4_LABEL_2
            | ID_TARGET_LINE_4_BUTTON
            | ID_TARGET_LINE_4_LABEL_3
            | ID_TARGET_LINE_5_LABEL_1
            | ID_TARGET_LINE_5_EDIT_CONTROL
            | ID_TARGET_CHECK_BOX_1
            | ID_TARGET_CHECK_BOX_2
            | ID_TARGET_CHECK_BOX_3
            | ID_TARGET_CHECK_BOX_4
            | ID_TARGET_CHECK_BOX_5
            | ID_TARGET_CHECK_BOX_6
            | ID_TARGET_VALUE_LABEL_TEXT
            | ID_TARGET_VALUE_OFF_BUTTON
            | ID_TARGET_VALUE_ON_BUTTON
            | ID_TARGET_VALUE_SLIDER_CONTROL
            | ID_TARGET_VALUE_EDIT_CONTROL
            | ID_TARGET_VALUE_TEXT
            | ID_TARGET_UNIT_BUTTON => Self::Target,
            ID_MAPPING_PANEL_GLUE_LABEL
            | ID_SETTINGS_RESET_BUTTON
            | ID_SETTINGS_SOURCE_LABEL
            | ID_SETTINGS_SOURCE_GROUP
            | ID_SETTINGS_SOURCE_MIN_LABEL
            | ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL
            | ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL
            | ID_SETTINGS_SOURCE_MAX_LABEL
            | ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL
            | ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL
            | ID_MODE_OUT_OF_RANGE_LABEL_TEXT
            | ID_MODE_OUT_OF_RANGE_COMBOX_BOX
            | ID_MODE_GROUP_INTERACTION_LABEL_TEXT
            | ID_MODE_GROUP_INTERACTION_COMBO_BOX
            | ID_SETTINGS_TARGET_LABEL_TEXT
            | ID_SETTINGS_TARGET_SEQUENCE_LABEL_TEXT
            | ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL
            | ID_SETTINGS_TARGET_GROUP
            | ID_SETTINGS_MIN_TARGET_LABEL_TEXT
            | ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL
            | ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL
            | ID_SETTINGS_MIN_TARGET_VALUE_TEXT
            | ID_SETTINGS_MAX_TARGET_LABEL_TEXT
            | ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL
            | ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL
            | ID_SETTINGS_MAX_TARGET_VALUE_TEXT
            | ID_SETTINGS_REVERSE_CHECK_BOX
            | IDC_MODE_FEEDBACK_TYPE_COMBO_BOX
            | ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL
            | IDC_MODE_FEEDBACK_TYPE_BUTTON
            | ID_MODE_KNOB_FADER_GROUP_BOX
            | ID_SETTINGS_MODE_LABEL
            | ID_SETTINGS_MODE_COMBO_BOX
            | ID_MODE_TAKEOVER_LABEL
            | ID_MODE_TAKEOVER_MODE
            | ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX
            | ID_MODE_EEL_CONTROL_TRANSFORMATION_LABEL
            | ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL
            | ID_MODE_EEL_CONTROL_TRANSFORMATION_DETAIL_BUTTON
            | ID_MODE_RELATIVE_GROUP_BOX
            | ID_SETTINGS_STEP_SIZE_LABEL_TEXT
            | ID_SETTINGS_STEP_SIZE_GROUP
            | ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT
            | ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL
            | ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL
            | ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT
            | ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT
            | ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL
            | ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL
            | ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT
            | ID_MODE_RELATIVE_FILTER_COMBO_BOX
            | ID_SETTINGS_ROTATE_CHECK_BOX
            | ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX
            | ID_MODE_BUTTON_GROUP_BOX
            | ID_MODE_FIRE_COMBO_BOX
            | ID_MODE_BUTTON_FILTER_COMBO_BOX
            | ID_MODE_FIRE_LINE_2_LABEL_1
            | ID_MODE_FIRE_LINE_2_SLIDER_CONTROL
            | ID_MODE_FIRE_LINE_2_EDIT_CONTROL
            | ID_MODE_FIRE_LINE_2_LABEL_2
            | ID_MODE_FIRE_LINE_3_LABEL_1
            | ID_MODE_FIRE_LINE_3_SLIDER_CONTROL
            | ID_MODE_FIRE_LINE_3_EDIT_CONTROL
            | ID_MODE_FIRE_LINE_3_LABEL_2 => Self::Glue,
            ID_MAPPING_HELP_LEFT_SUBJECT_LABEL
            | ID_MAPPING_HELP_LEFT_CONTENT_LABEL
            | IDC_MAPPING_MATCHED_INDICATOR_TEXT
            | ID_MAPPING_HELP_RIGHT_SUBJECT_LABEL
            | ID_MAPPING_HELP_RIGHT_CONTENT_LABEL
            | IDC_BEEP_ON_SUCCESS_CHECK_BOX
            | ID_MAPPING_PANEL_PREVIOUS_BUTTON
            | ID_MAPPING_PANEL_OK
            | ID_MAPPING_PANEL_NEXT_BUTTON
            | IDC_MAPPING_ENABLED_CHECK_BOX => Self::Help,
            _ => return None,
        };
        Some(section)
    }

    pub fn color_pair(&self) -> ColorPair {
        match self {
            Section::Mapping => colors::mapping(),
            Section::Source => colors::source(),
            Section::Target => colors::target(),
            Section::Glue => colors::glue(),
            Section::Help => colors::help(),
        }
    }
}

fn find_help_topic_for_resource(id: u32) -> Option<HelpTopic> {
    const RESOURCES_TO_HELP_TOPIC: &[(&[u32], HelpTopic)] = &[
        // Concepts
        (
            &[root::ID_MAPPING_PANEL_MAPPING_LABEL],
            HelpTopic::Concept(ConceptTopic::Mapping),
        ),
        (
            &[root::ID_MAPPING_PANEL_SOURCE_LABEL],
            HelpTopic::Concept(ConceptTopic::Source),
        ),
        (
            &[root::ID_SETTINGS_TARGET_LABEL_TEXT],
            HelpTopic::Concept(ConceptTopic::Target),
        ),
        (
            &[root::ID_MAPPING_PANEL_GLUE_LABEL],
            HelpTopic::Concept(ConceptTopic::Glue),
        ),
        // Mapping
        (
            &[
                root::ID_MAPPING_NAME_LABEL,
                root::ID_MAPPING_NAME_EDIT_CONTROL,
            ],
            HelpTopic::Mapping(MappingTopic::Name),
        ),
        (
            &[
                root::ID_MAPPING_TAGS_LABEL,
                root::ID_MAPPING_TAGS_EDIT_CONTROL,
            ],
            HelpTopic::Mapping(MappingTopic::Tags),
        ),
        (
            &[root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX],
            HelpTopic::Mapping(MappingTopic::ControlEnabled),
        ),
        (
            &[root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX],
            HelpTopic::Mapping(MappingTopic::FeedbackEnabled),
        ),
        (
            &[
                root::ID_MAPPING_ACTIVATION_TYPE_LABEL,
                root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX,
            ],
            HelpTopic::Mapping(MappingTopic::Active),
        ),
        (
            &[
                root::ID_MAPPING_PANEL_FEEDBACK_LABEL,
                root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX,
            ],
            HelpTopic::Mapping(MappingTopic::FeedbackMode),
        ),
        (
            &[root::ID_MAPPING_ADVANCED_BUTTON],
            HelpTopic::Mapping(MappingTopic::AdvancedSettings),
        ),
        (
            &[root::ID_MAPPING_FIND_IN_LIST_BUTTON],
            HelpTopic::Mapping(MappingTopic::FindInMappingList),
        ),
        (
            &[root::ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX],
            HelpTopic::Mapping(MappingTopic::ShowInProjection),
        ),
        (
            &[root::IDC_BEEP_ON_SUCCESS_CHECK_BOX],
            HelpTopic::Mapping(MappingTopic::BeepOnSuccess),
        ),
        (
            &[root::IDC_MAPPING_ENABLED_CHECK_BOX],
            HelpTopic::Mapping(MappingTopic::Enabled),
        ),
        (
            &[root::ID_MAPPING_PANEL_PREVIOUS_BUTTON],
            HelpTopic::Mapping(MappingTopic::PreviousMapping),
        ),
        (
            &[root::ID_MAPPING_PANEL_NEXT_BUTTON],
            HelpTopic::Mapping(MappingTopic::NextMapping),
        ),
        // Source
        (
            &[root::ID_SOURCE_LEARN_BUTTON],
            HelpTopic::Source(SourceTopic::Learn),
        ),
        (
            &[
                root::ID_MAPPING_PANEL_SOURCE_CATEGORY_LABEL,
                root::ID_SOURCE_CATEGORY_COMBO_BOX,
            ],
            HelpTopic::Source(SourceTopic::Category),
        ),
        (
            &[
                root::ID_SOURCE_TYPE_LABEL_TEXT,
                root::ID_SOURCE_TYPE_COMBO_BOX,
            ],
            HelpTopic::Source(SourceTopic::Type),
        ),
        // Target
        (
            &[root::ID_TARGET_LEARN_BUTTON],
            HelpTopic::Target(TargetTopic::Learn),
        ),
        (
            &[root::ID_TARGET_MENU_BUTTON],
            HelpTopic::Target(TargetTopic::Menu),
        ),
        (
            &[root::ID_TARGET_CATEGORY_COMBO_BOX],
            HelpTopic::Target(TargetTopic::Category),
        ),
        (
            &[
                root::ID_MAPPING_PANEL_TARGET_TYPE_LABEL,
                root::ID_TARGET_TYPE_BUTTON,
            ],
            HelpTopic::Target(TargetTopic::Type),
        ),
        (
            &[
                root::ID_TARGET_VALUE_LABEL_TEXT,
                root::ID_TARGET_VALUE_OFF_BUTTON,
                root::ID_TARGET_VALUE_ON_BUTTON,
                root::ID_TARGET_VALUE_SLIDER_CONTROL,
                root::ID_TARGET_VALUE_EDIT_CONTROL,
                root::ID_TARGET_VALUE_TEXT,
            ],
            HelpTopic::Target(TargetTopic::CurrentValue),
        ),
        (
            &[root::ID_TARGET_UNIT_BUTTON],
            HelpTopic::Target(TargetTopic::DisplayUnit),
        ),
        // Glue
        (
            SOURCE_MIN_MAX_ELEMENTS,
            HelpTopic::Glue(ModeParameter::SourceMinMax),
        ),
        (REVERSE_ELEMENTS, HelpTopic::Glue(ModeParameter::Reverse)),
        (
            OUT_OF_RANGE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::OutOfRangeBehavior),
        ),
        (
            TAKEOVER_MODE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::TakeoverMode),
        ),
        (
            CONTROL_TRANSFORMATION_ELEMENTS,
            HelpTopic::Glue(ModeParameter::ControlTransformation),
        ),
        (
            VALUE_SEQUENCE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::TargetValueSequence),
        ),
        (
            TARGET_MIN_MAX_ELEMENTS,
            HelpTopic::Glue(ModeParameter::TargetMinMax),
        ),
        (
            STEP_MIN_ELEMENTS,
            HelpTopic::Glue(ModeParameter::StepSizeMin),
        ),
        (
            STEP_MAX_ELEMENTS,
            HelpTopic::Glue(ModeParameter::StepSizeMax),
        ),
        (
            RELATIVE_FILTER_ELEMENTS,
            HelpTopic::Glue(ModeParameter::RelativeFilter),
        ),
        (FIRE_MODE_ELEMENTS, HelpTopic::Glue(ModeParameter::FireMode)),
        (
            MAKE_ABSOLUTE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::MakeAbsolute),
        ),
        (
            FEEDBACK_TYPE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::FeedbackType),
        ),
        (
            ROUND_TARGET_VALUE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::RoundTargetValue),
        ),
        (
            ABSOLUTE_MODE_ELEMENTS,
            HelpTopic::Glue(ModeParameter::AbsoluteMode),
        ),
        (
            GROUP_INTERACTION_ELEMENTS,
            HelpTopic::Glue(ModeParameter::GroupInteraction),
        ),
        (
            FEEDBACK_TRANSFORMATION_ELEMENTS,
            HelpTopic::Glue(ModeParameter::FeedbackTransformation),
        ),
        (ROTATE_ELEMENTS, HelpTopic::Glue(ModeParameter::Rotate)),
        (
            BUTTON_FILTER_ELEMENTS,
            HelpTopic::Glue(ModeParameter::ButtonFilter),
        ),
    ];
    RESOURCES_TO_HELP_TOPIC
        .iter()
        .find_map(|(elements, param)| {
            if !elements.contains(&id) {
                return None;
            }
            Some(*param)
        })
}

fn pick_png_file(current_file: Utf8PathBuf) -> Option<Utf8PathBuf> {
    let current_file = if current_file.as_str().trim().is_empty() {
        Reaper::get().resource_path()
    } else {
        current_file
    };
    Reaper::get()
        .medium_reaper()
        .get_user_file_name_for_read(&current_file, "Pick image", "png")
}
