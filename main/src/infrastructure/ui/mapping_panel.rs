use crate::core::{notification, when};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::{ItemProp, MainPanel, MappingHeaderPanel, YamlEditorPanel};

use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    AbsoluteMode, ControlValue, MidiClockTransportMessage, OscTypeTag, OutOfRangeBehavior,
    SoftSymmetricUnitValue, SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use reaper_high::{BookmarkType, Fx, FxChain, Project, Reaper, Track};
use reaper_low::raw;
use reaper_medium::{InitialAction, PromptForActionResult, SectionId};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::convert::TryInto;

use std::iter;

use std::ptr::null;
use std::rc::Rc;

use crate::application::{
    convert_factor_to_unit_value, convert_unit_value_to_factor, get_bookmark_label, get_fx_label,
    get_fx_param_label, get_non_present_bookmark_label, get_optional_fx_label, BookmarkAnchorType,
    MappingModel, MidiSourceType, ModeModel, ReaperTargetType, Session, SharedMapping,
    SharedSession, SourceCategory, SourceModel, TargetCategory, TargetModel,
    TargetModelWithContext, VirtualControlElementType, VirtualFxType, VirtualTrackType,
    WeakSession,
};
use crate::core::Global;
use crate::domain::{
    get_non_present_virtual_track_label, ActionInvocationType, CompoundMappingTarget,
    ExtendedProcessorContext, MappingCompartment, MappingId, RealearnTarget, ReaperTarget,
    SoloBehavior, TargetCharacter, TouchedParameterType, TrackExclusivity, TransportAction,
    VirtualControlElement, VirtualFx,
};
use itertools::Itertools;

use std::collections::HashMap;
use std::time::Duration;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, WeakView, Window};

#[derive(Debug)]
pub struct MappingPanel {
    view: ViewContext,
    session: WeakSession,
    mapping: RefCell<Option<SharedMapping>>,
    main_panel: WeakView<MainPanel>,
    mapping_header_panel: SharedView<MappingHeaderPanel>,
    is_invoked_programmatically: Cell<bool>,
    sliders: RefCell<Option<Sliders>>,
    advanced_settings_editor: RefCell<Option<SharedView<YamlEditorPanel>>>,
    // Fires when a mapping is about to change or the panel is hidden.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
}

// TODO-low Is it enough to have a MutableMappingPanel?
struct ImmutableMappingPanel<'a> {
    session: &'a Session,
    mapping_ptr: *const MappingModel,
    mapping: &'a MappingModel,
    source: &'a SourceModel,
    mode: &'a ModeModel,
    target: &'a TargetModel,
    view: &'a ViewContext,
    panel: &'a SharedView<MappingPanel>,
    shared_mapping: &'a SharedMapping,
}

struct MutableMappingPanel<'a> {
    session: &'a mut Session,
    shared_session: &'a SharedSession,
    mapping: &'a mut MappingModel,
    shared_mapping: &'a SharedMapping,
    panel: &'a SharedView<MappingPanel>,
    view: &'a ViewContext,
}

#[derive(Debug)]
struct Sliders {
    mode_min_target_value: Window,
    mode_max_target_value: Window,
    mode_min_source_value: Window,
    mode_max_source_value: Window,
    mode_min_step_size: Window,
    mode_max_step_size: Window,
    mode_min_length: Window,
    mode_max_length: Window,
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
            sliders: None.into(),
            advanced_settings_editor: Default::default(),
            party_is_over_subject: Default::default(),
        }
    }

    fn toggle_learn_source(&self) {
        let session = self.session();
        let mapping = self.mapping();
        session
            .borrow_mut()
            .toggle_learning_source(&session, &mapping);
    }

    fn handle_target_line_4_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let target_type = mapping.borrow().target_model.r#type.get();
        match target_type {
            ReaperTargetType::LoadFxSnapshot => {
                // Important that neither session nor mapping is mutably borrowed while doing this
                // because state of our ReaLearn instance is not unlikely to be
                // queried as well!
                let fx_snapshot = mapping
                    .borrow()
                    .target_model
                    .take_fx_snapshot(self.session().borrow().extended_context())?;
                mapping
                    .borrow_mut()
                    .target_model
                    .fx_snapshot
                    .set(Some(fx_snapshot));
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

    fn mapping_id(&self) -> Option<MappingId> {
        let mapping = self.mapping.borrow();
        let mapping = mapping.as_ref()?;
        let mapping = mapping.borrow();
        Some(mapping.id())
    }

    pub fn force_scroll_to_mapping_in_main_panel(&self) {
        if let Some(id) = self.mapping_id() {
            self.main_panel
                .upgrade()
                .expect("main view gone")
                .force_scroll_to_mapping(id);
        }
    }

    fn edit_advanced_settings(&self) {
        let mapping = self.mapping();
        let yaml_mapping = { mapping.borrow().advanced_settings().cloned() };
        let weak_mapping = Rc::downgrade(&mapping);
        let editor = YamlEditorPanel::new(yaml_mapping, move |yaml_mapping| {
            let m = match weak_mapping.upgrade() {
                None => return,
                Some(m) => m,
            };
            let result = { m.borrow_mut().set_advanced_settings(yaml_mapping) };
            if let Err(e) = result {
                notification::alert(format!(
                    "Your advanced mapping settings have been applied and saved but they contain the following error and therefore won't have any effect:\n\n{}",
                    e
                ));
            };
        });
        let editor = SharedView::new(editor);
        let editor_clone = editor.clone();
        if let Some(existing_editor) = self.advanced_settings_editor.replace(Some(editor)) {
            existing_editor.close();
        };
        editor_clone.open(self.view.require_window());
    }

    pub fn notify_target_value_changed(
        self: SharedView<Self>,
        target: Option<&CompoundMappingTarget>,
        new_value: UnitValue,
    ) {
        self.invoke_programmatically(|| {
            invalidate_target_controls_free(
                target,
                self.view
                    .require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL),
                self.view
                    .require_control(root::ID_TARGET_VALUE_EDIT_CONTROL),
                self.view.require_control(root::ID_TARGET_VALUE_TEXT),
                new_value,
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
            invalidate_dynamic_track_expression_result(
                &mapping.target_model,
                session.extended_context(),
                self.view.require_control(root::ID_TARGET_LINE_2_LABEL_3),
            );
            invalidate_dynamic_fx_expression_result(
                &mapping.target_model,
                session.extended_context(),
                self.view.require_control(root::ID_TARGET_LINE_3_LABEL_3),
            );
        });
        Ok(())
    }

    pub fn hide(&self) {
        self.stop_party();
        self.view.require_window().hide();
        self.mapping.replace(None);
        if let Some(p) = self.advanced_settings_editor.replace(None) {
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
            mapping_ptr: shared_mapping.as_ptr(),
            mapping: &mapping,
            source: &mapping.source_model,
            mode: &mapping.mode_model,
            target: &mapping.target_model,
            view: &self.view,
            panel: &self,
            shared_mapping: &shared_mapping,
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
            shared_session: &shared_session,
            mapping: &mut mapping,
            shared_mapping: &shared_mapping,
            panel: &self,
            view: &self.view,
        };
        op(&mut p)
    }

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }

    fn memorize_all_slider_controls(&self) {
        let view = &self.view;
        let sliders = Sliders {
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
            mode_min_length: view.require_control(root::ID_SETTINGS_MIN_LENGTH_SLIDER_CONTROL),
            mode_max_length: view.require_control(root::ID_SETTINGS_MAX_LENGTH_SLIDER_CONTROL),
            mode_min_jump: view.require_control(root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL),
            mode_max_jump: view.require_control(root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL),
            target_value: view.require_control(root::ID_TARGET_VALUE_SLIDER_CONTROL),
        };
        self.sliders.replace(Some(sliders));
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn when_do_sync(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(&ImmutableMappingPanel) + 'static + Copy,
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
        use root::*;
        match resource_id {
            ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_target_value_from_edit_control());
            }
            ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_target_value_from_edit_control());
            }
            ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_jump_from_edit_control());
            }
            ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_jump_from_edit_control());
            }
            ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_source_value_from_edit_control());
            }
            ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_source_value_from_edit_control());
            }
            ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_step_from_edit_control());
            }
            ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_step_from_edit_control());
            }
            ID_SETTINGS_MIN_LENGTH_EDIT_CONTROL => {
                self.write(|p| p.update_mode_min_length_from_edit_control());
            }
            ID_SETTINGS_MAX_LENGTH_EDIT_CONTROL => {
                self.write(|p| p.update_mode_max_length_from_edit_control());
            }
            _ => return Some(self),
        };
        None
    }
}

fn decorate_reaction(
    reaction: impl Fn(&ImmutableMappingPanel) + 'static + Copy,
) -> impl Fn(Rc<MappingPanel>, ()) + Copy {
    move |view, _| {
        let view_mirror = view.clone();
        view_mirror.is_invoked_programmatically.set(true);
        scopeguard::defer! { view_mirror.is_invoked_programmatically.set(false); }
        // If the reaction can't be displayed anymore because the mapping is not filled anymore,
        // so what.
        let _ = view.read(reaction);
    }
}

impl<'a> MutableMappingPanel<'a> {
    fn real_target(&self) -> Option<CompoundMappingTarget> {
        self.target_with_context().create_target().ok()
    }

    fn open_target(&self) {
        if let Some(t) = self.real_target() {
            Global::task_support()
                .do_later_in_main_thread_from_main_thread_asap(move || t.open())
                .unwrap();
        }
    }

    fn handle_target_line_2_button_press(&mut self) {
        match self.reaper_target_type() {
            ReaperTargetType::Action => {
                let reaper = Reaper::get().medium_reaper();
                use InitialAction::*;
                let initial_action = match self.mapping.target_model.action.get_ref().as_ref() {
                    None => NoneSelected,
                    Some(a) => Selected(a.command_id()),
                };
                // TODO-low Add this to reaper-high with rxRust
                if reaper.low().pointers().PromptForAction.is_none() {
                    self.view.require_window().alert(
                        "ReaLearn",
                        "Please update to REAPER >= 6.12 in order to pick actions!",
                    );
                    return;
                }
                reaper.prompt_for_action_create(initial_action, SectionId::new(0));
                let shared_mapping = self.shared_mapping.clone();
                Global::control_surface_rx()
                    .main_thread_idle()
                    .take_until(self.panel.party_is_over())
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
                                let action = Reaper::get()
                                    .main_section()
                                    .action_by_command_id(command_id);
                                shared_mapping
                                    .borrow_mut()
                                    .target_model
                                    .action
                                    .set(Some(action));
                            }
                        },
                        || {
                            Reaper::get()
                                .medium_reaper()
                                .prompt_for_action_finish(SectionId::new(0));
                        },
                    );
            }
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
                    (Some(mi), Some(ri)) => match self.mapping.target_model.bookmark_type.get() {
                        BookmarkType::Marker => (BookmarkType::Marker, mi),
                        BookmarkType::Region => (BookmarkType::Region, ri),
                    },
                };
                let bookmark_id = project
                    .find_bookmark_by_index(bookmark_index)
                    .unwrap()
                    .basic_info()
                    .id;
                let target = &mut self.mapping.target_model;
                target.bookmark_anchor_type.set(BookmarkAnchorType::Id);
                target.bookmark_type.set(bookmark_type);
                target.bookmark_ref.set(bookmark_id.get());
            }
            _ => {}
        }
    }

    fn update_mapping_prevent_echo_feedback(&mut self) {
        self.mapping.prevent_echo_feedback.set(
            self.view
                .require_control(root::ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_send_feedback_after_control(&mut self) {
        self.mapping.send_feedback_after_control.set(
            self.view
                .require_control(root::ID_MAPPING_SEND_FEEDBACK_AFTER_CONTROL_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_source_is_registered(&mut self) {
        self.mapping.source_model.is_registered.set(Some(
            self.view
                .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
                .is_checked(),
        ));
    }

    fn update_source_is_14_bit(&mut self) {
        let checked = self
            .view
            .require_control(root::ID_SOURCE_14_BIT_CHECK_BOX)
            .is_checked();
        use SourceCategory::*;
        match self.mapping.source_model.category.get() {
            Midi => {
                self.mapping.source_model.is_14_bit.set(Some(checked));
            }
            Osc => {
                self.mapping.source_model.osc_arg_is_relative.set(checked);
            }
            Virtual => {}
        };
    }

    fn update_source_channel_or_control_element(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.mapping.source_model.category.get() {
            Midi => {
                let value = match b.selected_combo_box_item_data() {
                    -1 => None,
                    id => Some(Channel::new(id as _)),
                };
                self.mapping.source_model.channel.set(value);
            }
            Virtual => {
                let index = b.selected_combo_box_item_index();
                self.mapping
                    .source_model
                    .control_element_index
                    .set(index as u32)
            }
            _ => {}
        };
    }

    fn update_source_midi_message_number(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(U7::new(id as _)),
        };
        self.mapping.source_model.midi_message_number.set(value);
    }

    fn update_source_character(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        let i = b.selected_combo_box_item_index();
        use SourceCategory::*;
        match self.mapping.source_model.category.get() {
            Midi => {
                self.mapping
                    .source_model
                    .custom_character
                    .set(i.try_into().expect("invalid source character"));
            }
            Osc => {
                self.mapping
                    .source_model
                    .osc_arg_type_tag
                    .set(i.try_into().expect("invalid OSC type tag"));
            }
            Virtual => {}
        }
    }

    fn update_source_category(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX);
        self.mapping.source_model.category.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid source category"),
        );
    }

    fn update_source_type(&mut self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        let i = b.selected_combo_box_item_index();
        use SourceCategory::*;
        match self.mapping.source_model.category.get() {
            Midi => self
                .mapping
                .source_model
                .midi_source_type
                .set(i.try_into().expect("invalid MIDI source type")),
            Virtual => self
                .mapping
                .source_model
                .control_element_type
                .set(i.try_into().expect("invalid virtual source type")),
            _ => {}
        };
    }

    fn update_source_midi_clock_transport_message_type(&mut self) {
        let b = self
            .view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX);
        self.mapping.source_model.midi_clock_transport_message.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid MTC message type"),
        );
    }

    fn update_source_parameter_number_message_number(&mut self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        let text = c.text().ok();
        use SourceCategory::*;
        match self.mapping.source_model.category.get() {
            Midi => {
                let value = text.and_then(|t| t.parse::<U14>().ok());
                self.mapping
                    .source_model
                    .parameter_number_message_number
                    .set(value);
            }
            Osc => {
                let value = text
                    .and_then(|t| {
                        let v = t.parse::<u32>().ok()?;
                        // UI is 1-rooted
                        Some(if v == 0 { v } else { v - 1 })
                    })
                    .unwrap_or(0);
                self.mapping.source_model.osc_arg_index.set(Some(value));
            }
            Virtual => {}
        };
    }

    fn update_source_pattern(&mut self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL);
        if let Ok(value) = c.text() {
            use SourceCategory::*;
            match self.mapping.source_model.category.get() {
                Midi => {
                    self.mapping.source_model.raw_midi_pattern.set(value);
                }
                Osc => {
                    self.mapping.source_model.osc_address_pattern.set(value);
                }
                Virtual => {}
            }
        }
    }

    fn update_mode_rotate(&mut self) {
        self.mapping.mode_model.rotate.set(
            self.view
                .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_make_absolute(&mut self) {
        self.mapping.mode_model.make_absolute.set(
            self.view
                .require_control(root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_out_of_range_behavior(&mut self) {
        let behavior = self
            .view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid out-of-range behavior");
        self.mapping.mode_model.out_of_range_behavior.set(behavior);
    }

    fn update_mode_round_target_value(&mut self) {
        self.mapping.mode_model.round_target_value.set(
            self.view
                .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_approach(&mut self) {
        self.mapping.mode_model.approach_target_value.set(
            self.view
                .require_control(root::ID_SETTINGS_SCALE_MODE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_reverse(&mut self) {
        self.mapping.mode_model.reverse.set(
            self.view
                .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn reset_mode(&mut self) {
        self.mapping.reset_mode(self.session.extended_context());
    }

    fn update_mode_type(&mut self) {
        let b = self.view.require_control(root::ID_SETTINGS_MODE_COMBO_BOX);
        self.mapping.mode_model.r#type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid mode type"),
        );
        self.mapping
            .set_preferred_mode_values(self.session.extended_context());
    }

    fn update_mode_min_target_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn get_value_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.real_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        target.parse_as_value(text.as_str()).ok()
    }

    fn get_step_size_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.real_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        target.parse_as_step_size(text.as_str()).ok()
    }

    fn update_mode_max_target_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_min_jump_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn update_mode_max_jump_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_min_source_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_source_edit_control(root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn get_value_from_source_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let text = self.view.require_control(edit_control_id).text().ok()?;
        self.mapping
            .source_model
            .parse_control_value(text.as_str())
            .ok()
    }

    fn update_mode_max_source_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_source_edit_control(root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_min_step_from_edit_control(&mut self) {
        let value = self
            .get_value_from_step_edit_control(root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL)
            .unwrap_or_else(|| UnitValue::MIN.to_symmetric());
        self.mapping
            .mode_model
            .step_interval
            .set_with(|prev| prev.with_min(value));
    }

    fn update_mode_min_length_from_edit_control(&mut self) {
        let value = self
            .get_value_from_duration_edit_control(root::ID_SETTINGS_MIN_LENGTH_EDIT_CONTROL)
            .unwrap_or_else(|| Duration::from_millis(0));
        self.mapping
            .mode_model
            .press_duration_interval
            .set_with(|prev| prev.with_min(value));
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
        let value = self
            .get_value_from_step_edit_control(root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL)
            .unwrap_or(SoftSymmetricUnitValue::SOFT_MAX);
        self.mapping
            .mode_model
            .step_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_max_length_from_edit_control(&mut self) {
        let value = self
            .get_value_from_duration_edit_control(root::ID_SETTINGS_MAX_LENGTH_EDIT_CONTROL)
            .unwrap_or_else(|| Duration::from_millis(0));
        self.mapping
            .mode_model
            .press_duration_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn update_mode_eel_control_transformation(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping
            .mode_model
            .eel_control_transformation
            .set(value);
    }

    fn update_mode_eel_feedback_transformation(&mut self) {
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping
            .mode_model
            .eel_feedback_transformation
            .set(value);
    }

    fn update_mode_min_target_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_target_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn update_mode_min_source_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_source_value_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn update_mode_min_step_from_slider(&mut self, slider: Window) {
        let step_counts = self.mapping_uses_step_counts();
        let prop = &mut self.mapping.mode_model.step_interval;
        if step_counts {
            prop.set_with(|prev| prev.with_min(slider.slider_symmetric_unit_value()));
        } else {
            prop.set_with(|prev| prev.with_min(slider.slider_unit_value().to_symmetric()));
        }
    }

    fn update_mode_max_step_from_slider(&mut self, slider: Window) {
        let step_counts = self.mapping_uses_step_counts();
        let prop = &mut self.mapping.mode_model.step_interval;
        if step_counts {
            prop.set_with(|prev| prev.with_max(slider.slider_symmetric_unit_value()));
        } else {
            prop.set_with(|prev| prev.with_max(slider.slider_unit_value().to_symmetric()));
        }
    }

    fn update_mode_min_length_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .press_duration_interval
            .set_with(|prev| prev.with_min(slider.slider_duration()));
    }

    fn update_mode_max_length_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .press_duration_interval
            .set_with(|prev| prev.with_max(slider.slider_duration()));
    }

    fn mapping_uses_step_counts(&self) -> bool {
        self.mapping
            .with_context(self.session.extended_context())
            .uses_step_counts()
    }

    fn update_mode_min_jump_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_jump_from_slider(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn handle_target_check_box_1_change(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_CHECK_BOX_1)
            .is_checked();
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let bookmark_type = if is_checked {
                        BookmarkType::Region
                    } else {
                        BookmarkType::Marker
                    };
                    self.mapping.target_model.bookmark_type.set(bookmark_type);
                }
                t if t.supports_fx() => {
                    self.mapping.target_model.fx_is_input_fx.set(is_checked);
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
        let target = &mut self.mapping.target_model;
        if target.supports_fx() {
            target.enable_only_if_fx_has_focus.set(is_checked);
        }
    }

    fn handle_target_check_box_2_change(&mut self) {
        self.mapping.target_model.enable_only_if_track_selected.set(
            self.view
                .require_control(root::ID_TARGET_CHECK_BOX_2)
                .is_checked(),
        );
    }

    fn toggle_learn_target(&mut self) {
        self.session
            .toggle_learning_target(self.shared_session, self.shared_mapping);
    }

    fn update_target_category(&mut self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX);
        self.mapping.target_model.category.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid target category"),
        );
    }

    fn update_target_type(&mut self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        let i = b.selected_combo_box_item_index();
        use TargetCategory::*;
        match self.mapping.target_model.category.get() {
            Reaper => self
                .mapping
                .target_model
                .r#type
                .set(i.try_into().expect("invalid REAPER target type")),
            Virtual => self
                .mapping
                .target_model
                .control_element_type
                .set(i.try_into().expect("invalid virtual target type")),
        };
    }

    fn handle_target_line_2_combo_box_1_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_2_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let bookmark_anchor_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping
                        .target_model
                        .bookmark_anchor_type
                        .set(bookmark_anchor_type);
                }
                t if t.supports_track() => {
                    let track_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping.target_model.track_type.set(track_type);
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
                t if t.supports_fx() => {
                    let fx_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping.target_model.fx_type.set(fx_type);
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_2_combo_box_2_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_2_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::GoToBookmark => {
                    let value: u32 = match self.mapping.target_model.bookmark_anchor_type.get() {
                        BookmarkAnchorType::Id => combo.selected_combo_box_item_data() as _,
                        BookmarkAnchorType::Index => combo.selected_combo_box_item_index() as _,
                    };
                    self.mapping.target_model.bookmark_ref.set(value);
                }
                ReaperTargetType::Transport => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .transport_action
                        .set(i.try_into().expect("invalid transport action"));
                }
                t if t.supports_track() => {
                    let project = self.session.context().project_or_current_project();
                    let i = combo.selected_combo_box_item_index();
                    if let Some(track) = project.track_by_index(i as _) {
                        self.mapping.target_model.track_id.set(Some(*track.guid()));
                        // We also set index and name so that we can easily switch between types.
                        self.mapping
                            .target_model
                            .track_index
                            .set_without_notification(i as _);
                        self.mapping
                            .target_model
                            .track_name
                            .set_without_notification(track.name().unwrap().into_string());
                    }
                }
                _ => {}
            },
            TargetCategory::Virtual => {
                let i = combo.selected_combo_box_item_index();
                self.mapping
                    .target_model
                    .control_element_index
                    .set(i as u32)
            }
        }
    }

    fn handle_target_line_3_combo_box_2_change(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_3_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => {
                    if let Ok(track) = self.target_with_context().effective_track() {
                        let chain = if self.mapping.target_model.fx_is_input_fx.get() {
                            track.input_fx_chain()
                        } else {
                            track.normal_fx_chain()
                        };
                        let i = combo.selected_combo_box_item_index();
                        if let Some(fx) = chain.fx_by_index(i as _) {
                            self.mapping.target_model.fx_id.set(fx.guid());
                            // We also set index and name so that we can easily switch between
                            // types.
                            self.mapping
                                .target_model
                                .fx_index
                                .set_without_notification(i as _);
                            self.mapping
                                .target_model
                                .fx_name
                                .set_without_notification(fx.name().into_string());
                        }
                    }
                }
                t if t.supports_send() => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping.target_model.send_index.set(Some(i as _));
                }
                ReaperTargetType::Action => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .action_invocation_type
                        .set(i.try_into().expect("invalid action invocation type"));
                }
                ReaperTargetType::TrackSolo => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .solo_behavior
                        .set(i.try_into().expect("invalid solo behavior"));
                }
                ReaperTargetType::AutomationTouchState => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .touched_parameter_type
                        .set(i.try_into().expect("invalid touched parameter type"));
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
                ReaperTargetType::FxParameter => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping.target_model.param_index.set(i as _);
                }
                t if t.supports_track_exclusivity() => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .track_exclusivity
                        .set(i.try_into().expect("invalid track exclusivity"));
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_2_edit_control_change(&mut self) {
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_2_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_track() => match self.mapping.target_model.track_type.get() {
                    VirtualTrackType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.mapping.target_model.track_expression.set(expression);
                    }
                    VirtualTrackType::ByName => {
                        let name = control.text().unwrap_or_default();
                        self.mapping.target_model.track_name.set(name);
                    }
                    VirtualTrackType::ByIndex => {
                        let position: i32 = control
                            .text()
                            .ok()
                            .and_then(|text| text.parse().ok())
                            .unwrap_or(1);
                        let index = std::cmp::max(position - 1, 0) as u32;
                        self.mapping.target_model.track_index.set(index);
                    }
                    _ => {}
                },
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_line_3_edit_control_change(&mut self) {
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_3_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => match self.mapping.target_model.fx_type.get() {
                    VirtualFxType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.mapping.target_model.fx_expression.set(expression);
                    }
                    VirtualFxType::ByName => {
                        let name = control.text().unwrap_or_default();
                        self.mapping.target_model.fx_name.set(name);
                    }
                    VirtualFxType::ByIndex => {
                        let position: i32 = control
                            .text()
                            .ok()
                            .and_then(|text| text.parse().ok())
                            .unwrap_or(1);
                        let index = std::cmp::max(position - 1, 0) as u32;
                        self.mapping.target_model.fx_index.set(index);
                    }
                    _ => {}
                },
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn target_category(&self) -> TargetCategory {
        self.mapping.target_model.category.get()
    }

    fn reaper_target_type(&self) -> ReaperTargetType {
        self.mapping.target_model.r#type.get()
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.extended_context())
    }
}

impl<'a> ImmutableMappingPanel<'a> {
    fn fill_all_controls(&self) {
        self.fill_source_category_combo_box();
        self.fill_source_midi_message_number_combo_box();
        self.fill_source_midi_clock_transport_message_type_combo_box();
        self.fill_mode_type_combo_box();
        self.fill_mode_out_of_range_behavior_combo_box();
        self.fill_target_category_combo_box();
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_window_title();
        self.panel.mapping_header_panel.invalidate_controls();
        self.invalidate_mapping_prevent_echo_feedback_check_box();
        self.invalidate_mapping_send_feedback_after_control_check_box();
        self.invalidate_mapping_advanced_settings_button();
        self.invalidate_source_controls();
        self.invalidate_target_controls();
        self.invalidate_mode_controls();
    }

    fn invalidate_window_title(&self) {
        self.view
            .require_window()
            .set_text(format!("Mapping \"{}\"", self.mapping.name.get_ref()));
    }

    fn invalidate_mapping_prevent_echo_feedback_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX);
        cb.set_checked(self.mapping.prevent_echo_feedback.get());
    }

    fn invalidate_mapping_send_feedback_after_control_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_SEND_FEEDBACK_AFTER_CONTROL_CHECK_BOX);
        cb.set_checked(self.mapping.send_feedback_after_control.get());
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
        self.invalidate_source_control_appearance();
        self.invalidate_source_category_combo_box();
        self.invalidate_source_type_combo_box();
        self.invalidate_source_learn_button();
        self.invalidate_source_channel_or_control_element_combo_box();
        self.invalidate_source_14_bit_check_box();
        self.invalidate_source_is_registered_check_box();
        self.invalidate_source_midi_message_number_controls();
        self.invalidate_source_parameter_number_message_number_controls();
        self.invalidate_source_character_combo_box();
        self.invalidate_source_midi_clock_transport_message_type_combo_box();
        self.invalidate_source_osc_address_pattern_edit_control();
    }

    fn invalidate_source_control_appearance(&self) {
        self.fill_source_channel_or_control_element_combo_box();
        self.invalidate_source_control_labels();
        self.invalidate_source_control_visibilities();
    }

    fn invalidate_source_control_labels(&self) {
        use SourceCategory::*;
        let (row_three, row_four, row_five, last_row) = match self.source.category.get() {
            Midi => (
                "Channel",
                self.source.midi_source_type.get().number_label(),
                "Character",
                "Pattern",
            ),
            Virtual => ("Number", "", "", ""),
            Osc => ("", "Argument", "Type", "Address"),
        };
        self.view
            .require_control(root::ID_SOURCE_CHANNEL_LABEL)
            .set_text(row_three);
        self.view
            .require_control(root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT)
            .set_text(row_four);
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_LABEL_TEXT)
            .set_text(row_five);
        self.view
            .require_control(root::ID_SOURCE_OSC_ADDRESS_LABEL_TEXT)
            .set_text(last_row);
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
        self.show_if(
            source.supports_channel() || source.supports_virtual_control_element_index(),
            &[
                root::ID_SOURCE_CHANNEL_COMBO_BOX,
                root::ID_SOURCE_CHANNEL_LABEL,
            ],
        );
        self.show_if(
            source.supports_midi_message_number()
                || source.supports_parameter_number_message_number()
                || source.is_osc(),
            &[root::ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT],
        );
        self.show_if(
            source.supports_is_registered(),
            &[root::ID_SOURCE_RPN_CHECK_BOX],
        );
        self.show_if(
            source.supports_14_bit() || source.is_osc(),
            &[root::ID_SOURCE_14_BIT_CHECK_BOX],
        );
        self.show_if(
            source.supports_midi_clock_transport_message_type(),
            &[
                root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX,
                root::ID_SOURCE_MIDI_MESSAGE_TYPE_LABEL_TEXT,
            ],
        );
        self.show_if(
            source.supports_custom_character() || source.is_osc(),
            &[
                root::ID_SOURCE_CHARACTER_COMBO_BOX,
                root::ID_SOURCE_CHARACTER_LABEL_TEXT,
            ],
        );
        self.show_if(
            source.supports_parameter_number_message_number() || source.is_osc(),
            &[root::ID_SOURCE_NUMBER_EDIT_CONTROL],
        );
        self.show_if(
            source.supports_midi_message_number(),
            &[root::ID_SOURCE_NUMBER_COMBO_BOX],
        );
        self.show_if(
            source.is_sys_ex() || source.is_osc(),
            &[
                root::ID_SOURCE_OSC_ADDRESS_LABEL_TEXT,
                root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL,
            ],
        )
    }

    fn show_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            self.view.require_control(*id).set_visible(condition);
        }
    }

    fn invalidate_source_category_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.source.category.get().into())
            .unwrap();
    }

    fn invalidate_target_category_combo_box(&self) {
        // Don't allow main mappings to have virtual target
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .set_enabled(self.mapping.compartment() != MappingCompartment::MainMappings);
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.target.category.get().into())
            .unwrap();
    }

    fn invalidate_source_type_combo_box(&self) {
        self.fill_source_type_combo_box();
        self.invalidate_source_type_combo_box_value();
    }

    fn invalidate_source_type_combo_box_value(&self) {
        use SourceCategory::*;
        let item_index = match self.source.category.get() {
            Midi => self.source.midi_source_type.get().into(),
            Virtual => self.source.control_element_type.get().into(),
            _ => return,
        };
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        b.select_combo_box_item_by_index(item_index).unwrap();
    }

    fn invalidate_source_learn_button(&self) {
        self.invalidate_learn_button(
            self.session.mapping_is_learning_source(self.mapping_ptr),
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

    fn invalidate_source_channel_or_control_element_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category.get() {
            Midi => {
                match self.source.channel.get() {
                    None => {
                        b.select_combo_box_item_by_data(-1).unwrap();
                    }
                    Some(ch) => {
                        b.select_combo_box_item_by_data(ch.get() as _).unwrap();
                    }
                };
            }
            Virtual => {
                b.select_combo_box_item_by_index(self.source.control_element_index.get() as _)
                    .unwrap();
            }
            _ => {}
        };
    }

    fn invalidate_source_14_bit_check_box(&self) {
        use SourceCategory::*;
        let (checked, label) = match self.source.category.get() {
            Midi => (
                self.source
                    .is_14_bit
                    .get()
                    // 14-bit == None not yet supported
                    .unwrap_or(false),
                "14-bit values",
            ),
            Osc => (self.source.osc_arg_is_relative.get(), "Is relative"),
            Virtual => return,
        };
        let c = self.view.require_control(root::ID_SOURCE_14_BIT_CHECK_BOX);
        c.set_text(label);
        c.set_checked(checked);
    }

    fn invalidate_source_is_registered_check_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_RPN_CHECK_BOX)
            .set_checked(
                self.source
                    .is_registered
                    .get()
                    // registered == None not yet supported
                    .unwrap_or(false),
            );
    }

    fn invalidate_source_midi_message_number_controls(&self) {
        let combo = self.view.require_control(root::ID_SOURCE_NUMBER_COMBO_BOX);
        let data = match self.source.midi_message_number.get() {
            None => -1,
            Some(n) => n.get() as _,
        };
        combo.select_combo_box_item_by_data(data).unwrap();
    }

    fn invalidate_source_parameter_number_message_number_controls(&self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        if c.has_focus() {
            return;
        }
        use SourceCategory::*;
        let text = match self.source.category.get() {
            Midi => match self.source.parameter_number_message_number.get() {
                None => "".to_owned(),
                Some(n) => n.to_string(),
            },
            Osc => {
                if let Some(i) = self.source.osc_arg_index.get() {
                    (i + 1).to_string()
                } else {
                    "".to_owned()
                }
            }
            Virtual => return,
        };
        c.set_text_if_not_focused(text)
    }

    fn invalidate_source_osc_address_pattern_edit_control(&self) {
        let c = self
            .view
            .require_control(root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL);
        if c.has_focus() {
            return;
        }
        use SourceCategory::*;
        let value_text = match self.source.category.get() {
            Midi => self.source.raw_midi_pattern.get_ref().as_str(),
            Osc => self.source.osc_address_pattern.get_ref().as_str(),
            Virtual => return,
        };
        c.set_text(value_text);
    }

    fn invalidate_source_character_combo_box(&self) {
        self.fill_source_character_combo_box();
        self.invalidate_source_character_combo_box_value();
    }

    fn invalidate_source_character_combo_box_value(&self) {
        use SourceCategory::*;
        let (label_text, item_index) = match self.source.category.get() {
            Midi => ("Character", self.source.custom_character.get().into()),
            Osc => ("Type", self.source.osc_arg_type_tag.get().into()),
            Virtual => return,
        };
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_LABEL_TEXT)
            .set_text(label_text);
        self.view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX)
            .select_combo_box_item_by_index(item_index)
            .unwrap();
    }

    fn invalidate_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .select_combo_box_item_by_index(self.source.midi_clock_transport_message.get().into())
            .unwrap();
    }

    fn invalidate_target_controls(&self) {
        self.invalidate_target_value_control_visibility();
        self.invalidate_target_category_combo_box();
        self.invalidate_target_type_combo_box();
        self.invalidate_target_line_2();
        self.invalidate_target_line_3();
        self.invalidate_target_line_4();
        self.invalidate_target_check_box_1();
        self.invalidate_target_check_box_2();
        self.invalidate_target_check_box_3();
        self.invalidate_target_value_controls();
        self.invalidate_target_learn_button();
    }

    fn invalidate_target_value_control_visibility(&self) {
        self.show_if(
            self.target.category.get() == TargetCategory::Reaper,
            &[
                root::ID_TARGET_VALUE_LABEL_TEXT,
                root::ID_TARGET_VALUE_SLIDER_CONTROL,
                root::ID_TARGET_VALUE_EDIT_CONTROL,
                root::ID_TARGET_VALUE_TEXT,
            ],
        );
    }

    fn invalidate_target_type_combo_box(&self) {
        self.fill_target_type_combo_box();
        self.invalidate_target_type_combo_box_value();
    }

    fn invalidate_target_type_combo_box_value(&self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        use TargetCategory::*;
        let item_index = match self.target.category.get() {
            Reaper => self.target.r#type.get().into(),
            Virtual => self.target.control_element_type.get().into(),
        };
        b.select_combo_box_item_by_index(item_index).unwrap();
    }

    fn target_category(&self) -> TargetCategory {
        self.target.category.get()
    }

    fn reaper_target_type(&self) -> ReaperTargetType {
        self.target.r#type.get()
    }

    fn invalidate_target_line_2_label_1(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some("Action"),
                ReaperTargetType::Transport => Some("Action"),
                ReaperTargetType::GoToBookmark => match self.target.bookmark_type.get() {
                    BookmarkType::Marker => Some("Marker"),
                    BookmarkType::Region => Some("Region"),
                },
                t if t.supports_track() => Some("Track"),
                _ => None,
            },
            TargetCategory::Virtual => Some("Number"),
        };
        self.view
            .require_control(root::ID_TARGET_LINE_2_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_2_label_2(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some(self.target.action_name_label().to_string()),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_2_LABEL_2)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_2_label_3(&self) {
        invalidate_dynamic_track_expression_result(
            self.target,
            self.session.extended_context(),
            self.view.require_control(root::ID_TARGET_LINE_2_LABEL_3),
        );
    }

    fn invalidate_target_line_2_combo_box_1(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_2_COMBO_BOX_1);
        match self.target_category() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                t if t.supports_track() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualTrackType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.track_type.get().into())
                        .unwrap();
                }
                ReaperTargetType::GoToBookmark => {
                    combo.show();
                    combo.fill_combo_box_indexed(BookmarkAnchorType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.target.bookmark_anchor_type.get().into(),
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

    fn invalidate_target_line_2_combo_box_2(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_2_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Transport => {
                    combo.show();
                    combo.fill_combo_box_indexed(TransportAction::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.transport_action.get().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::GoToBookmark => {
                    combo.show();
                    let project = self.target_with_context().project();
                    let bookmark_type = self.target.bookmark_type.get();
                    let bookmarks = bookmark_combo_box_entries(project, bookmark_type);
                    combo.fill_combo_box_with_data_vec(bookmarks.collect());
                    select_bookmark_in_combo_box(
                        combo,
                        self.target.bookmark_anchor_type.get(),
                        self.target.bookmark_ref.get(),
                    );
                }
                t if t.supports_track() => {
                    if matches!(
                        self.target.track_type.get(),
                        VirtualTrackType::ById | VirtualTrackType::ByIdOrName
                    ) {
                        combo.show();
                        let context = self.session.extended_context();
                        let project = context.context.project_or_current_project();
                        combo.fill_combo_box_indexed(track_combo_box_entries(project));
                        if let Some(virtual_track) = self.target.virtual_track() {
                            if let Ok(track) = virtual_track.resolve(context) {
                                let i = track.index().unwrap();
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
                combo.show();
                combo.fill_combo_box_small(1..=100);
                combo
                    .select_combo_box_item_by_index(self.target.control_element_index.get() as _)
                    .unwrap();
            }
        }
    }

    fn invalidate_target_line_2_edit_control(&self) {
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_2_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_track() => {
                    let text = match self.target.track_type.get() {
                        VirtualTrackType::Dynamic => self.target.track_expression.get_ref().clone(),
                        VirtualTrackType::ByIndex => {
                            let index = self.target.track_index.get();
                            (index + 1).to_string()
                        }
                        VirtualTrackType::ByName => self.target.track_name.get_ref().clone(),
                        _ => {
                            control.hide();
                            return;
                        }
                    };
                    control.set_text_if_not_focused(text);
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

    fn invalidate_target_line_2(&self) {
        self.invalidate_target_line_2_label_1();
        self.invalidate_target_line_2_label_2();
        self.invalidate_target_line_2_label_3();
        self.invalidate_target_line_2_combo_box_1();
        self.invalidate_target_line_2_combo_box_2();
        self.invalidate_target_line_2_edit_control();
        self.invalidate_target_line_2_button();
    }

    fn invalidate_target_line_2_button(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::Action => Some("Pick!"),
                ReaperTargetType::GoToBookmark => Some("Now!"),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_2_BUTTON)
            .set_text_or_hide(text);
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.extended_context())
    }

    fn invalidate_target_line_3(&self) {
        self.invalidate_target_line_3_label_1();
        self.invalidate_target_line_3_label_3();
        self.invalidate_target_line_3_combo_box_1();
        self.invalidate_target_line_3_combo_box_2();
        self.invalidate_target_line_3_edit_control();
    }

    fn invalidate_target_line_4(&self) {
        self.invalidate_target_line_4_label_1();
        self.invalidate_target_line_4_label_2();
        self.invalidate_target_line_4_label_3();
        self.invalidate_target_line_4_combo_box_1();
        self.invalidate_target_line_4_combo_box_2();
        self.invalidate_target_line_4_edit_control();
        self.invalidate_target_line_4_button();
    }

    fn invalidate_target_line_4_button(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
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
        invalidate_dynamic_fx_expression_result(
            self.target,
            self.session.extended_context(),
            self.view.require_control(root::ID_TARGET_LINE_3_LABEL_3),
        );
    }

    fn invalidate_target_line_4_label_3(&self) {
        let control = self.view.require_control(root::ID_TARGET_LINE_4_LABEL_3);
        control.hide();
        // TODO-high As soon as we have FX param dynamic
    }

    fn invalidate_target_line_4_edit_control(&self) {
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_4_EDIT_CONTROL);
        control.hide();
        // TODO-high As soon as we have FX param dynamic/name
    }

    fn invalidate_target_line_3_edit_control(&self) {
        let control = self
            .view
            .require_control(root::ID_TARGET_LINE_3_EDIT_CONTROL);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => {
                    let text = match self.target.fx_type.get() {
                        VirtualFxType::Dynamic => self.target.fx_expression.get_ref().clone(),
                        VirtualFxType::ByIndex => {
                            let index = self.target.fx_index.get();
                            (index + 1).to_string()
                        }
                        VirtualFxType::ByName => self.target.fx_name.get_ref().clone(),
                        _ => {
                            control.hide();
                            return;
                        }
                    };
                    control.set_text_if_not_focused(text);
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
                ReaperTargetType::AutomationTouchState => Some("Type"),
                t if t.supports_fx() => Some("FX"),
                t if t.supports_send() => Some("Send"),
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
                ReaperTargetType::FxParameter => Some("Parameter"),
                ReaperTargetType::LoadFxSnapshot => Some("Snapshot"),
                t if t.supports_track_exclusivity() => Some("Exclusive"),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.view
            .require_control(root::ID_TARGET_LINE_4_LABEL_1)
            .set_text_or_hide(text);
    }

    fn invalidate_target_line_4_label_2(&self) {
        let text = match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::LoadFxSnapshot => {
                    let label = if let Some(snapshot) = self.target.fx_snapshot.get_ref() {
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
            TargetCategory::Reaper => match self.target.r#type.get() {
                t if t.supports_fx() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.fx_type.get().into())
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
        combo.hide();
        // TODO-high As soon as we support different parameter types
    }

    fn invalidate_target_line_3_combo_box_2(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_3_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                t if t.supports_fx() => {
                    if matches!(
                        self.target.fx_type.get(),
                        VirtualFxType::ById | VirtualFxType::ByIdOrIndex
                    ) {
                        combo.show();
                        let context = self.session.extended_context();
                        if let Ok(track) = self.target.with_context(context).effective_track() {
                            // Fill
                            let chain = if self.target.fx_is_input_fx.get() {
                                track.input_fx_chain()
                            } else {
                                track.normal_fx_chain()
                            };
                            combo.fill_combo_box_indexed(fx_combo_box_entries(&chain));
                            // Set
                            if let Some(VirtualFx::ChainFx { chain_fx, .. }) =
                                self.target.virtual_fx()
                            {
                                if let Ok(fx) = chain_fx.resolve(&chain, context) {
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
                        combo.hide();
                    }
                }
                t if t.supports_send() => {
                    combo.show();
                    let context = self.session.extended_context();
                    if let Ok(track) = self.target.with_context(context).effective_track() {
                        // Fill
                        combo.fill_combo_box_indexed(send_combo_box_entries(&track));
                        // Set
                        if let Some(i) = self.target.send_index.get() {
                            combo
                                .select_combo_box_item_by_index(i as _)
                                .unwrap_or_else(|_| {
                                    let pity_label = format!("{}. <Not present>", i + 1);
                                    combo.select_new_combo_box_item(pity_label);
                                });
                        } else {
                            combo.select_new_combo_box_item("<None>");
                        }
                    } else {
                        combo.select_only_combo_box_item("<Requires track>");
                    }
                }
                ReaperTargetType::Action => {
                    combo.show();
                    combo.fill_combo_box_indexed(ActionInvocationType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.target.action_invocation_type.get().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::TrackSolo => {
                    combo.show();
                    combo.fill_combo_box_indexed(SoloBehavior::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.solo_behavior.get().into())
                        .unwrap();
                }
                ReaperTargetType::AutomationTouchState => {
                    combo.show();
                    combo.fill_combo_box_indexed(TouchedParameterType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.target.touched_parameter_type.get().into(),
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

    fn invalidate_target_line_4_combo_box_2(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_LINE_4_COMBO_BOX_2);
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
                ReaperTargetType::FxParameter => {
                    combo.show();
                    let context = self.session.extended_context();
                    if let Ok(fx) = self.target.with_context(context).fx() {
                        combo.fill_combo_box_indexed(fx_parameter_combo_box_entries(&fx));
                        let param_index = self.target.param_index.get();
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
                        .select_combo_box_item_by_index(self.target.track_exclusivity.get().into())
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

    fn invalidate_target_check_box_1(&self) {
        let res = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                t if t.supports_fx() => {
                    if self.target.fx_type.get() == VirtualFxType::Focused {
                        None
                    } else {
                        let is_input_fx = self.target.fx_is_input_fx.get();
                        let label = if self.target.track_type.get() == VirtualTrackType::Master {
                            "Monitoring FX"
                        } else {
                            "Input FX"
                        };
                        Some((label, is_input_fx))
                    }
                }
                ReaperTargetType::GoToBookmark => {
                    let is_regions = self.target.bookmark_type.get() == BookmarkType::Region;
                    Some(("Regions", is_regions))
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        let b = self.view.require_control(root::ID_TARGET_CHECK_BOX_1);
        if let Some((label, is_checked)) = res {
            b.set_text(label);
            b.set_checked(is_checked);
            b.show();
        } else {
            b.hide();
        }
    }

    fn invalidate_target_check_box_3(&self) {
        let is_checked = if self.target.supports_fx() {
            if self.target.fx_type.get() == VirtualFxType::Focused {
                None
            } else {
                Some(self.target.enable_only_if_fx_has_focus.get())
            }
        } else {
            None
        };
        self.view
            .require_control(root::ID_TARGET_CHECK_BOX_3)
            .set_checked_or_hide(is_checked);
    }

    fn invalidate_target_check_box_2(&self) {
        let target = self.target;
        let is_checked = if target.supports_track()
            && !matches!(target.track_type.get(), VirtualTrackType::Selected)
        {
            Some(target.enable_only_if_track_selected.get())
        } else {
            None
        };
        self.view
            .require_control(root::ID_TARGET_CHECK_BOX_2)
            .set_checked_or_hide(is_checked);
    }

    fn invalidate_target_value_controls(&self) {
        if let Some(t) = self.real_target() {
            let value = t.current_value().unwrap_or(UnitValue::MIN);
            self.invalidate_target_value_controls_with_value(value);
        }
    }

    fn invalidate_target_value_controls_with_value(&self, value: UnitValue) {
        self.invalidate_target_controls_internal(
            root::ID_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_TARGET_VALUE_EDIT_CONTROL,
            root::ID_TARGET_VALUE_TEXT,
            value,
        )
    }

    fn invalidate_target_learn_button(&self) {
        self.invalidate_learn_button(
            self.session.mapping_is_learning_target(self.mapping_ptr),
            root::ID_TARGET_LEARN_BUTTON,
        );
    }

    fn register_listeners(&self) {
        self.register_session_listeners();
        self.register_mapping_listeners();
        self.register_source_listeners();
        self.register_target_listeners();
        self.register_mode_listeners();
    }

    fn register_session_listeners(&self) {
        self.panel
            .when_do_sync(self.session.mapping_which_learns_source_changed(), |view| {
                view.invalidate_source_learn_button();
            });
        self.panel
            .when_do_sync(self.session.mapping_which_learns_target_changed(), |view| {
                view.invalidate_target_learn_button();
            });
        self.panel.when_do_sync(
            ReaperTarget::potential_static_change_events()
                .merge(ReaperTarget::potential_dynamic_change_events()),
            |view| {
                // TODO-medium The C++ code yields here (when FX changed):
                //  Yield. Because the model might also listen to such events and we want the model
                // to digest it *before* the  UI. It happened that this UI handler
                // is called *before* the model handler in some cases. Then it is super
                //  important - otherwise crash.
                let project = view.target_with_context().project();
                if !project.is_available() {
                    // This can happen when reacting to track changes while closing a project.
                    return;
                }
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
    }

    fn register_mapping_listeners(&self) {
        self.panel
            .when_do_sync(self.mapping.name.changed(), |view| {
                view.invalidate_window_title();
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::Name);
            });
        self.panel
            .when_do_sync(self.mapping.control_is_enabled.changed(), |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ControlEnabled);
            });
        self.panel
            .when_do_sync(self.mapping.feedback_is_enabled.changed(), |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::FeedbackEnabled);
            });
        self.panel
            .when_do_sync(self.mapping.prevent_echo_feedback.changed(), |view| {
                view.invalidate_mapping_prevent_echo_feedback_check_box();
            });
        self.panel
            .when_do_sync(self.mapping.send_feedback_after_control.changed(), |view| {
                view.invalidate_mapping_send_feedback_after_control_check_box();
            });
        self.panel
            .when_do_sync(self.mapping.advanced_settings_changed(), |view| {
                view.invalidate_mapping_advanced_settings_button();
            });
        self.panel.when_do_sync(
            self.mapping
                .activation_condition_model
                .activation_type
                .changed(),
            |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ActivationType);
            },
        );
        self.panel.when_do_sync(
            self.mapping
                .activation_condition_model
                .modifier_condition_1
                .changed(),
            |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ModifierCondition1);
            },
        );
        self.panel.when_do_sync(
            self.mapping
                .activation_condition_model
                .modifier_condition_2
                .changed(),
            |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ModifierCondition2);
            },
        );
        self.panel.when_do_sync(
            self.mapping
                .activation_condition_model
                .program_condition
                .changed(),
            |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ProgramCondition);
            },
        );
        self.panel.when_do_sync(
            self.mapping
                .activation_condition_model
                .eel_condition
                .changed(),
            |view| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::EelCondition);
            },
        );
    }

    fn register_source_listeners(&self) {
        let source = self.source;
        self.panel.when_do_sync(
            source
                .category
                .changed()
                .merge(source.midi_source_type.changed())
                .merge(source.control_element_type.changed()),
            |view| {
                view.invalidate_source_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when_do_sync(
            source
                .channel
                .changed()
                .merge(source.control_element_index.changed()),
            |view| {
                view.invalidate_source_channel_or_control_element_combo_box();
            },
        );
        self.panel.when_do_sync(source.is_14_bit.changed(), |view| {
            view.invalidate_source_controls();
            view.invalidate_mode_controls();
        });
        self.panel
            .when_do_sync(source.midi_message_number.changed(), |view| {
                view.invalidate_source_midi_message_number_controls();
            });
        self.panel.when_do_sync(
            source
                .parameter_number_message_number
                .changed()
                .merge(source.osc_arg_index.changed()),
            |view| {
                view.invalidate_source_parameter_number_message_number_controls();
            },
        );
        self.panel
            .when_do_sync(source.is_registered.changed(), |view| {
                view.invalidate_source_is_registered_check_box();
            });
        self.panel.when_do_sync(
            source
                .custom_character
                .changed()
                .merge(source.osc_arg_type_tag.changed()),
            |view| {
                view.invalidate_source_character_combo_box();
                view.invalidate_mode_controls();
            },
        );
        self.panel
            .when_do_sync(source.midi_clock_transport_message.changed(), |view| {
                view.invalidate_source_midi_clock_transport_message_type_combo_box();
            });
        self.panel.when_do_sync(
            source
                .osc_address_pattern
                .changed()
                .merge(source.raw_midi_pattern.changed()),
            |view| {
                view.invalidate_source_osc_address_pattern_edit_control();
            },
        );
        self.panel
            .when_do_sync(source.osc_arg_is_relative.changed(), |view| {
                view.invalidate_source_controls();
            });
    }

    fn invalidate_mode_controls(&self) {
        self.invalidate_mode_type_combo_box();
        self.invalidate_mode_control_appearance();
        self.invalidate_mode_source_value_controls();
        self.invalidate_mode_target_value_controls();
        self.invalidate_mode_step_controls();
        self.invalidate_mode_length_controls();
        self.invalidate_mode_rotate_check_box();
        self.invalidate_mode_make_absolute_check_box();
        self.invalidate_mode_out_of_range_behavior_combo_box();
        self.invalidate_mode_round_target_value_check_box();
        self.invalidate_mode_approach_check_box();
        self.invalidate_mode_reverse_check_box();
        self.invalidate_mode_eel_control_transformation_edit_control();
        self.invalidate_mode_eel_feedback_transformation_edit_control();
    }

    fn invalidate_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.r#type.get().into())
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
        let mode = self.mode;
        let target = match self.real_target() {
            None => return,
            Some(t) => t,
        };
        let show_round_controls = mode.supports_round_target_value()
            && self.target_with_context().is_known_to_be_roundable();
        self.show_if(
            show_round_controls,
            &[root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_reverse(),
            &[root::ID_SETTINGS_REVERSE_CHECK_BOX],
        );
        let show_jump_controls = mode.supports_jump() && target.can_report_current_value();
        self.show_if(
            show_jump_controls,
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
        self.show_if(
            show_jump_controls && mode.supports_approach_target_value(),
            &[root::ID_SETTINGS_SCALE_MODE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_out_of_range_behavior(),
            &[
                root::ID_MODE_OUT_OF_RANGE_LABEL_TEXT,
                root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX,
            ],
        );
        self.show_if(
            target.can_report_current_value(),
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
        self.show_if(
            mode.supports_steps(),
            &[
                root::ID_SETTINGS_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
                root::ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT,
                root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
                root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
            ],
        );
        self.show_if(
            mode.supports_rotate() && target.can_report_current_value(),
            &[root::ID_SETTINGS_ROTATE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_make_absolute(),
            &[root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX],
        );
        self.show_if(
            mode.supports_eel_control_transformation(),
            &[
                root::ID_MODE_EEL_CONTROL_TRANSFORMATION_LABEL,
                root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL,
            ],
        );
        self.show_if(
            mode.supports_eel_feedback_transformation(),
            &[
                root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_LABEL,
                root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL,
            ],
        );
    }

    fn invalidate_mode_source_value_controls(&self) {
        self.invalidate_mode_min_source_value_controls();
        self.invalidate_mode_max_source_value_controls();
    }

    fn invalidate_mode_target_value_controls(&self) {
        self.invalidate_mode_min_target_value_controls();
        self.invalidate_mode_max_target_value_controls();
        self.invalidate_mode_min_jump_controls();
        self.invalidate_mode_max_jump_controls();
    }

    fn invalidate_mode_min_source_value_controls(&self) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_source_value_controls(&self) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval.get_ref().max_val(),
        );
    }

    fn invalidate_mode_source_value_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value: UnitValue,
    ) {
        let formatted_value = self
            .source
            .format_control_value(ControlValue::Absolute(value))
            .unwrap_or_else(|_| "".to_string());
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(formatted_value);
        self.view
            .require_control(slider_control_id)
            .set_slider_unit_value(value);
    }

    fn invalidate_mode_min_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_VALUE_TEXT,
            self.mode.target_value_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_target_value_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
            self.mode.target_value_interval.get_ref().max_val(),
        );
    }

    fn invalidate_target_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: UnitValue,
    ) {
        invalidate_target_controls_free(
            self.real_target().as_ref(),
            self.view.require_control(slider_control_id),
            self.view.require_control(edit_control_id),
            self.view.require_control(value_text_control_id),
            value,
        );
    }

    fn get_text_right_to_step_size_edit_control(
        &self,
        t: &CompoundMappingTarget,
        step_size: UnitValue,
    ) -> String {
        if t.hide_formatted_step_size() {
            t.step_size_unit().to_string()
        } else {
            format!(
                "{}  {}",
                t.step_size_unit(),
                t.format_step_size_without_unit(step_size)
            )
        }
    }

    fn invalidate_mode_min_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
            self.mode.jump_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_jump_controls(&self) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
            self.mode.jump_interval.get_ref().max_val(),
        );
    }

    fn invalidate_mode_step_controls(&self) {
        self.invalidate_mode_min_step_controls();
        self.invalidate_mode_max_step_controls();
    }

    fn invalidate_mode_length_controls(&self) {
        self.invalidate_mode_min_length_controls();
        self.invalidate_mode_max_length_controls();
    }

    fn invalidate_mode_min_step_controls(&self) {
        self.invalidate_mode_step_controls_internal(
            root::ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT,
            self.mode.step_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_min_length_controls(&self) {
        self.invalidate_mode_press_duration_controls_internal(
            root::ID_SETTINGS_MIN_LENGTH_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_LENGTH_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_LENGTH_VALUE_TEXT,
            self.mode.press_duration_interval.get_ref().min_val(),
        );
    }

    fn invalidate_mode_max_step_controls(&self) {
        self.invalidate_mode_step_controls_internal(
            root::ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT,
            self.mode.step_interval.get_ref().max_val(),
        );
    }

    fn invalidate_mode_max_length_controls(&self) {
        self.invalidate_mode_press_duration_controls_internal(
            root::ID_SETTINGS_MAX_LENGTH_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_LENGTH_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_LENGTH_VALUE_TEXT,
            self.mode.press_duration_interval.get_ref().max_val(),
        );
    }

    fn invalidate_mode_step_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        value: SoftSymmetricUnitValue,
    ) {
        let (val, edit_text, value_text) = match &self.real_target() {
            Some(target) => {
                if self.mapping_uses_step_counts() {
                    let edit_text = convert_unit_value_to_factor(value).to_string();
                    let val = PositiveOrSymmetricUnitValue::Symmetric(value);
                    // "count {x}"
                    (val, edit_text, "x".to_string())
                } else {
                    // "{size} {unit}"
                    let pos_value = value.clamp_to_positive_unit_interval();
                    let edit_text = target.format_step_size_without_unit(pos_value);
                    let value_text =
                        self.get_text_right_to_step_size_edit_control(target, pos_value);
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
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(edit_text);
        self.view
            .require_control(value_text_control_id)
            .set_text(value_text)
    }

    fn invalidate_mode_press_duration_controls_internal(
        &self,
        slider_control_id: u32,
        edit_control_id: u32,
        value_text_control_id: u32,
        duration: Duration,
    ) {
        self.view
            .require_control(slider_control_id)
            .set_slider_duration(duration);
        self.view
            .require_control(edit_control_id)
            .set_text_if_not_focused(duration.as_millis().to_string());
        self.view
            .require_control(value_text_control_id)
            .set_text("ms")
    }

    fn invalidate_mode_rotate_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
            .set_checked(self.mode.rotate.get());
    }

    fn invalidate_mode_make_absolute_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX)
            .set_checked(self.mode.make_absolute.get());
    }

    fn invalidate_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .select_combo_box_item_by_index(self.mode.out_of_range_behavior.get().into())
            .unwrap();
    }

    fn invalidate_mode_round_target_value_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
            .set_checked(self.mode.round_target_value.get());
    }

    fn invalidate_mode_approach_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_SCALE_MODE_CHECK_BOX)
            .set_checked(self.mode.approach_target_value.get());
    }

    fn invalidate_mode_reverse_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
            .set_checked(self.mode.reverse.get());
    }

    fn invalidate_mode_eel_control_transformation_edit_control(&self) {
        self.view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .set_text_if_not_focused(self.mode.eel_control_transformation.get_ref().as_str());
    }

    fn invalidate_mode_eel_feedback_transformation_edit_control(&self) {
        self.view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .set_text_if_not_focused(self.mode.eel_feedback_transformation.get_ref().as_str());
    }

    fn register_target_listeners(&self) {
        let target = self.target;
        self.panel.when_do_sync(
            target
                .category
                .changed()
                .merge(target.r#type.changed())
                .merge(target.control_element_type.changed()),
            |view| {
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when_do_sync(
            target
                .track_type
                .changed()
                .merge(target.track_index.changed())
                .merge(target.track_id.changed())
                .merge(target.track_name.changed())
                .merge(target.track_expression.changed())
                .merge(target.bookmark_type.changed())
                .merge(target.bookmark_anchor_type.changed())
                .merge(target.bookmark_ref.changed())
                .merge(target.control_element_index.changed())
                .merge(target.transport_action.changed())
                .merge(target.action.changed()),
            |view| {
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when_do_sync(
            target
                .fx_type
                .changed()
                .merge(target.fx_index.changed())
                .merge(target.fx_id.changed())
                .merge(target.fx_name.changed())
                .merge(target.fx_expression.changed())
                .merge(target.fx_is_input_fx.changed()),
            |view| {
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel
            .when_do_sync(target.param_index.changed(), |view| {
                view.invalidate_target_line_4();
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            });
        self.panel
            .when_do_sync(target.action_invocation_type.changed(), |view| {
                view.invalidate_target_line_3();
                view.invalidate_mode_controls();
            });
        self.panel.when_do_sync(
            target
                .solo_behavior
                .changed()
                .merge(target.touched_parameter_type.changed()),
            |view| {
                view.invalidate_target_line_3();
            },
        );
        self.panel
            .when_do_sync(target.fx_snapshot.changed(), |view| {
                view.invalidate_target_line_4();
            });
        self.panel
            .when_do_sync(target.track_exclusivity.changed(), |view| {
                view.invalidate_target_line_4();
                view.invalidate_mode_controls();
            });
        self.panel.when_do_sync(
            target
                .fx_is_input_fx
                .changed()
                .merge(target.bookmark_type.changed()),
            |view| {
                view.invalidate_target_check_box_1();
            },
        );
        self.panel
            .when_do_sync(target.enable_only_if_track_selected.changed(), |view| {
                view.invalidate_target_check_box_2();
            });
        self.panel
            .when_do_sync(target.enable_only_if_fx_has_focus.changed(), |view| {
                view.invalidate_target_check_box_3();
            });
    }

    fn register_mode_listeners(&self) {
        let mode = self.mode;
        self.panel.when_do_sync(mode.r#type.changed(), |view| {
            view.invalidate_mode_controls();
        });
        self.panel
            .when_do_sync(mode.target_value_interval.changed(), |view| {
                view.invalidate_mode_min_target_value_controls();
                view.invalidate_mode_max_target_value_controls();
            });
        self.panel
            .when_do_sync(mode.source_value_interval.changed(), |view| {
                view.invalidate_mode_source_value_controls();
            });
        self.panel
            .when_do_sync(mode.jump_interval.changed(), |view| {
                view.invalidate_mode_min_jump_controls();
                view.invalidate_mode_max_jump_controls();
            });
        self.panel
            .when_do_sync(mode.step_interval.changed(), |view| {
                view.invalidate_mode_step_controls();
            });
        self.panel
            .when_do_sync(mode.press_duration_interval.changed(), |view| {
                view.invalidate_mode_length_controls();
            });
        self.panel
            .when_do_sync(mode.out_of_range_behavior.changed(), |view| {
                view.invalidate_mode_out_of_range_behavior_combo_box();
            });
        self.panel
            .when_do_sync(mode.round_target_value.changed(), |view| {
                view.invalidate_mode_round_target_value_check_box();
            });
        self.panel
            .when_do_sync(mode.approach_target_value.changed(), |view| {
                view.invalidate_mode_approach_check_box();
            });
        self.panel.when_do_sync(mode.rotate.changed(), |view| {
            view.invalidate_mode_rotate_check_box();
        });
        self.panel
            .when_do_sync(mode.make_absolute.changed(), |view| {
                view.invalidate_mode_make_absolute_check_box();
                view.invalidate_mode_step_controls();
            });
        self.panel.when_do_sync(mode.reverse.changed(), |view| {
            view.invalidate_mode_reverse_check_box();
        });
        self.panel
            .when_do_sync(mode.eel_control_transformation.changed(), |view| {
                view.invalidate_mode_eel_control_transformation_edit_control();
            });
        self.panel
            .when_do_sync(mode.eel_feedback_transformation.changed(), |view| {
                view.invalidate_mode_eel_feedback_transformation_edit_control();
            });
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

    fn fill_target_category_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX);
        b.fill_combo_box_indexed(TargetCategory::into_enum_iter());
    }

    fn fill_source_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category.get() {
            Midi => b.fill_combo_box_indexed(MidiSourceType::into_enum_iter()),
            Virtual => b.fill_combo_box_indexed(VirtualControlElementType::into_enum_iter()),
            Osc => {}
        };
    }

    fn fill_source_channel_or_control_element_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category.get() {
            Midi => b.fill_combo_box_with_data_small(
                iter::once((-1isize, "<Any> (no feedback)".to_string()))
                    .chain((0..16).map(|i| (i as isize, (i + 1).to_string()))),
            ),
            Virtual => {
                let controller_mappings = self
                    .session
                    .mappings(MappingCompartment::ControllerMappings);
                let grouped_mappings =
                    group_mappings_by_virtual_control_element(controller_mappings);
                let options = (0..100).map(|i| {
                    let element = self
                        .source
                        .control_element_type
                        .get()
                        .create_control_element(i);
                    let pos = i + 1;
                    match grouped_mappings.get(&element) {
                        None => pos.to_string(),
                        Some(mappings) => {
                            let first_mapping = mappings[0].borrow();
                            let first_mapping_name = first_mapping.name.get_ref().clone();
                            if mappings.len() == 1 {
                                format!("{} ({})", pos, first_mapping_name)
                            } else {
                                format!("{} ({} + {})", pos, first_mapping_name, mappings.len() - 1)
                            }
                        }
                    }
                });
                b.fill_combo_box_small(options);
            }
            _ => {}
        };
    }

    fn fill_source_midi_message_number_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_NUMBER_COMBO_BOX)
            .fill_combo_box_with_data_vec(
                iter::once((-1isize, "<Any> (no feedback)".to_string()))
                    .chain((0..128).map(|i| (i as isize, i.to_string())))
                    .collect(),
            )
    }

    fn fill_source_character_combo_box(&self) {
        let combo = self
            .view
            .require_control(root::ID_SOURCE_CHARACTER_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category.get() {
            Midi => {
                combo.fill_combo_box_indexed(SourceCharacter::into_enum_iter());
            }
            Osc => {
                combo.fill_combo_box_indexed(OscTypeTag::into_enum_iter());
            }
            Virtual => {}
        }
    }

    fn fill_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .fill_combo_box_indexed(MidiClockTransportMessage::into_enum_iter());
    }

    fn fill_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .fill_combo_box_indexed(AbsoluteMode::into_enum_iter());
    }

    fn fill_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .fill_combo_box_indexed(OutOfRangeBehavior::into_enum_iter());
    }

    fn fill_target_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        use TargetCategory::*;
        match self.target.category.get() {
            Reaper => {
                b.fill_combo_box_indexed(ReaperTargetType::into_enum_iter());
            }
            Virtual => b.fill_combo_box_indexed(VirtualControlElementType::into_enum_iter()),
        }
    }

    fn real_target(&self) -> Option<CompoundMappingTarget> {
        self.target_with_context().create_target().ok()
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
        self.memorize_all_slider_controls();
        self.mapping_header_panel.clone().open(window);
        true
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        self.hide();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.sliders.replace(None);
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Mapping
            root::ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX => {
                self.write(|p| p.update_mapping_prevent_echo_feedback())
            }
            root::ID_MAPPING_SEND_FEEDBACK_AFTER_CONTROL_CHECK_BOX => {
                self.write(|p| p.update_mapping_send_feedback_after_control())
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
            root::ID_SOURCE_RPN_CHECK_BOX => self.write(|p| p.update_source_is_registered()),
            root::ID_SOURCE_14_BIT_CHECK_BOX => self.write(|p| p.update_source_is_14_bit()),
            // Mode
            root::ID_SETTINGS_ROTATE_CHECK_BOX => self.write(|p| p.update_mode_rotate()),
            root::ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX => {
                self.write(|p| p.update_mode_make_absolute())
            }
            root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX => {
                self.write(|p| p.update_mode_round_target_value())
            }
            root::ID_SETTINGS_SCALE_MODE_CHECK_BOX => self.write(|p| p.update_mode_approach()),
            root::ID_SETTINGS_REVERSE_CHECK_BOX => self.write(|p| p.update_mode_reverse()),
            root::ID_SETTINGS_RESET_BUTTON => self.write(|p| p.reset_mode()),
            // Target
            root::ID_TARGET_CHECK_BOX_1 => self.write(|p| p.handle_target_check_box_1_change()),
            root::ID_TARGET_CHECK_BOX_2 => self.write(|p| p.handle_target_check_box_2_change()),
            root::ID_TARGET_CHECK_BOX_3 => self.write(|p| p.handle_target_check_box_3_change()),
            root::ID_TARGET_LEARN_BUTTON => self.write(|p| p.toggle_learn_target()),
            root::ID_TARGET_OPEN_BUTTON => self.write(|p| p.open_target()),
            root::ID_TARGET_LINE_2_BUTTON => {
                self.write(|p| p.handle_target_line_2_button_press());
            }
            root::ID_TARGET_LINE_4_BUTTON => {
                let _ = self.handle_target_line_4_button_press();
            }
            _ => unreachable!(),
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Source
            root::ID_SOURCE_CATEGORY_COMBO_BOX => self.write(|p| p.update_source_category()),
            root::ID_SOURCE_TYPE_COMBO_BOX => self.write(|p| p.update_source_type()),
            root::ID_SOURCE_CHANNEL_COMBO_BOX => {
                self.write(|p| p.update_source_channel_or_control_element())
            }
            root::ID_SOURCE_NUMBER_COMBO_BOX => {
                self.write(|p| p.update_source_midi_message_number())
            }
            root::ID_SOURCE_CHARACTER_COMBO_BOX => self.write(|p| p.update_source_character()),
            root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX => {
                self.write(|p| p.update_source_midi_clock_transport_message_type())
            }
            // Mode
            root::ID_SETTINGS_MODE_COMBO_BOX => self.write(|p| p.update_mode_type()),
            root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX => {
                self.write(|p| p.update_mode_out_of_range_behavior())
            }
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
            root::ID_TARGET_LINE_4_COMBO_BOX_2 => {
                self.write(|p| p.handle_target_line_4_combo_box_2_change())
            }
            _ => unreachable!(),
        }
    }

    fn slider_moved(self: SharedView<Self>, slider: Window) {
        let cloned_self = self.clone();
        let sliders = cloned_self.sliders.borrow();
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
            s if s == sliders.mode_min_length => {
                self.write(|p| p.update_mode_min_length_from_slider(s));
            }
            s if s == sliders.mode_max_length => {
                self.write(|p| p.update_mode_max_length_from_slider(s));
            }
            s if s == sliders.mode_min_jump => {
                self.write(|p| p.update_mode_min_jump_from_slider(s));
            }
            s if s == sliders.mode_max_jump => {
                self.write(|p| p.update_mode_max_jump_from_slider(s));
            }
            s if s == sliders.target_value => {
                if let Ok(Some(t)) = self.read(|p| p.real_target()) {
                    update_target_value(&t, s.slider_unit_value());
                }
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
            root::ID_SOURCE_NUMBER_EDIT_CONTROL => {
                view.write(|p| p.update_source_parameter_number_message_number());
            }
            root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL => {
                view.write(|p| p.update_source_pattern());
            }
            // Mode
            root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL => {
                view.write(|p| p.update_mode_eel_control_transformation());
            }
            root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL => {
                view.write(|p| p.update_mode_eel_feedback_transformation());
            }
            // Target
            root::ID_TARGET_LINE_2_EDIT_CONTROL => {
                view.write(|p| p.handle_target_line_2_edit_control_change())
            }
            root::ID_TARGET_LINE_3_EDIT_CONTROL => {
                view.write(|p| p.handle_target_line_3_edit_control_change())
            }
            root::ID_TARGET_VALUE_EDIT_CONTROL => {
                let (target, value) = view.write(|p| {
                    let value = p
                        .get_value_from_target_edit_control(root::ID_TARGET_VALUE_EDIT_CONTROL)
                        .unwrap_or(UnitValue::MIN);
                    (p.real_target(), value)
                });
                if let Some(t) = target {
                    update_target_value(&t, value);
                }
            }
            _ => return false,
        };
        true
    }

    // This is not called on Linux anyway, so this guard is just for making sure that nothing breaks
    // or is done two times if SWELL supports focus kill notification at some point on Linux.
    #[cfg(not(target_os = "linux"))]
    fn edit_control_focus_killed(self: SharedView<Self>, resource_id: u32) -> bool {
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
}

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

fn update_target_value(target: &CompoundMappingTarget, value: UnitValue) {
    // If it doesn't work in some cases, so what.
    let _ = target.control(ControlValue::Absolute(value));
}

fn group_mappings_by_virtual_control_element<'a>(
    mappings: impl Iterator<Item = &'a SharedMapping>,
) -> HashMap<VirtualControlElement, Vec<&'a SharedMapping>> {
    // Group by Option<VirtualControlElement>
    let grouped_by_option = mappings.group_by(|m| {
        let m = m.borrow();
        match m.target_model.category.get() {
            TargetCategory::Reaper => None,
            TargetCategory::Virtual => Some(m.target_model.create_control_element()),
        }
    });
    // Filter out None keys and collect to map with vector values
    grouped_by_option
        .into_iter()
        .filter_map(|(key, group)| key.map(|k| (k, group.collect())))
        .collect()
}

fn invalidate_target_controls_free(
    real_target: Option<&CompoundMappingTarget>,
    slider_control: Window,
    edit_control: Window,
    value_text_control: Window,
    value: UnitValue,
) {
    let (edit_text, value_text) = match real_target {
        Some(target) => {
            let edit_text = if target.character() == TargetCharacter::Discrete {
                target
                    .convert_unit_value_to_discrete_value(value)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| "".to_string())
            } else {
                target.format_value_without_unit(value)
            };
            let value_text = get_text_right_to_target_edit_control(&target, value);
            (edit_text, value_text)
        }
        None => ("".to_string(), "".to_string()),
    };
    slider_control.set_slider_unit_value(value);
    edit_control.set_text_if_not_focused(edit_text);
    value_text_control.set_text(value_text);
}

fn get_text_right_to_target_edit_control(t: &CompoundMappingTarget, value: UnitValue) -> String {
    if t.hide_formatted_value() {
        t.value_unit().to_string()
    } else if t.character() == TargetCharacter::Discrete {
        // Please note that discrete FX parameters can only show their *current* value,
        // unless they implement the REAPER VST extension functions.
        t.format_value(value)
    } else {
        format!("{}  {}", t.value_unit(), t.format_value(value))
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

fn send_combo_box_entries(track: &Track) -> impl Iterator<Item = String> + ExactSizeIterator + '_ {
    track.sends().map(|send| send.to_string())
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

fn invalidate_dynamic_track_expression_result(
    target: &TargetModel,
    context: ExtendedProcessorContext,
    label: Window,
) {
    let text = match target.category.get() {
        TargetCategory::Reaper => {
            if target.r#type.get().supports_track()
                && target.track_type.get() == VirtualTrackType::Dynamic
            {
                target
                    .virtual_track()
                    .and_then(|t| t.calculated_track_index(context))
                    .map(|i| i.to_string())
            } else {
                None
            }
        }
        TargetCategory::Virtual => None,
    };
    label.set_text_or_hide(text);
}

fn invalidate_dynamic_fx_expression_result(
    target: &TargetModel,
    context: ExtendedProcessorContext,
    label: Window,
) {
    let text = match target.category.get() {
        TargetCategory::Reaper => {
            if target.r#type.get().supports_fx() && target.fx_type.get() == VirtualFxType::Dynamic {
                target
                    .virtual_chain_fx()
                    .and_then(|fx| fx.calculated_fx_index(context))
                    .map(|i| i.to_string())
            } else {
                None
            }
        }
        TargetCategory::Virtual => None,
    };
    label.set_text_or_hide(text);
}
