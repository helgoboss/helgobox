use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::convert::TryInto;
use std::iter;
use std::ptr::null;
use std::rc::Rc;
use std::time::Duration;

use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_midi::{Channel, ShortMessageType, U7};
use itertools::Itertools;
use reaper_high::{
    BookmarkType, Fx, FxChain, Project, Reaper, SendPartnerType, Track, TrackRoutePartner,
};
use reaper_low::raw;
use reaper_medium::{InitialAction, PromptForActionResult, SectionId, WindowContext};
use rxrust::prelude::*;

use helgoboss_learn::{
    check_mode_applicability, format_percentage_without_unit, AbsoluteMode, AbsoluteValue,
    ButtonUsage, ControlValue, DetailedSourceCharacter, DisplayType, EncoderUsage, FeedbackType,
    FireMode, GroupInteraction, MackieSevenSegmentDisplayScope, MidiClockTransportMessage,
    ModeApplicabilityCheckInput, ModeParameter, OscTypeTag, OutOfRangeBehavior, PercentIo,
    RgbColor, SoftSymmetricUnitValue, SourceCharacter, TakeoverMode, Target, UnitValue,
    ValueSequence, VirtualColor,
};
use swell_ui::{
    DialogUnits, MenuBar, Point, SharedView, SwellStringArg, View, ViewContext, WeakView, Window,
};

use crate::application::{
    convert_factor_to_unit_value, convert_unit_value_to_factor, format_osc_feedback_args,
    get_bookmark_label, get_fx_label, get_fx_param_label, get_non_present_bookmark_label,
    get_optional_fx_label, get_route_label, parse_osc_feedback_args, Affected,
    AutomationModeOverrideType, BookmarkAnchorType, Change, CompartmentProp, ConcreteFxInstruction,
    ConcreteTrackInstruction, MappingChangeContext, MappingCommand, MappingModel, MappingProp,
    MidiSourceType, ModeCommand, ModeModel, ModeProp, RealearnAutomationMode, RealearnTrackArea,
    ReaperSourceType, Session, SessionProp, SharedMapping, SharedSession, SourceCategory,
    SourceCommand, SourceModel, SourceProp, TargetCategory, TargetCommand, TargetModel,
    TargetModelWithContext, TargetProp, TargetUnit, TrackRouteSelectorType,
    VirtualControlElementType, VirtualFxParameterType, VirtualFxType, VirtualTrackType,
    WeakSession,
};
use crate::base::Global;
use crate::base::{notification, when, Prop};
use crate::domain::clip::{ClipInfo, SlotContent};
use crate::domain::ui_util::parse_unit_value_from_percentage;
use crate::domain::{
    control_element_domains, AnyOnParameter, ControlContext, Exclusivity, FeedbackSendBehavior,
    ReaperTargetType, SendMidiDestination, SimpleExclusivity, WithControlContext, CLIP_SLOT_COUNT,
};
use crate::domain::{
    get_non_present_virtual_route_label, get_non_present_virtual_track_label,
    resolve_track_route_by_index, ActionInvocationType, CompoundMappingTarget,
    ExtendedProcessorContext, FeedbackResolution, FxDisplayType, MappingCompartment,
    QualifiedMappingId, RealearnTarget, ReaperTarget, SoloBehavior, TargetCharacter,
    TouchedParameterType, TrackExclusivity, TrackRouteType, TransportAction, VirtualControlElement,
    VirtualControlElementId, VirtualFx,
};
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{
    format_tags_as_csv, open_in_browser, parse_tags_from_csv, symbols,
};
use crate::infrastructure::ui::{
    EelEditorPanel, ItemProp, MainPanel, MappingHeaderPanel, YamlEditorPanel,
};

#[derive(Debug)]
pub struct MappingPanel {
    view: ViewContext,
    session: WeakSession,
    mapping: RefCell<Option<SharedMapping>>,
    main_panel: WeakView<MainPanel>,
    mapping_header_panel: SharedView<MappingHeaderPanel>,
    is_invoked_programmatically: Cell<bool>,
    window_cache: RefCell<Option<WindowCache>>,
    yaml_editor: RefCell<Option<SharedView<YamlEditorPanel>>>,
    eel_editor: RefCell<Option<SharedView<EelEditorPanel>>>,
    last_touched_mode_parameter: RefCell<Prop<Option<ModeParameter>>>,
    last_touched_source_character: RefCell<Prop<Option<DetailedSourceCharacter>>>,
    // Fires when a mapping is about to change or the panel is hidden.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
}

struct ImmutableMappingPanel<'a> {
    session: &'a Session,
    mapping: &'a MappingModel,
    source: &'a SourceModel,
    mode: &'a ModeModel,
    target: &'a TargetModel,
    view: &'a ViewContext,
    panel: &'a SharedView<MappingPanel>,
}

struct MutableMappingPanel<'a> {
    session: &'a mut Session,
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
    mode_min_jump: Window,
    mode_max_jump: Window,
    target_value: Window,
}

impl MappingPanel {
    pub fn new(session: WeakSession, main_panel: WeakView<MainPanel>) -> MappingPanel {
        MappingPanel {
            view: Default::default(),
            session: session.clone(),
            mapping: None.into(),
            main_panel,
            mapping_header_panel: SharedView::new(MappingHeaderPanel::new(
                session,
                Point::new(DialogUnits(7), DialogUnits(13)),
                None,
            )),
            is_invoked_programmatically: false.into(),
            window_cache: None.into(),
            yaml_editor: Default::default(),
            eel_editor: Default::default(),
            last_touched_mode_parameter: Default::default(),
            last_touched_source_character: Default::default(),
            party_is_over_subject: Default::default(),
        }
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
                // Mark as programmatical invocation.
                let panel_clone = self.clone();
                panel_clone.is_invoked_programmatically.set(true);
                scopeguard::defer! { panel_clone.is_invoked_programmatically.set(false); }
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
                                        view.invalidate_help();
                                    }
                                    One(p) => {
                                        use SourceProp as P;
                                        match p {
                                            P::Category | P::MidiSourceType | P::ControlElementType => {
                                                view.invalidate_source_controls();
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
                                            }
                                            P::Channel => {
                                                view.invalidate_source_control_visibilities();
                                                view.invalidate_source_line_3_combo_box_1();
                                                view.invalidate_source_line_5_combo_box();
                                            }
                                            P::MidiMessageNumber => {
                                                view.invalidate_source_line_4_combo_box();
                                            }
                                            P::ParameterNumberMessageNumber |
                                            P::OscArgIndex |
                                            P::ControlElementId => {
                                                view.invalidate_source_line_4_edit_control(initiator);
                                            }
                                            P::CustomCharacter |
                                            P::OscArgTypeTag => {
                                                view.invalidate_source_line_5_combo_box();
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
                                            }
                                            P::MidiClockTransportMessage => {
                                                view.invalidate_source_line_3_combo_box_2();
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
                                                view.invalidate_source_line_4_combo_box();
                                            }
                                            P::Line => {
                                                view.invalidate_source_line_5_combo_box();
                                            }
                                            P::OscAddressPattern |
                                            P::RawMidiPattern => {
                                                view.invalidate_source_line_3_edit_control(initiator);
                                            }
                                            P::MidiScript | P::OscFeedbackArgs => {
                                                view.invalidate_source_line_7_edit_control(initiator);
                                            }
                                            P::OscArgIsRelative => {
                                                view.invalidate_source_controls();
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
                                            }
                                            P::ReaperSourceType => {}
                                        }
                                    }
                                }
                                P::InMode(p) => match p {
                                    Multiple => {
                                        view.invalidate_mode_controls();
                                        view.invalidate_help();
                                    }
                                    One(p) => {
                                        use ModeProp as P;
                                        match p {
                                            P::AbsoluteMode => {
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
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
                                            P::PressDurationInterval | P::FireMode | P::TurboRate => {
                                                view.invalidate_mode_fire_controls(initiator);
                                            }
                                            P::JumpInterval => {
                                                view.invalidate_mode_min_jump_controls(initiator);
                                                view.invalidate_mode_max_jump_controls(initiator);
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
                                                view.invalidate_mode_eel_control_transformation_edit_control(initiator);
                                            }
                                            P::EelFeedbackTransformation | P::TextualFeedbackExpression => {
                                                view.invalidate_mode_eel_feedback_transformation_edit_control(initiator);
                                            }
                                            P::StepInterval => {
                                                view.invalidate_mode_step_controls(initiator);
                                            }
                                            P::Rotate => {
                                                view.invalidate_mode_rotate_check_box();
                                            }
                                            P::MakeAbsolute => {
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
                                            }
                                            P::GroupInteraction => {
                                                view.invalidate_mode_group_interaction_combo_box();
                                            }
                                            P::TargetValueSequence => {
                                                view.invalidate_mode_target_value_sequence_edit_control(initiator);
                                            }
                                            P::FeedbackType => {
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
                                            }
                                            P::FeedbackColor | P::FeedbackBackgroundColor => {
                                                view.invalidate_mode_feedback_type_button();
                                            }
                                        }
                                    }
                                },

                                P::InTarget(p) => match p {
                                    Multiple => {
                                        view.invalidate_target_controls(None);
                                    }
                                    One(p) => {
                                        use TargetProp as P;
                                        match p {
                                            P::Category | P::TargetType | P::ControlElementType => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_controls(None);
                                                view.invalidate_mode_controls();
                                                view.invalidate_help();
                                            }
                                            P::TrackType | P::TrackIndex | P::TrackId | P::TrackName
                                            | P::TrackExpression | P::BookmarkType | P::BookmarkAnchorType
                                            | P::BookmarkRef | P::TransportAction | P::AnyOnParameter
                                            | P::Action => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_controls(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::ControlElementId => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_line_2(initiator);
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
                                            }
                                            P::ActionInvocationType => {
                                                view.invalidate_target_line_3(None);
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::SoloBehavior | P::TouchedParameterType | P::AutomationMode | P::TrackArea | P::SlotIndex => {
                                                view.invalidate_target_line_3(None);
                                            }
                                            P::AutomationModeOverrideType => {
                                                view.invalidate_window_title();
                                                view.invalidate_target_line_2_combo_box_2(initiator);
                                                view.invalidate_target_line_3(None);
                                            }
                                            P::FxSnapshot | P::FxDisplayType  => {
                                                view.invalidate_target_line_4(None);
                                                view.invalidate_target_value_controls();
                                            }
                                            P::TrackExclusivity => {
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
                                                view.invalidate_target_value_controls();
                                                view.invalidate_mode_controls();
                                            }
                                            P::ScrollArrangeView | P::SeekPlay => {
                                                view.invalidate_target_check_boxes();
                                                view.invalidate_target_value_controls();
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
                                            P::UseRegions  | P::NextBar => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::UseLoopPoints | P::Buffered | P::PollForFeedback => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::UseTimeSelection => {
                                                view.invalidate_target_check_boxes();
                                            }
                                            P::FeedbackResolution => {
                                                view.invalidate_target_line_2_combo_box_1();
                                            }
                                            P::RawMidiPattern | P::OscAddressPattern => {
                                                view.invalidate_target_line_3(initiator);
                                                view.invalidate_mode_controls();
                                            }
                                            P::SendMidiDestination | P::OscDevId => {
                                                view.invalidate_target_line_2(None);
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
        let mapping = self.mapping();
        session
            .borrow_mut()
            .toggle_learning_source(&session, &mapping);
    }

    fn toggle_learn_target(&self) {
        let session = self.session();
        session
            .borrow_mut()
            .toggle_learning_target(&session, self.qualified_mapping_id().expect("no mapping"));
    }

    fn handle_target_line_2_button_press(self: SharedView<Self>) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let category = mapping.borrow().target_model.category();
        match category {
            TargetCategory::Reaper => {
                self.write(|p| p.handle_target_line_2_button_press());
            }
            TargetCategory::Virtual => {
                let control_element_type = mapping.borrow().target_model.control_element_type();
                let window = self.view.require_window();
                let text = prompt_for_predefined_control_element_name(
                    window,
                    control_element_type,
                    &HashMap::new(),
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

    fn handle_target_line_3_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let target_type = mapping.borrow().target_model.target_type();
        match target_type {
            ReaperTargetType::SendMidi => {
                if let Some(preset) =
                    prompt_for_predefined_raw_midi_pattern(self.view.require_window())
                {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetRawMidiPattern(preset),
                    ));
                }
            }
            t if t.supports_slot() => {
                if let Some(action) = self.prompt_for_slot_action() {
                    self.invoke_slot_menu_action(action)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn prompt_for_slot_action(&self) -> Option<SlotMenuAction> {
        let menu_bar = MenuBar::new_popup_menu();
        let pure_menu = {
            use swell_ui::menu_tree::*;
            let session = self.session();
            let session = session.borrow();
            let entries = vec![
                item("Show slot info", || SlotMenuAction::ShowSlotInfo),
                item_with_opts(
                    "Fill with selected item source",
                    ItemOpts {
                        enabled: session
                            .context()
                            .project_or_current_project()
                            .first_selected_item()
                            .is_some(),
                        checked: false,
                    },
                    || SlotMenuAction::FillWithItemSource,
                ),
            ];
            let mut root_menu = root_menu(entries);
            root_menu.index(1);
            fill_menu(menu_bar.menu(), &root_menu);
            root_menu
        };
        let result_index = self
            .view
            .require_window()
            .open_popup_menu(menu_bar.menu(), Window::cursor_pos())?;
        let item = pure_menu.find_item_by_id(result_index)?;
        Some(item.invoke_handler())
    }

    fn invoke_slot_menu_action(&self, action: SlotMenuAction) -> Result<(), &'static str> {
        match action {
            SlotMenuAction::ShowSlotInfo => {
                struct SlotInfo {
                    file_name: String,
                    clip_info: Option<ClipInfo>,
                }
                let info = {
                    let instance_state = self.session().borrow().instance_state().clone();
                    let instance_state = instance_state.borrow();
                    let mapping = self.mapping();
                    let mapping = mapping.borrow();
                    let slot_index = mapping.target_model.slot_index();
                    if let Ok(slot) = instance_state.get_slot(slot_index) {
                        if let Some(content) = &slot.descriptor().content {
                            let info = SlotInfo {
                                file_name: content
                                    .file()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                clip_info: slot.clip_info(),
                            };
                            Some(info)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                let msg = if let Some(info) = info {
                    let suffix = if let Some(clip_info) = info.clip_info {
                        format!(
                            "Type: {}\n\nLength: {}",
                            clip_info.r#type,
                            clip_info
                                .length
                                .map(|l| format!("{} secs", l))
                                .unwrap_or_default()
                        )
                    } else {
                        "<offline>".to_owned()
                    };
                    format!("Source: {}\n\n{}", info.file_name, suffix)
                } else {
                    "Slot is empty".to_owned()
                };
                self.view.require_window().alert("ReaLearn", msg);
                Ok(())
            }
            SlotMenuAction::FillWithItemSource => {
                let result = {
                    let session = self.session();
                    let session = session.borrow();
                    let item = session
                        .context()
                        .project_or_current_project()
                        .first_selected_item()
                        .ok_or("no item selected")?;
                    let slot_index = self.mapping().borrow().target_model.slot_index();
                    let mut instance_state = session.instance_state().borrow_mut();
                    instance_state.fill_slot_with_item_source(slot_index, item)
                };
                if let Err(e) = result {
                    self.view.require_window().alert("ReaLearn", e.to_string());
                }
                Ok(())
            }
        }
    }

    fn handle_source_line_4_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let control_element_type = mapping.borrow().source_model.control_element_type();
        let window = self.view.require_window();
        let controller_mappings: Vec<_> = {
            let session = self.session();
            let session = session.borrow();
            session
                .mappings(MappingCompartment::ControllerMappings)
                .cloned()
                .collect()
        };
        let grouped_mappings =
            group_mappings_by_virtual_control_element(controller_mappings.iter());
        let text = prompt_for_predefined_control_element_name(
            window,
            control_element_type,
            &grouped_mappings,
        )
        .ok_or("nothing picked")?;
        let control_element_id = text.parse().unwrap_or_default();
        self.change_mapping(MappingCommand::ChangeSource(
            SourceCommand::SetControlElementId(control_element_id),
        ));
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
        let instruction = prompt_for_color(
            self.view.require_window(),
            current_color,
            current_background_color,
        )?;
        let cmd = match instruction.target {
            ColorTarget::Color => ModeCommand::SetFeedbackColor(instruction.color),
            ColorTarget::BackgroundColor => {
                ModeCommand::SetFeedbackBackgroundColor(instruction.color)
            }
        };
        self.change_mapping(MappingCommand::ChangeMode(cmd));
        Ok(())
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

    fn handle_target_line_4_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let target_type = mapping.borrow().target_model.target_type();
        match target_type {
            ReaperTargetType::Action => {
                let reaper = Reaper::get().medium_reaper();
                use InitialAction::*;
                let initial_action = match mapping.borrow().target_model.action() {
                    None => NoneSelected,
                    Some(a) => Selected(a.command_id()),
                };
                // TODO-low Add this to reaper-high with rxRust
                if reaper.low().pointers().PromptForAction.is_none() {
                    self.view.require_window().alert(
                        "ReaLearn",
                        "Please update to REAPER >= 6.12 in order to pick actions!",
                    );
                    return Ok(());
                }
                reaper.prompt_for_action_create(initial_action, SectionId::new(0));
                let shared_mapping = self.mapping();
                let weak_session = self.session.clone();
                Global::control_surface_rx()
                    .main_thread_idle()
                    .take_until(self.party_is_over())
                    .map(|_| {
                        Reaper::get()
                            .medium_reaper()
                            .prompt_for_action_poll(SectionId::new(0))
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
                                let cmd = MappingCommand::ChangeTarget(TargetCommand::SetAction(
                                    Some(action),
                                ));
                                session.borrow_mut().change_mapping_from_ui_expert(
                                    &mut mapping,
                                    cmd,
                                    None,
                                    weak_session.clone(),
                                );
                            }
                        },
                        || {
                            Reaper::get()
                                .medium_reaper()
                                .prompt_for_action_finish(SectionId::new(0));
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
            self.main_panel
                .upgrade()
                .expect("main view gone")
                .force_scroll_to_mapping(id.id);
        }
    }

    fn edit_midi_source_script(&self) {
        let session = self.session.clone();
        self.edit_eel(
            |m| m.source_model.midi_script().to_owned(),
            move |m, eel| {
                Session::change_mapping_from_ui_simple(
                    session.clone(),
                    m,
                    MappingCommand::ChangeSource(SourceCommand::SetMidiScript(eel)),
                    None,
                );
            },
        );
    }

    fn edit_eel(
        &self,
        get_initial_value: impl Fn(&MappingModel) -> String,
        apply: impl Fn(&mut MappingModel, String) + 'static,
    ) {
        let mapping = self.mapping();
        let weak_mapping = Rc::downgrade(&mapping);
        let initial_value = { get_initial_value(&mapping.borrow()) };
        let editor = EelEditorPanel::new(initial_value, move |edited_script| {
            let m = match weak_mapping.upgrade() {
                None => return,
                Some(m) => m,
            };
            apply(&mut m.borrow_mut(), edited_script);
        });
        let editor = SharedView::new(editor);
        let editor_clone = editor.clone();
        if let Some(existing_editor) = self.eel_editor.replace(Some(editor)) {
            existing_editor.close();
        };
        editor_clone.open(self.view.require_window());
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
                    "Your changes have been applied and saved but they contain the following error and therefore won't have any effect:\n\n{}",
                    e
                ));
            };
        });
        let editor = SharedView::new(editor);
        let editor_clone = editor.clone();
        if let Some(existing_editor) = self.yaml_editor.replace(Some(editor)) {
            existing_editor.close();
        };
        editor_clone.open(self.view.require_window());
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

    pub fn notify_parameters_changed(
        self: SharedView<Self>,
        session: &Session,
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
        if let Some(p) = self.yaml_editor.replace(None) {
            p.close();
        }
        if let Some(p) = self.eel_editor.replace(None) {
            p.close();
        }
        self.mapping_header_panel.clear_item();
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
    fn invoke_programmatically(&self, f: impl FnOnce()) {
        self.is_invoked_programmatically.set(true);
        scopeguard::defer! { self.is_invoked_programmatically.set(false); }
        f();
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

    fn session(&self) -> SharedSession {
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
            mode_min_jump: view.require_control(root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL),
            mode_max_jump: view.require_control(root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL),
            target_value: view.require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL),
        };
        self.window_cache.replace(Some(sliders));
        let indicator = self
            .view
            .require_control(root::IDC_MAPPING_MATCHED_INDICATOR_TEXT);
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
            root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_jump_from_edit_control());
            }
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_jump_from_edit_control());
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

fn decorate_reaction<I: Send + Sync + Clone + 'static>(
    reaction: impl Fn(&ImmutableMappingPanel, I) + 'static + Copy,
) -> impl Fn(Rc<MappingPanel>, I) + Copy {
    move |view, item| {
        let view_mirror = view.clone();
        view_mirror.is_invoked_programmatically.set(true);
        scopeguard::defer! { view_mirror.is_invoked_programmatically.set(false); }
        // If the reaction can't be displayed anymore because the mapping is not filled anymore,
        // so what.
        let _ = view.read(move |p| reaction(p, item.clone()));
    }
}

impl<'a> MutableMappingPanel<'a> {
    fn resolved_targets(&self) -> Vec<CompoundMappingTarget> {
        self.target_with_context().resolve().unwrap_or_default()
    }

    fn first_resolved_target(&self) -> Option<CompoundMappingTarget> {
        self.resolved_targets().into_iter().next()
    }

    fn open_target(&self) {
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

    #[allow(clippy::single_match)]
    fn handle_target_line_2_button_press(&mut self) {
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let project = self.session.context().project_or_current_project();
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

    fn update_mode_hint(&self, mode_parameter: ModeParameter) {
        self.panel
            .last_touched_mode_parameter
            .borrow_mut()
            .set(Some(mode_parameter));
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
            Reaper | Virtual | Never => {}
        };
    }

    #[allow(clippy::single_match)]
    fn handle_source_line_3_combo_box_1_change(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                let value = match b.selected_combo_box_item_data() {
                    -1 => None,
                    id => Some(Channel::new(id as _)),
                };
                self.change_mapping(MappingCommand::ChangeSource(SourceCommand::SetChannel(
                    value,
                )));
            }
            _ => {}
        };
    }

    #[allow(clippy::single_match)]
    fn handle_source_line_4_combo_box_change(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => {
                use MidiSourceType::*;
                match self.mapping.source_model.midi_source_type() {
                    Display => {
                        use DisplayType::*;
                        let value = match self.mapping.source_model.display_type() {
                            MackieLcd | SiniConE24 => match b.selected_combo_box_item_data() {
                                -1 => None,
                                id => Some(id as u8),
                            },
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
            Osc => {
                let i = b.selected_combo_box_item_index();
                let tag = i.try_into().expect("invalid OSC type tag");
                self.change_mapping(MappingCommand::ChangeSource(
                    SourceCommand::SetOscArgTypeTag(tag),
                ));
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
                    SourceCommand::SetControlElementType(element_type),
                ));
            }
            _ => {}
        };
    }

    #[allow(clippy::single_match)]
    fn handle_source_line_3_combo_box_2_change(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX);
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
                _ => {}
            },
            _ => {}
        }
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
            Osc => {
                let value = parse_osc_arg_index(&text);
                self.change_mapping_with_initiator(
                    MappingCommand::ChangeSource(SourceCommand::SetOscArgIndex(value)),
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
            Reaper | Never => {}
        };
    }

    #[allow(clippy::single_match)]
    fn handle_source_line_3_edit_control_change(&mut self) {
        let edit_control_id = root::ID_SOURCE_LINE_3_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        if let Ok(value) = c.text() {
            use SourceCategory::*;
            match self.mapping.source_model.category() {
                Midi => match self.mapping.source_model.midi_source_type() {
                    MidiSourceType::Raw => {
                        self.change_mapping_with_initiator(
                            MappingCommand::ChangeSource(SourceCommand::SetRawMidiPattern(value)),
                            Some(edit_control_id),
                        );
                    }
                    _ => {}
                },
                Osc => {
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeSource(SourceCommand::SetOscAddressPattern(value)),
                        Some(edit_control_id),
                    );
                }
                Reaper | Virtual | Never => {}
            }
        }
    }

    #[allow(clippy::single_match)]
    fn handle_source_line_7_edit_control_change(&mut self) {
        let edit_control_id = root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        let value = c.text().unwrap_or_default();
        use SourceCategory::*;
        match self.mapping.source_model.category() {
            Midi => match self.mapping.source_model.midi_source_type() {
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
            _ => {}
        }
    }

    fn update_mode_rotate(&mut self) {
        self.update_mode_hint(ModeParameter::Rotate);
        let checked = self
            .view
            .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetRotate(checked)));
    }

    fn update_mode_feedback_type(&mut self) {
        self.update_mode_hint(ModeParameter::FeedbackType);
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
        self.update_mode_hint(ModeParameter::MakeAbsolute);
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
        self.update_mode_hint(ModeParameter::SpecificOutOfRangeBehavior(behavior));
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
        self.update_mode_hint(ModeParameter::SpecificGroupInteraction(interaction));
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
        self.update_mode_hint(ModeParameter::SpecificFireMode(mode));
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetFireMode(mode)));
    }

    fn update_mode_round_target_value(&mut self) {
        self.update_mode_hint(ModeParameter::RoundTargetValue);
        let checked = self
            .view
            .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::ChangeMode(
            ModeCommand::SetRoundTargetValue(checked),
        ));
    }

    fn update_takeover_mode(&mut self) {
        self.update_mode_hint(ModeParameter::TakeoverMode);
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
        self.update_mode_hint(ModeParameter::ButtonFilter);
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
        self.update_mode_hint(ModeParameter::RelativeFilter);
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
        self.update_mode_hint(ModeParameter::Reverse);
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
        self.update_mode_hint(ModeParameter::SpecificAbsoluteMode(mode));
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

    fn update_mode_min_jump_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL;
        let value = self
            .get_step_size_from_target_edit_control(control_id)
            .unwrap_or(UnitValue::MIN);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMinJump(value)),
            Some(control_id),
        );
    }

    fn update_mode_max_jump_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL;
        let value = self
            .get_step_size_from_target_edit_control(control_id)
            .unwrap_or(UnitValue::MAX);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMaxJump(value)),
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
        let control_id = root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL;
        let value = self
            .get_value_from_step_edit_control(control_id)
            .unwrap_or_else(|| UnitValue::MIN.to_symmetric());
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMinStep(value)),
            Some(control_id),
        );
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

    fn get_value_from_step_edit_control(
        &self,
        edit_control_id: u32,
    ) -> Option<SoftSymmetricUnitValue> {
        if self.mapping_uses_step_counts() {
            let text = self.view.require_control(edit_control_id).text().ok()?;
            Some(convert_factor_to_unit_value(text.parse().ok()?))
        } else {
            self.get_step_size_from_target_edit_control(edit_control_id)
                .map(|v| v.to_symmetric())
        }
    }

    fn update_mode_max_step_from_edit_control(&mut self) {
        let control_id = root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL;
        let value = self
            .get_value_from_step_edit_control(control_id)
            .unwrap_or(SoftSymmetricUnitValue::SOFT_MAX);
        self.change_mapping_with_initiator(
            MappingCommand::ChangeMode(ModeCommand::SetMaxStep(value)),
            Some(control_id),
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
        self.update_mode_hint(ModeParameter::TargetValueSequence);
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
        self.update_mode_hint(ModeParameter::ControlTransformation);
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
        let mode_parameter = if self.mapping.mode_model.feedback_type().is_textual() {
            ModeParameter::TextualFeedbackExpression
        } else {
            ModeParameter::FeedbackTransformation
        };
        self.update_mode_hint(mode_parameter);
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
        self.update_mode_hint(ModeParameter::TargetMinMax);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMinTargetValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_max_target_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::TargetMinMax);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMaxTargetValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_min_source_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::SourceMinMax);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMinSourceValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_max_source_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::SourceMinMax);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMaxSourceValue(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_min_step_from_slider(&mut self, slider: Window) {
        let step_counts = self.mapping_uses_step_counts();
        let (mode_param, value) = if step_counts {
            (
                ModeParameter::SpeedMin,
                slider.slider_symmetric_unit_value(),
            )
        } else {
            (
                ModeParameter::StepSizeMin,
                slider.slider_unit_value().to_symmetric(),
            )
        };
        self.update_mode_hint(mode_param);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMinStep(value)));
    }

    fn update_mode_max_step_from_slider(&mut self, slider: Window) {
        let step_counts = self.mapping_uses_step_counts();
        let (mode_param, value) = if step_counts {
            (
                ModeParameter::SpeedMax,
                slider.slider_symmetric_unit_value(),
            )
        } else {
            (
                ModeParameter::StepSizeMax,
                slider.slider_unit_value().to_symmetric(),
            )
        };
        self.update_mode_hint(mode_param);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMaxStep(value)));
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
            FireMode::WhenButtonReleased | FireMode::OnSinglePress | FireMode::OnDoublePress => {
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

    fn mapping_uses_step_counts(&self) -> bool {
        self.mapping
            .with_context(self.session.extended_context())
            .uses_step_counts()
    }

    fn update_mode_min_jump_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::JumpMinMax);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMinJump(
            slider.slider_unit_value(),
        )));
    }

    fn update_mode_max_jump_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::JumpMinMax);
        self.change_mapping(MappingCommand::ChangeMode(ModeCommand::SetMaxJump(
            slider.slider_unit_value(),
        )));
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
                t if t.supports_fx_chain() => {
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
            TargetCategory::Virtual => {}
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
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    #[allow(clippy::single_match)]
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
                ReaperTargetType::ClipTransport => {
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetNextBar(
                        is_checked,
                    )));
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

    #[allow(clippy::single_match)]
    fn handle_target_check_box_5_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_5)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Seek | ReaperTargetType::GoToBookmark => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetUseLoopPoints(is_checked),
                    ));
                }
                ReaperTargetType::ClipTransport => {
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetBuffered(
                        is_checked,
                    )));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    #[allow(clippy::single_match)]
    fn handle_target_check_box_6_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_6)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Seek | ReaperTargetType::GoToBookmark => {
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetUseTimeSelection(is_checked),
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

    fn update_target_type(&mut self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        use TargetCategory::*;
        match self.mapping.target_model.category() {
            Reaper => {
                let data = b.selected_combo_box_item_data() as usize;
                let v = data.try_into().expect("invalid REAPER target type");
                self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetTargetType(
                    v,
                )));
            }
            Virtual => {
                let v = b
                    .selected_combo_box_item_index()
                    .try_into()
                    .expect("invalid virtual target type");
                self.change_mapping(MappingCommand::ChangeTarget(
                    TargetCommand::SetControlElementType(v),
                ));
            }
        };
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
                t if t.supports_slot() => {
                    let slot_index = combo.selected_combo_box_item_index();
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetSlotIndex(
                        slot_index,
                    )));
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
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid OSC type tag");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetOscArgTypeTag(v),
                    ));
                }
                ReaperTargetType::FxParameter => {
                    let param_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(TargetCommand::SetParamType(
                        param_type,
                    )));
                }
                ReaperTargetType::NavigateWithinGroup => {
                    let exclusivity: SimpleExclusivity = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetExclusivity(exclusivity.into()),
                    ));
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
                    let value: u32 = match self.mapping.target_model.bookmark_anchor_type() {
                        BookmarkAnchorType::Id => combo.selected_combo_box_item_data() as _,
                        BookmarkAnchorType::Index => combo.selected_combo_box_item_index() as _,
                    };
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
                ReaperTargetType::NavigateWithinGroup => {
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
                        TargetCommand::SetSendMidiDestination(v),
                    ));
                }
                ReaperTargetType::SendOsc => {
                    let dev_id = match combo.selected_combo_box_item_data() {
                        -1 => None,
                        i if i >= 0 => App::get()
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
                _ if self.mapping.target_model.supports_track() => {
                    let project = self.session.context().project_or_current_project();
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
                            self.mapping.target_model.set_concrete_fx(
                                ConcreteFxInstruction::ByIdWithFx(fx),
                                false,
                                true,
                            );
                        }
                    }
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
                | ReaperTargetType::TrackSendAutomationMode => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid automation mode");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetAutomationMode(v),
                    ));
                }
                ReaperTargetType::AutomationTouchState => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid touched parameter type");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTouchedParameterType(v),
                    ));
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
                ReaperTargetType::ClipTransport => {
                    let i = combo.selected_combo_box_item_index();
                    let v = i.try_into().expect("invalid transport action");
                    self.change_mapping(MappingCommand::ChangeTarget(
                        TargetCommand::SetTransportAction(v),
                    ));
                }
                ReaperTargetType::FxParameter => {
                    if let Ok(fx) = self.target_with_context().first_fx() {
                        let i = combo.selected_combo_box_item_index();
                        let param = fx.parameter_by_index(i as _);
                        self.change_mapping(MappingCommand::ChangeTarget(
                            TargetCommand::SetParamIndex(i as _),
                        ));
                        // We also set name so that we can easily switch between types.
                        // Parameter names are not reliably UTF-8-encoded (e.g. "JS: Stereo Width")
                        let param_name = param.name().into_inner().to_string_lossy().to_string();
                        self.change_mapping(MappingCommand::ChangeTarget(
                            TargetCommand::SetParamName(param_name),
                        ));
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

    fn handle_applicable_to_combo_box_change(&mut self) {
        let data = self
            .view
            .require_control(root::ID_MAPPING_HELP_APPLICABLE_TO_COMBO_BOX)
            .selected_combo_box_item_data()
            .try_into()
            .ok();
        self.panel
            .last_touched_source_character
            .borrow_mut()
            .set(data);
    }

    fn handle_target_line_2_edit_control_change(&mut self) {
        let edit_control_id = root::ID_TARGET_LINE_2_EDIT_CONTROL;
        let control = self.view.require_control(edit_control_id);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                _ if self.mapping.target_model.supports_track() => {
                    match self.mapping.target_model.track_type() {
                        VirtualTrackType::Dynamic => {
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
                        VirtualTrackType::ByIndex => {
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
                ReaperTargetType::SendMidi => {
                    let text = control.text().unwrap_or_default();
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetRawMidiPattern(text)),
                        Some(edit_control_id),
                    );
                }
                ReaperTargetType::SendOsc => {
                    let pattern = control.text().unwrap_or_default();
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetOscAddressPattern(pattern)),
                        Some(edit_control_id),
                    );
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
                ReaperTargetType::SendOsc => {
                    let text = control.text().unwrap_or_default();
                    let v = parse_osc_arg_index(&text);
                    self.change_mapping_with_initiator(
                        MappingCommand::ChangeTarget(TargetCommand::SetOscArgIndex(v)),
                        Some(edit_control_id),
                    );
                }
                ReaperTargetType::FxParameter => match self.mapping.target_model.param_type() {
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

impl<'a> ImmutableMappingPanel<'a> {
    fn hit_target(&self, value: UnitValue) {
        self.session.hit_target(
            self.mapping.qualified_id(),
            AbsoluteValue::Continuous(value),
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
        self.invalidate_mapping_enabled_check_box();
        self.invalidate_mapping_feedback_send_behavior_combo_box();
        self.invalidate_mapping_visible_in_projection_check_box();
        self.invalidate_mapping_advanced_settings_button();
        self.invalidate_source_controls();
        self.invalidate_target_controls(None);
        self.invalidate_mode_controls();
    }

    fn invalidate_help(&self) {
        let applicable_to_label = self
            .view
            .require_control(root::ID_MAPPING_HELP_APPLICABLE_TO_LABEL);
        let applicable_to_combo = self
            .view
            .require_control(root::ID_MAPPING_HELP_APPLICABLE_TO_COMBO_BOX);
        let success = if let Some(mode_parameter) =
            self.panel.last_touched_mode_parameter.borrow().get()
        {
            let relevant_source_characters: Vec<_> = self
                .mapping
                .source_model
                .possible_detailed_characters()
                .into_iter()
                .filter(|character| {
                    let (ch, fh) = self.get_control_and_feedback_hint(*character, mode_parameter);
                    ch.is_some() || fh.is_some()
                })
                .collect();
            if let Some(first_character) = relevant_source_characters.first().copied() {
                applicable_to_label.show();
                applicable_to_combo.show();
                applicable_to_combo.fill_combo_box_with_data_small(
                    relevant_source_characters
                        .into_iter()
                        .map(|ch| (ch.into(), ch)),
                );
                self.panel
                    .last_touched_source_character
                    .borrow_mut()
                    .set(Some(first_character));
                self.invalidate_help_from_source_character();
                true
            } else {
                false
            }
        } else {
            false
        };
        if !success {
            self.clear_help();
        }
    }

    fn invalidate_help_from_source_character(&self) {
        let success = if let Some(source_character) =
            self.panel.last_touched_source_character.borrow().get()
        {
            if self
                .view
                .require_control(root::ID_MAPPING_HELP_APPLICABLE_TO_COMBO_BOX)
                .select_combo_box_item_by_data(source_character.into())
                .is_err()
            {
                false
            } else if let Some(mode_parameter) =
                self.panel.last_touched_mode_parameter.borrow().get()
            {
                let (control_hint, feedback_hint) =
                    self.get_control_and_feedback_hint(source_character, mode_parameter);
                let mut content = String::new();
                if let Some(hint) = control_hint {
                    content.push_str("- Control: ");
                    content.push_str(hint);
                    content.push('\n');
                }
                if let Some(hint) = feedback_hint {
                    content.push_str("- Feedback: ");
                    content.push_str(hint);
                    content.push('\n');
                }
                let subject = format!("Help: {}", mode_parameter.to_string());
                self.view
                    .require_control(root::ID_MAPPING_HELP_SUBJECT_LABEL)
                    .set_text(subject);
                self.view
                    .require_control(root::ID_MAPPING_HELP_CONTENT_LABEL)
                    .set_multi_line_text(content);
                true
            } else {
                false
            }
        } else {
            false
        };
        if !success {
            self.clear_help();
        }
    }

    fn get_control_and_feedback_hint(
        &self,
        source_character: DetailedSourceCharacter,
        mode_parameter: ModeParameter,
    ) -> (Option<&str>, Option<&str>) {
        let base_input = ModeApplicabilityCheckInput {
            target_is_virtual: self.mapping.target_model.is_virtual(),
            // TODO-high-discrete Set correctly
            target_supports_discrete_values: false,
            is_feedback: false,
            make_absolute: self.mapping.mode_model.make_absolute(),
            use_textual_feedback: self.mapping.mode_model.feedback_type().is_textual(),
            source_character,
            absolute_mode: self.mapping.mode_model.absolute_mode(),
            mode_parameter,
            target_value_sequence_is_set: !self
                .mapping
                .mode_model
                .target_value_sequence()
                .is_empty(),
        };
        let control = ModeApplicabilityCheckInput {
            is_feedback: false,
            ..base_input
        };
        let feedback = ModeApplicabilityCheckInput {
            is_feedback: true,
            ..base_input
        };
        (
            check_mode_applicability(control).hint(),
            check_mode_applicability(feedback).hint(),
        )
    }

    fn clear_help(&self) {
        self.view
            .require_control(root::ID_MAPPING_HELP_APPLICABLE_TO_LABEL)
            .hide();
        self.view
            .require_control(root::ID_MAPPING_HELP_APPLICABLE_TO_COMBO_BOX)
            .hide();
        self.view
            .require_control(root::ID_MAPPING_HELP_SUBJECT_LABEL)
            .set_text("Help");
        self.view
            .require_control(root::ID_MAPPING_HELP_CONTENT_LABEL)
            .set_text("");
    }

    fn invalidate_window_title(&self) {
        self.view
            .require_window()
            .set_text(format!("Mapping \"{}\"", self.mapping.effective_name()));
    }

    fn invalidate_mapping_feedback_send_behavior_combo_box(&self) {
        let combo = self
            .view
            .require_control(root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX);
        combo
            .select_combo_box_item_by_index(self.mapping.feedback_send_behavior().into())
            .unwrap();
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
        let text = format!("Advanced settings {}", suffix);
        cb.set_text(text);
    }

    fn invalidate_source_controls(&self) {
        self.invalidate_source_control_visibilities();
        self.invalidate_source_learn_button();
        self.invalidate_source_category_combo_box();
        self.invalidate_source_line_2();
        self.invalidate_source_line_3(None);
        self.invalidate_source_line_4(None);
        self.invalidate_source_line_5();
        self.invalidate_source_check_box_2();
        self.invalidate_source_line_7(None);
    }

    fn invalidate_source_control_visibilities(&self) {
        let source = self.source;
        // Show/hide stuff
        self.show_if(
            source.supports_type(),
            &[
                root::ID_SOURCE_TYPE_LABEL_TEXT,
                root::ID_SOURCE_TYPE_COMBO_BOX,
            ],
        );
    }

    fn show_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            self.view.require_control(*id).set_visible(condition);
        }
    }

    fn enable_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            self.view.require_control(*id).set_visible(condition);
        }
    }

    fn invalidate_source_category_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.source.category().into())
            .unwrap();
    }

    fn invalidate_target_category_combo_box(&self) {
        // Don't allow main mappings to have virtual target
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .set_enabled(self.mapping.compartment() != MappingCompartment::MainMappings);
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.target.category().into())
            .unwrap();
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
            Virtual => self.source.control_element_type().into(),
            _ => return,
        };
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        b.select_combo_box_item_by_index(item_index).unwrap();
    }

    fn invalidate_source_learn_button(&self) {
        self.invalidate_learn_button(
            self.session
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
        self.invalidate_source_line_3_combo_box_1();
        self.invalidate_source_line_3_combo_box_2();
        self.invalidate_source_line_3_edit_control(initiator);
    }

    #[allow(clippy::single_match)]
    fn invalidate_source_line_3_label_1(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::Raw => Some("Pattern"),
                t if t.supports_channel() => Some("Channel"),
                _ => None,
            },
            Osc => Some("Address"),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_CHANNEL_LABEL)
            .set_text_or_hide(text);
    }

    #[allow(clippy::single_match)]
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

    #[allow(clippy::single_match)]
    fn invalidate_source_line_3_combo_box_1(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi if self.source.supports_channel() => {
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
        self.invalidate_source_line_4_combo_box();
        self.invalidate_source_line_4_edit_control(initiator);
    }

    fn invalidate_source_line_4_button(&self) {
        use SourceCategory::*;
        let text = match self.source.category() {
            Virtual => Some("Pick"),
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
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT)
            .set_text_or_hide(text);
    }

    fn invalidate_source_line_4_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => {
                use MidiSourceType::*;
                match self.source.midi_source_type() {
                    Display => {
                        use DisplayType::*;
                        match self.source.display_type() {
                            MackieLcd | SiniConE24 => {
                                b.show();
                                let display_count = self.source.display_count() as isize;
                                b.fill_combo_box_with_data_vec(
                                    iter::once((-1isize, "<All>".to_string()))
                                        .chain(
                                            (0..display_count)
                                                .map(|i| (i as isize, (i + 1).to_string())),
                                        )
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
                                b.fill_combo_box_indexed(
                                    MackieSevenSegmentDisplayScope::into_enum_iter(),
                                );
                                b.select_combo_box_item_by_index(
                                    self.source.mackie_7_segment_display_scope().into(),
                                )
                                .unwrap();
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
            Osc => Some(format_osc_arg_index(self.source.osc_arg_index())),
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
            Midi if self.source.is_midi_script() => Some("..."),
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
                MidiSourceType::Script => Some("Script"),
                _ => None,
            },
            Osc => Some("Feedback arguments"),
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
        let value_text = match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::Raw => Some(self.source.raw_midi_pattern()),
                _ => None,
            },
            Osc => Some(self.source.osc_address_pattern()),
            _ => None,
        };
        c.set_text_or_hide(value_text);
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
                MidiSourceType::Script => {
                    let midi_script = self.source.midi_script();
                    (
                        Some(midi_script.lines().next().unwrap_or_default().to_owned()),
                        midi_script.lines().count() > 1,
                    )
                }
                _ => (None, false),
            },
            Osc => {
                let text = format_osc_feedback_args(self.source.osc_feedback_args());
                (Some(text), false)
            }
            _ => (None, false),
        };
        c.set_text_or_hide(value_text);
        c.set_enabled(!read_only);
    }

    fn invalidate_source_line_5(&self) {
        self.invalidate_source_line_5_label();
        self.invalidate_source_line_5_combo_box();
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
            Osc => Some("Type"),
            _ => None,
        };
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_LABEL_TEXT)
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
                        b.fill_combo_box_indexed(SourceCharacter::into_enum_iter());
                        b.select_combo_box_item_by_index(self.source.custom_character().into())
                            .unwrap();
                    }
                    _ => {
                        b.hide();
                    }
                }
            }
            Osc => {
                b.show();
                b.fill_combo_box_indexed(OscTypeTag::into_enum_iter());
                b.select_combo_box_item_by_index(self.source.osc_arg_type_tag().into())
                    .unwrap();
            }
            _ => {
                b.hide();
            }
        };
    }

    fn invalidate_source_line_3_combo_box_2(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => match self.source.midi_source_type() {
                MidiSourceType::ClockTransport => {
                    b.show();
                    b.fill_combo_box_indexed(MidiClockTransportMessage::into_enum_iter());
                    b.select_combo_box_item_by_index(
                        self.source.midi_clock_transport_message().into(),
                    )
                    .unwrap();
                }
                MidiSourceType::Display => {
                    b.show();
                    b.fill_combo_box_indexed(DisplayType::into_enum_iter());
                    b.select_combo_box_item_by_index(self.source.display_type().into())
                        .unwrap();
                }
                _ => {
                    b.hide();
                }
            },
            _ => {
                b.hide();
            }
        }
    }

    fn invalidate_target_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_category_combo_box();
        self.invalidate_target_type_combo_box();
        self.invalidate_target_line_2(initiator);
        self.invalidate_target_line_3(initiator);
        self.invalidate_target_line_4(initiator);
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

    fn invalidate_target_type_combo_box(&self) {
        self.fill_target_type_combo_box();
        self.invalidate_target_type_combo_box_value();
    }

    fn invalidate_target_type_combo_box_value(&self) {
        let combo = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        use TargetCategory::*;
        let hint = match self.target.category() {
            Reaper => {
                let item_data: usize = self.target.target_type().into();
                combo
                    .select_combo_box_item_by_data(item_data as isize)
                    .unwrap();
                self.target.target_type().hint()
            }
            Virtual => {
                let item_index = self.target.control_element_type().into();
                combo.select_combo_box_item_by_index(item_index).unwrap();
                ""
            }
        };
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
                ReaperTargetType::NavigateWithinGroup => Some("Group"),
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
        // Currently not in use
        self.view
            .require_control(root::ID_TARGET_LINE_2_LABEL_2)
            .hide();
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
                    combo.fill_combo_box_indexed(VirtualTrackType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.track_type().into())
                        .unwrap();
                }
                ReaperTargetType::GoToBookmark => {
                    combo.show();
                    combo.fill_combo_box_indexed(BookmarkAnchorType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.bookmark_anchor_type().into())
                        .unwrap();
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    combo.show();
                    combo.select_only_combo_box_item("Initial");
                }
                t if t.supports_feedback_resolution() => {
                    combo.show();
                    combo.fill_combo_box_indexed(FeedbackResolution::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.feedback_resolution().into(),
                        )
                        .unwrap();
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
                ReaperTargetType::Transport => {
                    combo.show();
                    combo.fill_combo_box_indexed(TransportAction::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.transport_action().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::AnyOn => {
                    combo.show();
                    combo.fill_combo_box_indexed(AnyOnParameter::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.any_on_parameter().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::AutomationModeOverride => {
                    combo.show();
                    combo.fill_combo_box_indexed(AutomationModeOverrideType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping
                                .target_model
                                .automation_mode_override_type()
                                .into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::GoToBookmark => {
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
                ReaperTargetType::NavigateWithinGroup => {
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
                            combo
                                .select_new_combo_box_item(format!("<Not present> ({})", group_id));
                        }
                        Some(i) => {
                            combo.select_combo_box_item_by_data(i as isize).unwrap();
                        }
                    }
                }
                ReaperTargetType::SendMidi => {
                    combo.show();
                    combo.fill_combo_box_indexed(SendMidiDestination::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.send_midi_destination().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::SendOsc => {
                    combo.show();
                    let osc_device_manager = App::get().osc_device_manager();
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
                                combo.select_new_combo_box_item(format!(
                                    "<Not present> ({})",
                                    dev_id
                                ));
                            }
                            Some(i) => combo.select_combo_box_item_by_data(i as isize).unwrap(),
                        }
                    } else {
                        combo.select_combo_box_item_by_data(-1).unwrap();
                    };
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
                                combo.select_combo_box_item_by_index(i as _).unwrap();
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
                        VirtualTrackType::Dynamic => self.target.track_expression().to_owned(),
                        VirtualTrackType::ByIndex => {
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

    fn invalidate_target_line_3_button(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_slot() => Some("..."),
                ReaperTargetType::SendMidi => Some("Pick!"),
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

    fn invalidate_target_line_4_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_TARGET_LINE_4_EDIT_CONTROL) {
            return;
        }
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_4_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendOsc => {
                    control.show();
                    let text = format_osc_arg_index(self.target.osc_arg_index());
                    control.set_text(text.as_str());
                }
                ReaperTargetType::FxParameter => {
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
                    control.set_text_or_hide(text);
                }
                t if t.supports_send() => {
                    let text = match self.target.route_selector_type() {
                        TrackRouteSelectorType::Dynamic => {
                            self.target.route_expression().to_owned()
                        }
                        TrackRouteSelectorType::ByName => self.target.route_name().to_owned(),
                        TrackRouteSelectorType::ByIndex => {
                            let index = self.target.route_index();
                            (index + 1).to_string()
                        }
                        _ => {
                            control.hide();
                            return;
                        }
                    };
                    control.set_text(text);
                    control.show();
                }
                t if t.supports_tags() => {
                    let text = format_tags_as_csv(self.target.tags());
                    control.set_text(text);
                    control.show();
                }
                _ => {
                    control.hide();
                }
            },
            TargetCategory::Virtual => {
                control.hide();
            }
        }
    }

    fn invalidate_target_line_3_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_TARGET_LINE_3_EDIT_CONTROL) {
            return;
        }
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_3_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::SendMidi => {
                    control.show();
                    let text = self.target.raw_midi_pattern();
                    control.set_text(text);
                }
                ReaperTargetType::SendOsc => {
                    control.show();
                    let text = self.target.osc_address_pattern();
                    control.set_text(text);
                }
                t if t.supports_fx() => {
                    let text = match self.target.fx_type() {
                        VirtualFxType::Dynamic => self.target.fx_expression().to_owned(),
                        VirtualFxType::ByIndex => {
                            let index = self.target.fx_index();
                            (index + 1).to_string()
                        }
                        VirtualFxType::ByName | VirtualFxType::AllByName => {
                            self.target.fx_name().to_owned()
                        }
                        _ => {
                            control.hide();
                            return;
                        }
                    };
                    control.set_text(text);
                    control.show();
                }
                _ => {
                    control.hide();
                }
            },
            TargetCategory::Virtual => {
                control.hide();
            }
        }
    }

    fn invalidate_target_line_3_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some("Invoke"),
                ReaperTargetType::TrackSolo => Some("Behavior"),
                ReaperTargetType::TrackShow => Some("Area"),
                ReaperTargetType::AutomationTouchState => Some("Type"),
                ReaperTargetType::SendMidi => Some("Pattern"),
                ReaperTargetType::SendOsc => Some("Address"),
                _ if self.target.supports_automation_mode() => Some("Mode"),
                t if t.supports_slot() => Some("Slot"),
                t if t.supports_fx() => Some("FX"),
                t if t.supports_send() => Some("Kind"),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_3_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_4_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some("Action"),
                ReaperTargetType::FxParameter => Some("Parameter"),
                ReaperTargetType::LoadFxSnapshot => Some("Snapshot"),
                ReaperTargetType::SendOsc => Some("Argument"),
                ReaperTargetType::ClipTransport => Some("Action"),
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
        let state = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_slot() => {
                    let instance_state = self.session.instance_state().borrow();
                    let slot = instance_state.get_slot(self.target.slot_index()).ok();
                    let (label, enabled) = if let Some(slot) = slot {
                        if let Some(content) = &slot.descriptor().content {
                            match content {
                                SlotContent::File { file } => (
                                    file.to_string_lossy().to_string(),
                                    slot.clip_info().is_some(),
                                ),
                            }
                        } else {
                            ("<Slot empty>".to_owned(), false)
                        }
                    } else {
                        ("<Invalid slot>".to_owned(), false)
                    };
                    Some((label, enabled))
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        let label = self.view.require_control(root::ID_TARGET_LINE_3_LABEL_2);
        if let Some((text, enabled)) = state {
            label.show();
            label.set_enabled(enabled);
            label.set_text(text);
        } else {
            label.hide();
        }
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
                t if t.supports_slot() => {
                    combo.show();
                    combo.fill_combo_box_indexed(
                        (0..CLIP_SLOT_COUNT).map(|i| format!("Slot {}", i + 1)),
                    );
                    combo
                        .select_combo_box_item_by_index(self.target.slot_index())
                        .unwrap();
                }
                t if t.supports_fx() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.fx_type().into())
                        .unwrap();
                }
                t if t.supports_send() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackRouteType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.route_type().into())
                        .unwrap();
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
                    combo.show();
                    combo.fill_combo_box_indexed(OscTypeTag::into_enum_iter());
                    let tag = self.target.osc_arg_type_tag();
                    combo.select_combo_box_item_by_index(tag.into()).unwrap();
                }
                ReaperTargetType::FxParameter => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxParameterType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.param_type().into())
                        .unwrap();
                }
                ReaperTargetType::NavigateWithinGroup => {
                    combo.show();
                    combo.fill_combo_box_indexed(SimpleExclusivity::into_enum_iter());
                    let simple_exclusivity: SimpleExclusivity = self.target.exclusivity().into();
                    combo
                        .select_combo_box_item_by_index(simple_exclusivity.into())
                        .unwrap();
                }
                t if t.supports_exclusivity() => {
                    combo.show();
                    combo.fill_combo_box_indexed(Exclusivity::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.exclusivity().into())
                        .unwrap();
                }
                t if t.supports_send() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackRouteSelectorType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.route_selector_type().into())
                        .unwrap();
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
                                        .resolve(&chain, context, self.mapping.compartment())
                                        .ok()
                                        .and_then(|fxs| fxs.into_iter().next())
                                    {
                                        combo
                                            .select_combo_box_item_by_index(fx.index() as _)
                                            .unwrap();
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
                                "Use 'By ID' only if track is 'By ID' as well!",
                            );
                        }
                    } else {
                        combo.hide();
                    }
                }
                ReaperTargetType::Action => {
                    combo.show();
                    combo.fill_combo_box_indexed(ActionInvocationType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.action_invocation_type().into())
                        .unwrap();
                }
                ReaperTargetType::TrackSolo => {
                    combo.show();
                    combo.fill_combo_box_indexed(SoloBehavior::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.solo_behavior().into())
                        .unwrap();
                }
                ReaperTargetType::TrackShow => {
                    combo.show();
                    combo.fill_combo_box_indexed(RealearnTrackArea::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.track_area().into())
                        .unwrap();
                }
                _ if self.target.supports_automation_mode() => {
                    combo.show();
                    combo.fill_combo_box_indexed(RealearnAutomationMode::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.automation_mode().into())
                        .unwrap();
                }
                ReaperTargetType::AutomationTouchState => {
                    combo.show();
                    combo.fill_combo_box_indexed(TouchedParameterType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.touched_parameter_type().into())
                        .unwrap();
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
                ReaperTargetType::ClipTransport => {
                    combo.show();
                    combo.fill_combo_box_indexed(TransportAction::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.transport_action().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::FxParameter
                    if self.target.param_type() == VirtualFxParameterType::ById =>
                {
                    combo.show();
                    let context = self.session.extended_context();
                    if let Ok(fx) = self
                        .target
                        .with_context(context, self.mapping.compartment())
                        .first_fx()
                    {
                        combo.fill_combo_box_indexed(fx_parameter_combo_box_entries(&fx));
                        let param_index = self.target.param_index();
                        combo
                            .select_combo_box_item_by_index(param_index as _)
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
                    combo.fill_combo_box_indexed(TrackExclusivity::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.track_exclusivity().into())
                        .unwrap();
                }
                t if t.supports_fx_display_type() => {
                    combo.show();
                    combo.fill_combo_box_indexed(FxDisplayType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.fx_display_type().into())
                        .unwrap();
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
                                    .select_combo_box_item_by_index(i as _)
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
                                        combo.select_combo_box_item_by_index(i as _).unwrap();
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
                t if t.supports_fx_chain() => {
                    if matches!(
                        self.target.fx_type(),
                        VirtualFxType::Focused | VirtualFxType::This
                    ) {
                        None
                    } else {
                        let is_input_fx = self.target.fx_is_input_fx();
                        let label = if self.target.track_type() == VirtualTrackType::Master {
                            "Monitoring FX"
                        } else {
                            "Input FX"
                        };
                        Some((label, is_input_fx))
                    }
                }
                t if t.supports_track_scrolling() => {
                    Some(("Scroll TCP", self.target.scroll_arrange_view()))
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
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
                    Some(("Scroll mixer", self.target.scroll_mixer()))
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
                ReaperTargetType::ClipTransport
                    if matches!(
                        self.target.transport_action(),
                        TransportAction::PlayStop
                            | TransportAction::PlayPause
                            | TransportAction::Stop
                    ) =>
                {
                    Some(("Next bar", self.target.next_bar()))
                }
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
                ReaperTargetType::Seek => Some(("Use loop points", self.target.use_loop_points())),
                ReaperTargetType::GoToBookmark => {
                    Some(("Set loop points", self.target.use_loop_points()))
                }
                ReaperTargetType::ClipTransport
                    if matches!(
                        self.target.transport_action(),
                        TransportAction::PlayStop | TransportAction::PlayPause
                    ) =>
                {
                    let is_enabled = !self.target.next_bar();
                    self.view
                        .require_control(checkbox_id)
                        .set_enabled(is_enabled);
                    Some((
                        "Buffered",
                        self.target.slot_play_options().is_effectively_buffered(),
                    ))
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
                ReaperTargetType::Seek => {
                    Some(("Use time selection", self.target.use_time_selection()))
                }
                ReaperTargetType::GoToBookmark => {
                    Some(("Set time selection", self.target.use_time_selection()))
                }
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
        let (error_msg, read_enabled, write_enabled, character) =
            if let Some(t) = self.first_resolved_target() {
                let control_context = self.session.control_context();
                let (error_msg, read_enabled, write_enabled) = if t.is_virtual() {
                    // Makes no sense to display any value controls for virtual targets. They neither
                    // have a value nor would moving a slider make any difference.
                    (None, false, false)
                } else if t.can_report_current_value() {
                    let value = t.current_value(control_context).unwrap_or_default();
                    self.invalidate_target_value_controls_with_value(value);
                    let write_enabled = !t.control_type(control_context).is_relative();
                    (None, true, write_enabled)
                } else {
                    // Target is real but can't report values (e.g. load mapping snapshot)
                    (None, false, true)
                };
                (
                    error_msg,
                    read_enabled,
                    write_enabled,
                    Some(t.character(control_context)),
                )
            } else {
                (Some("Target inactive!"), false, false, None)
            };
        self.show_if(
            read_enabled,
            &[
                root::ID_TARGET_VALUE_LABEL_TEXT,
                root::ID_TARGET_VALUE_EDIT_CONTROL,
                root::ID_TARGET_UNIT_BUTTON,
            ],
        );
        // Slider or buttons
        let off_button = self.view.require_control(root::ID_TARGET_VALUE_OFF_BUTTON);
        let on_button = self.view.require_control(root::ID_TARGET_VALUE_ON_BUTTON);
        let slider_control = self
            .view
            .require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL);
        if write_enabled {
            use TargetCharacter::*;
            match character {
                Some(Trigger) => {
                    slider_control.hide();
                    off_button.hide();
                    on_button.show();
                    on_button.set_text("Trigger!");
                }
                Some(Switch) => {
                    slider_control.hide();
                    off_button.show();
                    on_button.show();
                    on_button.set_text("On");
                }
                _ => {
                    off_button.hide();
                    on_button.hide();
                    slider_control.show();
                }
            }
        } else {
            slider_control.hide();
            off_button.hide();
            on_button.hide();
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
            self.panel.last_touched_mode_parameter.borrow().changed(),
            |view, _| {
                view.invalidate_help();
            },
        );
        self.panel.when(
            self.panel.last_touched_source_character.borrow().changed(),
            |view, _| {
                view.invalidate_help_from_source_character();
            },
        );
    }

    fn register_session_listeners(&self) {
        self.panel.when(
            self.session
                .instance_state()
                .borrow()
                .slot_contents_changed(),
            |view, _| {
                view.invalidate_target_line_3_label_2();
            },
        );
        self.panel.when(
            self.session.mapping_which_learns_source_changed(),
            |view, _| {
                view.invalidate_source_learn_button();
            },
        );
        self.panel.when(
            self.session.mapping_which_learns_target_changed(),
            |view, _| {
                view.invalidate_target_learn_button();
            },
        );
        self.panel.when(
            ReaperTarget::potential_static_change_events()
                .merge(ReaperTarget::potential_dynamic_change_events()),
            |view, _| {
                // These changes can happen because of removals (e.g. project close, FX deletions,
                // track deletions etc.). We want to update whatever is possible. But if the own
                // project is missing, this was a project close and we don't need to do anything
                // at all.
                if !view.target_with_context().project().is_available() {
                    return;
                }
                view.invalidate_target_controls(None);
                view.invalidate_mode_controls();
            },
        );
    }

    fn invalidate_mode_controls(&self) {
        self.fill_mode_type_combo_box();
        self.invalidate_mode_type_combo_box();
        self.invalidate_mode_control_appearance();
        self.invalidate_mode_source_value_controls(None);
        self.invalidate_mode_target_value_controls(None);
        self.invalidate_mode_step_controls(None);
        self.invalidate_mode_fire_controls(None);
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
        self.invalidate_mode_target_value_sequence_edit_control(None);
        self.invalidate_mode_eel_control_transformation_edit_control(None);
        self.invalidate_mode_eel_feedback_transformation_edit_control(None);
    }

    fn invalidate_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.absolute_mode().into())
            .unwrap();
    }

    fn invalidate_mode_control_appearance(&self) {
        self.invalidate_mode_control_labels();
        self.invalidate_mode_control_visibilities();
    }

    fn mapping_uses_step_counts(&self) -> bool {
        self.mapping
            .with_context(self.session.extended_context())
            .uses_step_counts()
    }

    fn invalidate_mode_control_labels(&self) {
        let step_label = if self.mapping_uses_step_counts() {
            "Speed"
        } else {
            "Step size"
        };
        self.view
            .require_control(root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT)
            .set_text(step_label);
    }

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
        // For all source characters
        {
            let show_source_min_max = is_relevant(ModeParameter::SourceMinMax);
            self.enable_if(
                show_source_min_max,
                &[
                    root::ID_SETTINGS_SOURCE_LABEL,
                    root::ID_SETTINGS_SOURCE_MIN_LABEL,
                    root::ID_SETTINGS_SOURCE_MAX_LABEL,
                    root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL,
                    root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL,
                    root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
                    root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
                ],
            );
            let show_reverse = is_relevant(ModeParameter::Reverse);
            self.enable_if(show_reverse, &[root::ID_SETTINGS_REVERSE_CHECK_BOX]);
            let show_out_of_range_behavior = is_relevant(ModeParameter::OutOfRangeBehavior);
            self.enable_if(
                show_out_of_range_behavior,
                &[
                    root::ID_MODE_OUT_OF_RANGE_LABEL_TEXT,
                    root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX,
                ],
            );
            let show_group_interaction = is_relevant(ModeParameter::GroupInteraction);
            self.enable_if(
                show_group_interaction,
                &[
                    root::ID_MODE_GROUP_INTERACTION_LABEL_TEXT,
                    root::ID_MODE_GROUP_INTERACTION_COMBO_BOX,
                ],
            );
            let show_target_value_sequence =
                is_relevant(ModeParameter::TargetValueSequence) && real_target.is_some();
            self.enable_if(
                show_target_value_sequence,
                &[
                    root::ID_SETTINGS_TARGET_SEQUENCE_LABEL_TEXT,
                    root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL,
                ],
            );
            let show_target_min_max =
                is_relevant(ModeParameter::TargetMinMax) && real_target.is_some();
            self.enable_if(
                show_target_min_max,
                &[
                    root::ID_SETTINGS_TARGET_LABEL_TEXT,
                    root::ID_SETTINGS_MIN_TARGET_LABEL_TEXT,
                    root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL,
                    root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL,
                    root::ID_SETTINGS_MIN_TARGET_VALUE_TEXT,
                    root::ID_SETTINGS_MAX_TARGET_LABEL_TEXT,
                    root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
                    root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
                    root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
                ],
            );
            let show_feedback_transformation = is_relevant(ModeParameter::FeedbackTransformation)
                || is_relevant(ModeParameter::TextualFeedbackExpression);
            self.enable_if(
                show_feedback_transformation,
                &[
                    root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL,
                    root::IDC_MODE_FEEDBACK_TYPE_BUTTON,
                ],
            );
            let show_feedback_type = is_relevant(ModeParameter::FeedbackType);
            self.enable_if(
                show_feedback_type,
                &[root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX],
            );
        }
        // For knobs/faders and buttons
        {
            let show_jump =
                target_can_report_current_value && is_relevant(ModeParameter::JumpMinMax);
            self.enable_if(
                show_jump,
                &[
                    root::ID_SETTINGS_TARGET_JUMP_LABEL_TEXT,
                    root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
                    root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
                    root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
                    root::ID_SETTINGS_MIN_TARGET_JUMP_LABEL_TEXT,
                    root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
                    root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
                    root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
                    root::ID_SETTINGS_MAX_TARGET_JUMP_LABEL_TEXT,
                ],
            );
            let show_round_controls = is_relevant(ModeParameter::RoundTargetValue)
                && self.target_with_context().is_known_to_be_roundable();
            self.enable_if(
                show_round_controls,
                &[root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX],
            );
            let show_takeover =
                target_can_report_current_value && is_relevant(ModeParameter::TakeoverMode);
            self.enable_if(
                show_takeover,
                &[root::ID_MODE_TAKEOVER_LABEL, root::ID_MODE_TAKEOVER_MODE],
            );
            let show_control_transformation = is_relevant(ModeParameter::ControlTransformation);
            self.enable_if(
                show_control_transformation,
                &[
                    root::ID_MODE_EEL_CONTROL_TRANSFORMATION_LABEL,
                    root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL,
                ],
            );
            let show_absolute_mode = is_relevant(ModeParameter::AbsoluteMode);
            self.enable_if(
                show_absolute_mode,
                &[
                    root::ID_SETTINGS_MODE_COMBO_BOX,
                    root::ID_SETTINGS_MODE_LABEL,
                ],
            );
            self.enable_if(
                show_jump
                    || show_round_controls
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
                    || is_relevant(ModeParameter::SpeedMin));
            let step_max_is_relevant = real_target.is_some()
                && (is_relevant(ModeParameter::StepSizeMax)
                    || is_relevant(ModeParameter::SpeedMax));
            self.enable_if(
                step_min_is_relevant || step_max_is_relevant,
                &[root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT],
            );
            self.enable_if(
                step_min_is_relevant,
                &[
                    root::ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT,
                    root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
                    root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
                    root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
                ],
            );
            self.enable_if(
                step_max_is_relevant,
                &[
                    root::ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT,
                    root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
                    root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
                    root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
                ],
            );
            let show_rotate = is_relevant(ModeParameter::Rotate);
            self.enable_if(show_rotate, &[root::ID_SETTINGS_ROTATE_CHECK_BOX]);
            let show_make_absolute = is_relevant(ModeParameter::MakeAbsolute);
            self.enable_if(
                show_make_absolute,
                &[root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX],
            );
            let show_relative_filter = is_relevant(ModeParameter::RelativeFilter);
            self.enable_if(
                show_relative_filter,
                &[root::ID_MODE_RELATIVE_FILTER_COMBO_BOX],
            );
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
            self.enable_if(show_button_filter, &[root::ID_MODE_BUTTON_FILTER_COMBO_BOX]);
            let show_fire_mode = is_relevant(ModeParameter::FireMode);
            self.enable_if(
                show_fire_mode,
                &[
                    root::ID_MODE_FIRE_COMBO_BOX,
                    root::ID_MODE_FIRE_LINE_2_LABEL_1,
                    root::ID_MODE_FIRE_LINE_2_SLIDER_CONTROL,
                    root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL,
                    root::ID_MODE_FIRE_LINE_2_LABEL_2,
                    root::ID_MODE_FIRE_LINE_3_LABEL_1,
                    root::ID_MODE_FIRE_LINE_3_SLIDER_CONTROL,
                    root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL,
                    root::ID_MODE_FIRE_LINE_3_LABEL_2,
                ],
            );
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
        self.invalidate_mode_min_jump_controls(initiator);
        self.invalidate_mode_max_jump_controls(initiator);
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

    fn invalidate_mode_min_jump_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.jump_interval().min_val()),
            initiator,
            true,
        );
    }

    fn invalidate_mode_max_jump_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.jump_interval().max_val()),
            initiator,
            true,
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
            self.mode.step_interval().min_val(),
            initiator,
        );
    }

    fn invalidate_mode_fire_line_2_controls(&self, initiator: Option<u32>) {
        let label = match self.mapping.mode_model.fire_mode() {
            FireMode::WhenButtonReleased => Some("Min"),
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
        self.show_if(
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
            self.mode.step_interval().max_val(),
            initiator,
        );
    }

    fn invalidate_mode_fire_line_3_controls(&self, initiator: Option<u32>) {
        let option = match self.mapping.mode_model.fire_mode() {
            FireMode::WhenButtonReleased | FireMode::OnSinglePress => {
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
        self.show_if(
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
        value: SoftSymmetricUnitValue,
        initiator: Option<u32>,
    ) {
        let (val, edit_text, value_text) = match &self.first_resolved_target() {
            Some(target) => {
                if self.mapping_uses_step_counts() {
                    let edit_text = convert_unit_value_to_factor(value).to_string();
                    let val = PositiveOrSymmetricUnitValue::Symmetric(value);
                    // "count {x}"
                    (val, edit_text, "x".to_string())
                } else {
                    // "{size} {unit}"
                    let control_context = self.session.control_context();
                    let pos_value = value.clamp_to_positive_unit_interval();
                    let edit_text =
                        target.format_step_size_without_unit(pos_value, control_context);
                    let value_text = get_text_right_to_step_size_edit_control(
                        target,
                        pos_value,
                        control_context,
                    );
                    (
                        PositiveOrSymmetricUnitValue::Positive(pos_value),
                        edit_text,
                        value_text,
                    )
                }
            }
            None => (
                PositiveOrSymmetricUnitValue::Positive(UnitValue::MIN),
                "".to_string(),
                "".to_string(),
            ),
        };
        match val {
            PositiveOrSymmetricUnitValue::Positive(v) => {
                self.view
                    .require_control(slider_control_id)
                    .set_slider_unit_value(v);
            }
            PositiveOrSymmetricUnitValue::Symmetric(v) => {
                self.view
                    .require_control(slider_control_id)
                    .set_slider_symmetric_unit_value(v);
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
            .select_combo_box_item_by_index(self.mode.feedback_type().into())
            .unwrap();
    }

    fn invalidate_mode_make_absolute_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX)
            .set_checked(self.mode.make_absolute());
    }

    fn invalidate_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .select_combo_box_item_by_index(self.mode.out_of_range_behavior().into())
            .unwrap();
    }

    fn invalidate_mode_group_interaction_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.group_interaction().into())
            .unwrap();
    }

    fn invalidate_mode_fire_mode_combo_box(&self) {
        let combo = self.view.require_control(root::ID_MODE_FIRE_COMBO_BOX);
        combo.set_enabled(self.target_category() != TargetCategory::Virtual);
        combo
            .select_combo_box_item_by_index(self.mapping.mode_model.fire_mode().into())
            .unwrap();
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
            .select_combo_box_item_by_index(mode.into())
            .unwrap();
    }

    fn invalidate_mode_button_usage_combo_box(&self) {
        let usage = self.mode.button_usage();
        self.view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .select_combo_box_item_by_index(usage.into())
            .unwrap();
    }

    fn invalidate_mode_encoder_usage_combo_box(&self) {
        let usage = self.mode.encoder_usage();
        self.view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .select_combo_box_item_by_index(usage.into())
            .unwrap();
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
        self.view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .set_text(self.mode.eel_control_transformation());
    }

    fn invalidate_mode_eel_feedback_transformation_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL) {
            return;
        }
        let prop = if self.mode.feedback_type().is_textual() {
            self.mode.textual_feedback_expression()
        } else {
            self.mode.eel_feedback_transformation()
        };
        self.view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .set_text(prop);
    }

    fn fill_source_category_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX);
        let is_main_mapping = self.mapping.compartment() == MappingCompartment::MainMappings;
        b.fill_combo_box_small(
            SourceCategory::into_enum_iter()
                // Don't allow controller mappings to have virtual source
                .filter(|c| is_main_mapping || *c != SourceCategory::Virtual),
        );
    }

    fn fill_mapping_feedback_send_behavior_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX);
        b.fill_combo_box_indexed(FeedbackSendBehavior::into_enum_iter());
    }

    fn fill_target_category_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX);
        b.fill_combo_box_indexed(TargetCategory::into_enum_iter());
    }

    fn fill_source_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category() {
            Midi => b.fill_combo_box_indexed(MidiSourceType::into_enum_iter()),
            Reaper => b.fill_combo_box_indexed(ReaperSourceType::into_enum_iter()),
            Virtual => b.fill_combo_box_indexed(VirtualControlElementType::into_enum_iter()),
            Osc | Never => {}
        };
    }

    fn fill_mode_type_combo_box(&self) {
        let target_category = self.mapping.target_model.category();
        let items = AbsoluteMode::into_enum_iter().map(|m| {
            let suffix =
                if target_category == TargetCategory::Virtual && m == AbsoluteMode::ToggleButton {
                    " (invalid because target is virtual!)"
                } else {
                    ""
                };
            format!("{}{}", m, suffix)
        });
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .fill_combo_box_indexed(items);
    }

    fn fill_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .fill_combo_box_indexed(OutOfRangeBehavior::into_enum_iter());
    }

    fn fill_mode_group_interaction_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .fill_combo_box_indexed(GroupInteraction::into_enum_iter());
    }

    fn fill_mode_fire_mode_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_FIRE_COMBO_BOX)
            .fill_combo_box_indexed(FireMode::into_enum_iter());
    }

    fn fill_mode_feedback_type_combo_box(&self) {
        self.view
            .require_control(root::IDC_MODE_FEEDBACK_TYPE_COMBO_BOX)
            .fill_combo_box_indexed(FeedbackType::into_enum_iter());
    }

    fn fill_mode_takeover_mode_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_TAKEOVER_MODE)
            .fill_combo_box_indexed(TakeoverMode::into_enum_iter());
    }

    fn fill_mode_button_usage_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .fill_combo_box_indexed(ButtonUsage::into_enum_iter());
    }

    fn fill_mode_encoder_usage_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .fill_combo_box_indexed(EncoderUsage::into_enum_iter());
    }

    fn fill_target_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        use TargetCategory::*;
        match self.target.category() {
            Reaper => {
                let items =
                    ReaperTargetType::into_enum_iter().map(|t| (usize::from(t) as isize, t));
                b.fill_combo_box_with_data(items);
            }
            Virtual => b.fill_combo_box_indexed(VirtualControlElementType::into_enum_iter()),
        }
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
        true
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        self.hide();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.window_cache.replace(None);
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Mapping
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
            root::ID_SOURCE_SCRIPT_DETAIL_BUTTON => self.edit_midi_source_script(),
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
            root::ID_TARGET_CHECK_BOX_1 => self.write(|p| p.handle_target_check_box_1_change()),
            root::ID_TARGET_CHECK_BOX_2 => self.write(|p| p.handle_target_check_box_2_change()),
            root::ID_TARGET_CHECK_BOX_3 => self.write(|p| p.handle_target_check_box_3_change()),
            root::ID_TARGET_CHECK_BOX_4 => self.write(|p| p.handle_target_check_box_4_change()),
            root::ID_TARGET_CHECK_BOX_5 => self.write(|p| p.handle_target_check_box_5_change()),
            root::ID_TARGET_CHECK_BOX_6 => self.write(|p| p.handle_target_check_box_6_change()),
            root::ID_TARGET_LEARN_BUTTON => self.toggle_learn_target(),
            root::ID_TARGET_OPEN_BUTTON => self.write(|p| p.open_target()),
            root::ID_TARGET_LINE_2_BUTTON => {
                let _ = self.handle_target_line_2_button_press();
            }
            root::ID_TARGET_LINE_3_BUTTON => {
                let _ = self.handle_target_line_3_button_press();
            }
            root::ID_TARGET_LINE_4_BUTTON => {
                let _ = self.handle_target_line_4_button_press();
            }
            root::ID_TARGET_VALUE_OFF_BUTTON => {
                let _ = self.read(|p| p.hit_target(UnitValue::MIN));
            }
            root::ID_TARGET_VALUE_ON_BUTTON => {
                let _ = self.read(|p| p.hit_target(UnitValue::MAX));
            }
            root::ID_TARGET_UNIT_BUTTON => self.write(|p| p.handle_target_unit_button_press()),
            _ => unreachable!(),
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
                self.write(|p| p.handle_source_line_3_combo_box_1_change())
            }
            root::ID_SOURCE_NUMBER_COMBO_BOX => {
                self.write(|p| p.handle_source_line_4_combo_box_change())
            }
            root::ID_SOURCE_CHARACTER_COMBO_BOX => {
                self.write(|p| p.handle_source_line_5_combo_box_change())
            }
            root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX => {
                self.write(|p| p.handle_source_line_3_combo_box_2_change())
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
            root::ID_TARGET_TYPE_COMBO_BOX => self.write(|p| p.update_target_type()),
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
            // Help
            root::ID_MAPPING_HELP_APPLICABLE_TO_COMBO_BOX => {
                self.write(|p| p.handle_applicable_to_combo_box_change())
            }
            _ => unreachable!(),
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
            s if s == sliders.mode_min_jump => {
                self.write(|p| p.update_mode_min_jump_from_slider(s));
            }
            s if s == sliders.mode_max_jump => {
                self.write(|p| p.update_mode_max_jump_from_slider(s));
            }
            s if s == sliders.target_value => {
                let _ = self.read(|p| p.hit_target(s.slider_unit_value()));
            }
            _ => unreachable!(),
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
}

const SOURCE_MATCH_INDICATOR_TIMER_ID: usize = 570;

trait WindowExt {
    fn slider_unit_value(&self) -> UnitValue;
    fn slider_symmetric_unit_value(&self) -> SoftSymmetricUnitValue;
    fn slider_duration(&self) -> Duration;
    fn set_slider_unit_value(&self, value: UnitValue);
    fn set_slider_symmetric_unit_value(&self, value: SoftSymmetricUnitValue);
    fn set_slider_duration(&self, value: Duration);
}

impl WindowExt for Window {
    fn slider_unit_value(&self) -> UnitValue {
        let discrete_value = self.slider_value();
        UnitValue::new(discrete_value as f64 / 100.0)
    }

    fn slider_symmetric_unit_value(&self) -> SoftSymmetricUnitValue {
        self.slider_unit_value().map_to_symmetric_unit_interval()
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

    fn set_slider_symmetric_unit_value(&self, value: SoftSymmetricUnitValue) {
        self.set_slider_unit_value(value.map_to_positive_unit_interval());
    }

    fn set_slider_duration(&self, value: Duration) {
        // 0 = 0ms, 1 = 50ms, ..., 100 = 5s
        self.set_slider_range(0, 100);
        let val = (value.as_millis() / 50) as u32;
        self.set_slider_value(val);
    }
}

enum PositiveOrSymmetricUnitValue {
    Positive(UnitValue),
    Symmetric(SoftSymmetricUnitValue),
}

fn group_mappings_by_virtual_control_element<'a>(
    mappings: impl Iterator<Item = &'a SharedMapping>,
) -> HashMap<VirtualControlElement, Vec<&'a SharedMapping>> {
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
                    (edit_text, "".to_string())
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

fn track_combo_box_entries(project: Project) -> impl Iterator<Item = String> + ExactSizeIterator {
    let mut current_folder_level: i32 = 0;
    project.tracks().enumerate().map(move |(i, track)| {
        let indentation = ".".repeat(current_folder_level.abs() as usize * 4);
        let space = if indentation.is_empty() { "" } else { " " };
        let name = track.name().expect("non-master track must have name");
        let label = format!("{}. {}{}{}", i + 1, indentation, space, name.to_str());
        current_folder_level += track.folder_depth_change();
        label
    })
}

fn fx_combo_box_entries(chain: &FxChain) -> impl Iterator<Item = String> + ExactSizeIterator + '_ {
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

fn fx_parameter_combo_box_entries(
    fx: &Fx,
) -> impl Iterator<Item = String> + ExactSizeIterator + '_ {
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
        .enumerate()
        .map(|(i, (b, info))| {
            let name = b.name();
            let label = get_bookmark_label(i as _, info.id, &name);
            (info.id.get() as isize, label)
        })
}

fn select_bookmark_in_combo_box(combo: Window, anchor_type: BookmarkAnchorType, bookmark_ref: u32) {
    let successful = match anchor_type {
        BookmarkAnchorType::Id => combo
            .select_combo_box_item_by_data(bookmark_ref as _)
            .is_ok(),
        BookmarkAnchorType::Index => combo
            .select_combo_box_item_by_index(bookmark_ref as _)
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
    compartment: MappingCompartment,
) {
    let text = match target.category() {
        TargetCategory::Reaper => {
            if target.target_type().supports_track()
                && target.track_type() == VirtualTrackType::Dynamic
            {
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
    compartment: MappingCompartment,
) {
    let text = match target.category() {
        TargetCategory::Reaper => {
            if target.target_type().supports_fx() && target.fx_type() == VirtualFxType::Dynamic {
                target
                    .virtual_chain_fx()
                    .and_then(|fx| fx.calculated_fx_index(context, compartment))
                    .map(|i| i.to_string())
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
    compartment: MappingCompartment,
) {
    let text = match target.category() {
        TargetCategory::Reaper => match target.target_type() {
            ReaperTargetType::FxParameter
                if target.param_type() == VirtualFxParameterType::Dynamic =>
            {
                target
                    .virtual_fx_parameter()
                    .and_then(|p| p.calculated_fx_parameter_index(context, compartment))
                    .map(|i| i.to_string())
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

fn prompt_for_predefined_control_element_name(
    window: Window,
    r#type: VirtualControlElementType,
    grouped_mappings: &HashMap<VirtualControlElement, Vec<&SharedMapping>>,
) -> Option<String> {
    let menu_bar = MenuBar::new_popup_menu();
    let pure_menu = {
        use swell_ui::menu_tree::*;
        let daw_control_names = match r#type {
            VirtualControlElementType::Multi => {
                control_element_domains::daw::PREDEFINED_VIRTUAL_MULTI_NAMES
            }
            VirtualControlElementType::Button => {
                control_element_domains::daw::PREDEFINED_VIRTUAL_BUTTON_NAMES
            }
        };
        let entries = vec![
            menu(
                "DAW control",
                build_slash_menu_entries(daw_control_names, ""),
            ),
            menu(
                "Numbered",
                chunked_number_menu(100, 10, true, |i| {
                    let label = {
                        let pos = i + 1;
                        let element =
                            r#type.create_control_element(VirtualControlElementId::Indexed(i));
                        match grouped_mappings.get(&element) {
                            None => pos.to_string(),
                            Some(mappings) => {
                                let first_mapping = mappings[0].borrow();
                                let first_mapping_name = first_mapping.effective_name();
                                if mappings.len() == 1 {
                                    format!("{} ({})", pos, first_mapping_name)
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
                    item(label, move || (i + 1).to_string())
                }),
            ),
        ];
        let mut root_menu = root_menu(entries);
        root_menu.index(1);
        fill_menu(menu_bar.menu(), &root_menu);
        root_menu
    };
    let result_index = window.open_popup_menu(menu_bar.menu(), Window::cursor_pos())?;
    let item = pure_menu.find_item_by_id(result_index)?;
    Some(item.invoke_handler())
}

#[derive(Copy, Clone, Display)]
enum ColorTarget {
    #[display(fmt = "Color (if supported)")]
    Color,
    #[display(fmt = "Background color (if supported)")]
    BackgroundColor,
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

fn prompt_for_color(
    window: Window,
    color: Option<VirtualColor>,
    background_color: Option<VirtualColor>,
) -> Result<ChangeColorInstruction, &'static str> {
    let menu_bar = MenuBar::new_popup_menu();
    enum MenuAction {
        ControllerDefault(ColorTarget),
        OpenColorPicker(ColorTarget),
        UseColorProp(ColorTarget, &'static str),
    }
    let pure_menu = {
        use swell_ui::menu_tree::*;
        use MenuAction::*;
        let create_color_target_menu = |color_target: ColorTarget| {
            let relevant_color = match color_target {
                ColorTarget::Color => &color,
                ColorTarget::BackgroundColor => &background_color,
            };
            menu(
                color_target.to_string(),
                IntoIterator::into_iter([
                    item_with_opts(
                        "<Default color>",
                        ItemOpts {
                            enabled: true,
                            checked: relevant_color.is_none(),
                        },
                        move || ControllerDefault(color_target),
                    ),
                    item_with_opts(
                        "<Pick color...>",
                        ItemOpts {
                            enabled: true,
                            checked: matches!(relevant_color, Some(VirtualColor::Rgb(_))),
                        },
                        move || OpenColorPicker(color_target),
                    ),
                ])
                    .chain(IntoIterator::into_iter(["target.track.color", "target.bookmark.color"]).map(|key| {
                        item_with_opts(
                            key,
                            ItemOpts {
                                enabled: true,
                                checked: {
                                    matches!(relevant_color, Some(VirtualColor::Prop{prop}) if key == prop)
                                },
                            },
                            move || UseColorProp(color_target, key),
                        )
                    }))
                    .collect(),
            )
        };
        let entries = vec![
            create_color_target_menu(ColorTarget::Color),
            create_color_target_menu(ColorTarget::BackgroundColor),
        ];
        let mut root_menu = root_menu(entries);
        root_menu.index(1);
        fill_menu(menu_bar.menu(), &root_menu);
        root_menu
    };
    let result_index = window
        .open_popup_menu(menu_bar.menu(), Window::cursor_pos())
        .ok_or("color selection cancelled")?;
    let item = pure_menu
        .find_item_by_id(result_index)
        .ok_or("unknown item picked")?;
    let instruction = match item.invoke_handler() {
        MenuAction::ControllerDefault(target) => ChangeColorInstruction::new(target, None),
        MenuAction::UseColorProp(target, key) => ChangeColorInstruction::new(
            target,
            Some(VirtualColor::Prop {
                prop: key.to_string(),
            }),
        ),
        MenuAction::OpenColorPicker(target) => {
            let reaper = Reaper::get().medium_reaper();
            if let Some(native_color) =
                reaper.gr_select_color(WindowContext::Win(window.raw_non_null()))
            {
                let reaper_medium::RgbColor { r, g, b } = reaper.color_from_native(native_color);
                ChangeColorInstruction::new(target, Some(VirtualColor::Rgb(RgbColor::new(r, g, b))))
            } else {
                return Err("color picking cancelled");
            }
        }
    };
    Ok(instruction)
}

fn prompt_for_predefined_raw_midi_pattern(window: Window) -> Option<String> {
    let menu_bar = MenuBar::new_popup_menu();
    enum MenuAction {
        Preset(String),
        Help,
    }
    fn fmt_ch(ch: u8) -> String {
        format!("Channel {}", ch + 1)
    }
    fn double_data_byte_msg_menu(
        source_type: MidiSourceType,
        msg_type: ShortMessageType,
        label: &str,
    ) -> swell_ui::menu_tree::Entry<MenuAction> {
        use swell_ui::menu_tree::*;
        menu(
            source_type.to_string(),
            channel_menu(|ch| {
                menu(
                    fmt_ch(ch),
                    chunked_number_menu(128, 8, false, |i| {
                        item(format!("{} {}", label, i), move || {
                            let status_byte: u8 = msg_type.into();
                            MenuAction::Preset(format!(
                                "{:02X} {:02X} [0gfe dcba]",
                                status_byte + ch,
                                i
                            ))
                        })
                    }),
                )
            }),
        )
    }

    fn single_data_byte_msg_menu(
        source_type: MidiSourceType,
        msg_type: ShortMessageType,
        last_byte: u8,
    ) -> swell_ui::menu_tree::Entry<MenuAction> {
        use swell_ui::menu_tree::*;
        menu(
            source_type.to_string(),
            channel_menu(|ch| {
                item(fmt_ch(ch), move || {
                    let status_byte: u8 = msg_type.into();
                    MenuAction::Preset(format!(
                        "{:02X} [0gfe dcba] {:02X}",
                        status_byte + ch,
                        last_byte
                    ))
                })
            }),
        )
    }

    let pure_menu = {
        use swell_ui::menu_tree::*;

        use MenuAction::*;
        let entries = vec![
            item("Help", || Help),
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
                    item(fmt_ch(ch), move || {
                        let status_byte: u8 = ShortMessageType::PitchBendChange.into();
                        Preset(format!("{:02X} [0gfe dcba] [0nml kjih]", status_byte + ch))
                    })
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
        let mut root_menu = root_menu(entries);
        root_menu.index(1);
        fill_menu(menu_bar.menu(), &root_menu);
        root_menu
    };
    let result_index = window.open_popup_menu(menu_bar.menu(), Window::cursor_pos())?;
    let item = pure_menu.find_item_by_id(result_index)?;
    match item.invoke_handler() {
        MenuAction::Preset(preset) => Some(preset),
        MenuAction::Help => {
            open_in_browser(
                "https://github.com/helgoboss/realearn/blob/master/doc/user-guide.adoc#midi-send-message",
            );
            None
        }
    }
}

fn build_slash_menu_entries(
    names: &[&str],
    prefix: &str,
) -> Vec<swell_ui::menu_tree::Entry<String>> {
    use swell_ui::menu_tree::*;
    let mut entries = Vec::new();
    for (key, group) in &names
        .iter()
        .sorted()
        .group_by(|name| extract_first_segment(name))
    {
        if key.is_empty() {
            // All non-nested entries on this level (items).
            for name in group {
                let full_name = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", prefix, name)
                };
                entries.push(item(*name, move || full_name));
            }
        } else {
            // A nested entry (menu).
            let remaining_names: Vec<_> = group.map(|name| extract_remaining_name(name)).collect();
            let new_prefix = if prefix.is_empty() {
                key.to_string()
            } else {
                format!("{}/{}", prefix, key)
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

fn parse_osc_arg_index(text: &str) -> Option<u32> {
    let v = text.parse::<u32>().ok()?;
    // UI is 1-rooted
    Some(if v == 0 { v } else { v - 1 })
}

fn format_osc_arg_index(index: Option<u32>) -> String {
    if let Some(i) = index {
        (i + 1).to_string()
    } else {
        "".to_owned()
    }
}

enum SlotMenuAction {
    ShowSlotInfo,
    FillWithItemSource,
}
