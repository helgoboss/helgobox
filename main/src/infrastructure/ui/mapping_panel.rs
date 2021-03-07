use crate::core::{notification, when};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::{ItemProp, MainPanel, MappingHeaderPanel, YamlEditorPanel};

use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    AbsoluteMode, ControlValue, MidiClockTransportMessage, OscTypeTag, OutOfRangeBehavior,
    SoftSymmetricUnitValue, SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{Channel, U14, U7};
use reaper_high::{BookmarkType, Reaper};
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
    get_fx_param_label, get_guid_based_fx_at_index, get_non_present_bookmark_label,
    get_optional_fx_label, BookmarkAnchorType, FxAnchorType, MappingModel, MidiSourceType,
    ModeModel, ReaperTargetType, Session, SharedMapping, SharedSession, SourceCategory,
    SourceModel, TargetCategory, TargetModel, TargetModelWithContext, TrackAnchorType,
    VirtualControlElementType, WeakSession,
};
use crate::core::Global;
use crate::domain::{
    ActionInvocationType, CompoundMappingTarget, FxAnchor, MappingCompartment, MappingId,
    ProcessorContext, RealearnTarget, ReaperTarget, SoloBehavior, TargetCharacter,
    TouchedParameterType, TrackAnchor, TrackExclusivity, TransportAction, VirtualControlElement,
    VirtualFx, VirtualTrack,
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

    fn take_snapshot(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        // Important that neither session nor mapping is mutably borrowed while doing this because
        // state of our ReaLearn instance is not unlikely to be queried as well!
        let fx_snapshot = mapping
            .borrow()
            .target_model
            .take_fx_snapshot(self.session().borrow().context())?;
        mapping
            .borrow_mut()
            .target_model
            .fx_snapshot
            .set(Some(fx_snapshot));
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
        self.mapping.reset_mode(self.session.context());
    }

    fn update_mode_type(&mut self) {
        let b = self.view.require_control(root::ID_SETTINGS_MODE_COMBO_BOX);
        self.mapping.mode_model.r#type.set(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid mode type"),
        );
        self.mapping
            .set_preferred_mode_values(self.session.context());
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
            .with_context(self.session.context())
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

    fn update_target_is_input_fx(&mut self) {
        let is_enabled = self
            .view
            .require_control(root::ID_TARGET_INPUT_FX_CHECK_BOX)
            .is_checked();
        if self.mapping.target_model.r#type.get() == ReaperTargetType::GoToBookmark {
            let bookmark_type = if is_enabled {
                BookmarkType::Region
            } else {
                BookmarkType::Marker
            };
            self.mapping.target_model.bookmark_type.set(bookmark_type);
        } else {
            let new_virtual_fx = match self.mapping.target_model.fx.get_ref().as_ref() {
                None | Some(VirtualFx::Focused) => Some(VirtualFx::Particular {
                    is_input_fx: is_enabled,
                    anchor: FxAnchor::Index(0),
                }),
                Some(VirtualFx::Particular { anchor, .. }) => Some(VirtualFx::Particular {
                    anchor: anchor.clone(),
                    is_input_fx: is_enabled,
                }),
            };
            self.mapping.target_model.fx.set(new_virtual_fx);
        }
    }

    fn update_target_only_if_fx_has_focus(&mut self) {
        let is_checked = self
            .view
            .require_control(root::ID_TARGET_FX_FOCUS_CHECK_BOX)
            .is_checked();
        let target = &mut self.mapping.target_model;
        if target.supports_fx() {
            target.enable_only_if_fx_has_focus.set(is_checked);
        }
    }

    fn update_target_only_if_track_is_selected(&mut self) {
        self.mapping.target_model.enable_only_if_track_selected.set(
            self.view
                .require_control(root::ID_TARGET_TRACK_SELECTED_CHECK_BOX)
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

    fn update_target_line_two_anchor(&mut self) -> Result<(), &'static str> {
        if self.mapping.target_model.supports_track() {
            self.update_target_line_two_data()
        } else {
            self.update_target_bookmark_anchor();
            Ok(())
        }
    }

    fn update_target_bookmark_anchor(&mut self) {
        let anchor_type: BookmarkAnchorType = self
            .view
            .require_control(root::ID_TARGET_TRACK_ANCHOR_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .unwrap_or(BookmarkAnchorType::Id);
        self.mapping
            .target_model
            .bookmark_anchor_type
            .set(anchor_type);
    }

    fn update_target_line_two_data(&mut self) -> Result<(), &'static str> {
        let main_combo = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX);
        use TargetCategory::*;
        match self.mapping.target_model.category.get() {
            Reaper => {
                if self.mapping.target_model.supports_track() {
                    let data = main_combo.selected_combo_box_item_data();
                    use VirtualTrack::*;
                    let project = self.target_with_context().project();
                    let track = match data {
                        -3 => This,
                        -2 => Selected,
                        -1 => Master,
                        _ => {
                            let t = project
                                .track_by_index(data as u32)
                                .ok_or("track not existing")?;
                            let anchor_type: TrackAnchorType = self
                                .view
                                .require_control(root::ID_TARGET_TRACK_ANCHOR_COMBO_BOX)
                                .selected_combo_box_item_index()
                                .try_into()
                                .unwrap_or(TrackAnchorType::Id);
                            Particular(anchor_type.to_anchor(t).unwrap())
                        }
                    };
                    self.mapping.target_model.track.set(track);
                } else if self.mapping.target_model.r#type.get() == ReaperTargetType::Transport {
                    let data = main_combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .transport_action
                        .set(data.try_into().expect("invalid transport action"));
                } else if self.mapping.target_model.r#type.get() == ReaperTargetType::GoToBookmark {
                    let data: u32 = match self.mapping.target_model.bookmark_anchor_type.get() {
                        BookmarkAnchorType::Id => main_combo.selected_combo_box_item_data() as _,
                        BookmarkAnchorType::Index => {
                            main_combo.selected_combo_box_item_index() as _
                        }
                    };
                    self.mapping.target_model.bookmark_ref.set(data);
                }
            }
            Virtual => {
                let index = main_combo.selected_combo_box_item_index();
                self.mapping
                    .target_model
                    .control_element_index
                    .set(index as u32)
            }
        };
        Ok(())
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.context())
    }

    fn update_target_from_combo_box_line_three(&mut self) {
        let main_combo = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_COMBO_BOX);
        let target = &mut self.mapping.target_model;
        if target.supports_fx() {
            let anchor_combo = self
                .view
                .require_control(root::ID_TARGET_FX_ANCHOR_COMBO_BOX);
            Self::update_target_fx(self.session.context(), main_combo, anchor_combo, target);
        } else if target.supports_send() {
            let data = main_combo.selected_combo_box_item_data();
            let send_index = if data == -1 { None } else { Some(data as u32) };
            target.send_index.set(send_index);
        } else if target.r#type.get() == ReaperTargetType::Action {
            let index = main_combo.selected_combo_box_item_index();
            target
                .action_invocation_type
                .set(index.try_into().expect("invalid action invocation type"));
        } else if target.r#type.get() == ReaperTargetType::TrackSolo {
            let index = main_combo.selected_combo_box_item_index();
            target
                .solo_behavior
                .set(index.try_into().expect("invalid solo behavior"));
        } else if target.r#type.get() == ReaperTargetType::AutomationTouchState {
            let index = main_combo.selected_combo_box_item_index();
            target
                .touched_parameter_type
                .set(index.try_into().expect("invalid touched parameter type"));
        }
    }

    fn update_target_fx(
        context: &ProcessorContext,
        main_combo: Window,
        anchor_combo: Window,
        target: &mut TargetModel,
    ) {
        let item_data = main_combo.selected_combo_box_item_data();
        let virtual_fx = match item_data {
            -1 => VirtualFx::Focused,
            _ => {
                let i = item_data as u32;
                let track = target.track.get_ref();
                let is_input_fx = match target.fx.get_ref() {
                    None => false,
                    Some(virtual_fx) => match virtual_fx {
                        VirtualFx::Focused => false,
                        VirtualFx::Particular { is_input_fx, .. } => *is_input_fx,
                    },
                };
                if let Ok(fx) = get_guid_based_fx_at_index(context, track, is_input_fx, i) {
                    let anchor_type: FxAnchorType = anchor_combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or(FxAnchorType::Id);
                    VirtualFx::Particular {
                        is_input_fx,
                        anchor: anchor_type.to_anchor(&fx).unwrap_or(FxAnchor::Index(i)),
                    }
                } else {
                    VirtualFx::Particular {
                        is_input_fx,
                        anchor: FxAnchor::Index(i),
                    }
                }
            }
        };
        target.fx.set(Some(virtual_fx));
    }

    fn update_target_from_combo_box_line_four(&mut self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_FX_PARAMETER_COMBO_BOX);
        let target = &mut self.mapping.target_model;
        if target.supports_track_exclusivity() {
            let index = combo.selected_combo_box_item_index();
            target
                .track_exclusivity
                .set(index.try_into().expect("invalid track exclusivity"));
        } else {
            let data = combo.selected_combo_box_item_data();
            target.param_index.set(data as _);
        }
    }
}

impl<'a> ImmutableMappingPanel<'a> {
    fn pick_action(&self) {
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

    fn fill_all_controls(&self) {
        self.fill_source_category_combo_box();
        self.fill_source_midi_message_number_combo_box();
        self.fill_source_midi_clock_transport_message_type_combo_box();
        self.fill_mode_type_combo_box();
        self.fill_mode_out_of_range_behavior_combo_box();
        self.fill_target_category_combo_box();
        self.fill_target_fx_anchor_combo_box();
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
            .select_combo_box_item_by_index(self.source.category.get().into());
    }

    fn invalidate_target_category_combo_box(&self) {
        // Don't allow main mappings to have virtual target
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .set_enabled(self.mapping.compartment() != MappingCompartment::MainMappings);
        self.view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX)
            .select_combo_box_item_by_index(self.target.category.get().into());
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
        b.select_combo_box_item_by_index(item_index);
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
            .select_combo_box_item_by_index(item_index);
    }

    fn invalidate_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .select_combo_box_item_by_index(self.source.midi_clock_transport_message.get().into());
    }

    fn invalidate_target_controls(&self) {
        self.invalidate_target_control_appearance();
        self.invalidate_target_category_combo_box();
        self.invalidate_target_type_combo_box();
        self.invalidate_target_line_two();
        self.invalidate_target_line_three();
        self.invalidate_target_only_if_fx_has_focus_check_box();
        self.invalidate_target_only_if_track_is_selected_check_box();
        self.invalidate_target_line_four();
        self.invalidate_target_value_controls();
        self.invalidate_target_learn_button();
    }

    fn invalidate_target_control_appearance(&self) {
        self.invalidate_target_control_visibilities();
    }

    fn invalidate_target_control_visibilities(&self) {
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
        b.select_combo_box_item_by_index(item_index);
    }

    fn invalidate_target_line_two(&self) {
        let pick_button = self
            .view
            .require_control(root::ID_TARGET_PICK_ACTION_BUTTON);
        let action_label = self.view.require_control(root::ID_TARGET_ACTION_LABEL_TEXT);
        let main_combo = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX);
        let anchor_combo = self
            .view
            .require_control(root::ID_TARGET_TRACK_ANCHOR_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_TRACK_OR_CMD_LABEL_TEXT);
        let target = self.target;
        use TargetCategory::*;
        match self.target.category.get() {
            Reaper => {
                if target.supports_track() {
                    label.show();
                    main_combo.show();
                    action_label.hide();
                    pick_button.hide();
                    label.set_text("Track");
                    self.fill_target_track_combo_box(main_combo);
                    self.fill_track_anchor_combo_box(anchor_combo);
                    self.invalidate_target_track_combo_box_value(main_combo, anchor_combo);
                } else if self.target.r#type.get() == ReaperTargetType::Action {
                    label.show();
                    action_label.show();
                    pick_button.show();
                    main_combo.hide();
                    anchor_combo.hide();
                    label.set_text("Action");
                    let action_name = self.target.action_name_label().to_string();
                    action_label.set_text(action_name);
                } else if self.target.r#type.get() == ReaperTargetType::Transport {
                    label.show();
                    main_combo.show();
                    anchor_combo.hide();
                    action_label.hide();
                    pick_button.hide();
                    label.set_text("Action");
                    self.fill_target_transport_action_combo_box(main_combo);
                    self.set_target_transport_action_combo_box_value(main_combo);
                } else if self.target.r#type.get() == ReaperTargetType::GoToBookmark {
                    label.show();
                    main_combo.show();
                    anchor_combo.show();
                    action_label.hide();
                    pick_button.hide();
                    let label_text = match self.target.bookmark_type.get() {
                        BookmarkType::Marker => "Marker",
                        BookmarkType::Region => "Region",
                    };
                    label.set_text(label_text);
                    self.fill_target_bookmark_combo_box(main_combo);
                    self.fill_target_bookmark_anchor_combo_box(anchor_combo);
                    self.set_target_bookmark_combo_box_value(main_combo);
                    self.set_target_bookmark_anchor_combo_box_value(anchor_combo);
                } else {
                    label.hide();
                    main_combo.hide();
                    anchor_combo.hide();
                    action_label.hide();
                    pick_button.hide();
                }
            }
            Virtual => {
                label.show();
                main_combo.show();
                action_label.hide();
                anchor_combo.hide();
                pick_button.hide();
                label.set_text("Number");
                main_combo.fill_combo_box_small(1..=100);
                main_combo
                    .select_combo_box_item_by_index(self.target.control_element_index.get() as _);
            }
        };
    }

    fn fill_target_track_combo_box(&self, combo: Window) {
        let mut v = vec![
            (-3isize, VirtualTrack::This.to_string()),
            (-2isize, VirtualTrack::Selected.to_string()),
            (-1isize, VirtualTrack::Master.to_string()),
        ];
        let project = self.target_with_context().project();
        let mut current_folder_level: i32 = 0;
        let particular_tracks = project.tracks().enumerate().map(|(i, track)| {
            let indentation = ".".repeat(current_folder_level.abs() as usize * 4);
            let space = if indentation.is_empty() { "" } else { " " };
            let name = track.name().expect("non-master track must have name");
            let label = format!("{}. {}{}{}", i + 1, indentation, space, name.to_str());
            current_folder_level += track.folder_depth_change();
            (i as isize, label)
        });
        v.extend(particular_tracks);
        combo.fill_combo_box_with_data_vec(v);
    }

    fn fill_target_transport_action_combo_box(&self, combo: Window) {
        combo.fill_combo_box(TransportAction::into_enum_iter());
    }

    fn fill_target_bookmark_combo_box(&self, combo: Window) {
        let project = self.target_with_context().project();
        let bookmark_type = self.target.bookmark_type.get();
        let bookmarks = project
            .bookmarks()
            .map(|b| (b, b.basic_info()))
            .filter(|(_, info)| info.bookmark_type() == bookmark_type)
            .enumerate()
            .map(|(i, (b, info))| {
                let name = b.name();
                let label = get_bookmark_label(i as _, info.id, &name);
                (info.id.get() as isize, label)
            })
            .collect();
        combo.fill_combo_box_with_data_vec(bookmarks);
    }

    fn target_with_context(&'a self) -> TargetModelWithContext<'a> {
        self.mapping
            .target_model
            .with_context(self.session.context())
    }

    fn set_target_bookmark_anchor_combo_box_value(&self, b: Window) {
        let anchor = self.mapping.target_model.bookmark_anchor_type.get();
        b.select_combo_box_item_by_index(anchor.into());
    }

    fn invalidate_target_track_combo_box_value(&self, track_combo: Window, anchor_combo: Window) {
        use VirtualTrack::*;
        let virtual_track = self.target.track.get_ref();
        let (track_item_data, anchor): (Option<isize>, Option<&TrackAnchor>) = match virtual_track {
            This => (Some(-3), None),
            Selected => (Some(-2), None),
            Master => (Some(-1), None),
            Particular(anchor) => {
                if let Ok(track) =
                    anchor.resolve(self.session.context().project_or_current_project())
                {
                    let track_item_data = track.index().map(|i| i as isize).unwrap_or(-1);
                    (Some(track_item_data), Some(anchor))
                } else {
                    (None, Some(anchor))
                }
            }
        };
        // Track combo box
        if let Some(d) = track_item_data {
            track_combo.select_combo_box_item_by_data(d).unwrap();
        } else {
            let text = format!("<Not present> ({})", anchor.expect("can't happen"));
            track_combo.select_new_combo_box_item(text.as_str());
        }
        // Anchor combo box
        if let Some(a) = anchor {
            let anchor_type = TrackAnchorType::from_anchor(a);
            anchor_combo.show();
            anchor_combo.select_combo_box_item_by_index(anchor_type.into());
        } else {
            anchor_combo.hide();
            // We should at least initialize it so that it has a value. It's used for updating.
            anchor_combo.select_combo_box_item_by_index(0);
        }
    }

    fn set_target_transport_action_combo_box_value(&self, combo: Window) {
        combo.select_combo_box_item_by_index(
            self.mapping.target_model.transport_action.get().into(),
        );
    }

    fn set_target_bookmark_combo_box_value(&self, combo: Window) {
        let bookmark_ref = self.mapping.target_model.bookmark_ref.get();
        let anchor_type = self.mapping.target_model.bookmark_anchor_type.get();
        let successful = match anchor_type {
            BookmarkAnchorType::Id => combo
                .select_combo_box_item_by_data(bookmark_ref as _)
                .is_ok(),
            BookmarkAnchorType::Index => {
                if (bookmark_ref as usize) < combo.combo_box_item_count() {
                    combo.select_combo_box_item_by_index(bookmark_ref as _);
                    true
                } else {
                    false
                }
            }
        };
        if !successful {
            combo.select_new_combo_box_item(
                get_non_present_bookmark_label(anchor_type, bookmark_ref).as_str(),
            );
        }
    }

    fn invalidate_target_line_three(&self) {
        let main_combo = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_COMBO_BOX);
        let anchor_combo = self
            .view
            .require_control(root::ID_TARGET_FX_ANCHOR_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_FX_OR_SEND_LABEL_TEXT);
        let input_fx_box = self
            .view
            .require_control(root::ID_TARGET_INPUT_FX_CHECK_BOX);
        let target = self.target;
        let hide_all = || {
            label.hide();
            main_combo.hide();
            anchor_combo.hide();
            input_fx_box.hide();
        };
        if target.category.get() != TargetCategory::Reaper {
            hide_all();
            return;
        }
        if target.supports_fx() {
            main_combo.show();
            label.show();
            input_fx_box.show();
            self.fill_target_fx_combo_box(label, main_combo);
            self.invalidate_target_fx_combo_box_value(main_combo, input_fx_box, anchor_combo);
        } else if target.supports_send() {
            main_combo.show();
            anchor_combo.hide();
            label.show();
            input_fx_box.hide();
            self.fill_target_send_combo_box(label, main_combo);
            self.set_target_send_combo_box_value(main_combo);
        } else if target.r#type.get() == ReaperTargetType::Action {
            label.show();
            main_combo.show();
            anchor_combo.hide();
            input_fx_box.hide();
            self.fill_target_invocation_type_combo_box(label, main_combo);
            self.set_target_invocation_type_combo_box_value(main_combo);
        } else if target.r#type.get() == ReaperTargetType::TrackSolo {
            label.show();
            main_combo.show();
            anchor_combo.hide();
            input_fx_box.hide();
            self.fill_target_solo_behavior_combo_box(label, main_combo);
            self.set_target_solo_behavior_combo_box_value(main_combo);
        } else if target.r#type.get() == ReaperTargetType::AutomationTouchState {
            label.show();
            main_combo.show();
            anchor_combo.hide();
            input_fx_box.hide();
            self.fill_target_touched_parameter_type_combo_box(label, main_combo);
            self.set_target_touched_parameter_type_combo_box_value(main_combo);
        } else if target.r#type.get() == ReaperTargetType::GoToBookmark {
            label.hide();
            main_combo.hide();
            anchor_combo.hide();
            input_fx_box.show();
            input_fx_box.set_text("Regions");
            let is_checked = target.bookmark_type.get() == BookmarkType::Region;
            input_fx_box.set_checked(is_checked);
        } else {
            hide_all();
        }
    }

    fn fill_target_send_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Send");
        let track = match self.target_with_context().effective_track().ok() {
            None => {
                combo.clear_combo_box();
                return;
            }
            Some(t) => t,
        };
        let sends = track
            .sends()
            .enumerate()
            .map(|(i, send)| (i as isize, send));
        combo.fill_combo_box_with_data_small(sends);
    }

    fn set_target_send_combo_box_value(&self, combo: Window) {
        let target = self.target;
        match target.send_index.get() {
            None => combo.select_new_combo_box_item("<None>"),
            Some(i) => combo
                .select_combo_box_item_by_data(i as isize)
                .unwrap_or_else(|_| {
                    combo.select_new_combo_box_item(format!("{}. <Not present>", i + 1).as_str());
                }),
        }
    }

    fn fill_target_invocation_type_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Invoke");
        combo.fill_combo_box(ActionInvocationType::into_enum_iter());
    }

    fn set_target_invocation_type_combo_box_value(&self, combo: Window) {
        combo.select_combo_box_item_by_index(self.target.action_invocation_type.get().into());
    }

    fn set_target_solo_behavior_combo_box_value(&self, combo: Window) {
        combo.select_combo_box_item_by_index(self.target.solo_behavior.get().into());
    }

    fn fill_target_solo_behavior_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Behavior");
        combo.fill_combo_box(SoloBehavior::into_enum_iter());
    }

    fn set_target_touched_parameter_type_combo_box_value(&self, combo: Window) {
        combo.select_combo_box_item_by_index(self.target.touched_parameter_type.get().into());
    }

    fn fill_target_touched_parameter_type_combo_box(&self, label: Window, combo: Window) {
        label.set_text("Type");
        combo.fill_combo_box(TouchedParameterType::into_enum_iter());
    }

    fn fill_target_fx_param_combo_box(&self, combo: Window) {
        let fx = match self.target_with_context().fx().ok() {
            None => {
                combo.clear_combo_box();
                return;
            }
            Some(fx) => fx,
        };
        let params: Vec<_> = fx
            .parameters()
            .map(|param| {
                (
                    param.index() as isize,
                    get_fx_param_label(Some(&param), param.index()),
                )
            })
            .collect();
        // TODO-low Just the index would be enough, don't need data.
        combo.fill_combo_box_with_data_vec(params);
    }

    fn set_target_fx_param_combo_box_value(&self, combo: Window) {
        let target = self.target;
        let param_index = target.param_index.get();
        combo
            .select_combo_box_item_by_data(param_index as isize)
            .unwrap_or_else(|_| {
                combo.select_new_combo_box_item(get_fx_param_label(None, param_index).as_ref());
            });
    }

    fn fill_target_track_exclusivity_combo_box(&self, combo: Window) {
        combo.fill_combo_box(TrackExclusivity::into_enum_iter());
    }

    fn set_target_track_exclusivity_combo_box_value(&self, combo: Window) {
        combo.select_combo_box_item_by_index(self.target.track_exclusivity.get().into());
    }

    fn fill_target_fx_combo_box(&self, label: Window, combo: Window) {
        label.set_text("FX");
        let mut v = vec![(
            -1isize,
            format!("{} (ignores track and chain)", VirtualFx::Focused),
        )];
        let fx_chain = {
            if let Ok(track) = self.target_with_context().effective_track() {
                match self.target.fx.get_ref() {
                    None | Some(VirtualFx::Focused) => Some(track.normal_fx_chain()),
                    Some(VirtualFx::Particular { is_input_fx, .. }) => {
                        if *is_input_fx {
                            Some(track.input_fx_chain())
                        } else {
                            Some(track.normal_fx_chain())
                        }
                    }
                }
            } else {
                None
            }
        };
        if let Some(fx_chain) = fx_chain {
            let fxs = fx_chain
                .fxs()
                .enumerate()
                .map(|(i, fx)| (i as isize, get_fx_label(i as u32, &fx)));
            v.extend(fxs);
        }
        combo.fill_combo_box_with_data_vec(v);
    }

    fn invalidate_target_fx_combo_box_value(
        &self,
        combo: Window,
        input_fx_box: Window,
        anchor_combo: Window,
    ) {
        // FX combo box
        let (is_input_fx, anchor) = match self.target.fx.get_ref() {
            None => {
                combo.select_new_combo_box_item("<None>");
                (false, None)
            }
            Some(virtual_fx) => match virtual_fx {
                VirtualFx::Focused => {
                    let _ = combo.select_combo_box_item_by_data(-1);
                    (false, None)
                }
                VirtualFx::Particular {
                    anchor,
                    is_input_fx,
                } => {
                    let successfully_selected_item =
                        match self.target_with_context().fx().ok().map(|fx| fx.index()) {
                            None => false,
                            Some(index) => {
                                combo.select_combo_box_item_by_data(index as isize).is_ok()
                            }
                        };
                    if !successfully_selected_item {
                        let label = get_optional_fx_label(anchor, None);
                        combo.select_new_combo_box_item(label.as_str());
                    }
                    (*is_input_fx, Some(anchor.clone()))
                }
            },
        };
        // Anchor combo box
        if let Some(a) = anchor {
            let anchor_type = FxAnchorType::from_anchor(&a);
            anchor_combo.show();
            anchor_combo.select_combo_box_item_by_index(anchor_type.into());
        } else {
            anchor_combo.hide();
            // We should at least initialize it so that it has a value. It's used for updating.
            anchor_combo.select_combo_box_item_by_index(0);
        }
        // Input FX checkbox
        let label = if let VirtualTrack::Master = self.mapping.target_model.track.get_ref() {
            "Monitoring FX"
        } else {
            "Input FX"
        };
        input_fx_box.set_text(label);
        input_fx_box.set_checked(is_input_fx);
    }

    fn invalidate_target_only_if_fx_has_focus_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_FX_FOCUS_CHECK_BOX);
        let target = self.target;
        if target.supports_fx() {
            if let Some(fx) = target.fx.get_ref().as_ref() {
                if matches!(fx, VirtualFx::Focused) {
                    b.hide();
                } else {
                    b.show();
                    b.set_text("FX must have focus");
                    b.set_checked(target.enable_only_if_fx_has_focus.get());
                }
            } else {
                b.hide();
            }
        } else {
            b.hide();
        }
    }

    fn invalidate_target_only_if_track_is_selected_check_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_TRACK_SELECTED_CHECK_BOX);
        let target = self.target;
        if target.supports_track() && !matches!(target.track.get_ref(), VirtualTrack::Selected) {
            b.show();
            b.set_checked(target.enable_only_if_track_selected.get());
        } else {
            b.hide();
        }
    }

    fn invalidate_target_line_four(&self) {
        let combo = self
            .view
            .require_control(root::ID_TARGET_FX_PARAMETER_COMBO_BOX);
        let label = self
            .view
            .require_control(root::ID_TARGET_FX_PARAMETER_LABEL_TEXT);
        let button = self
            .view
            .require_control(root::ID_TARGET_TAKE_SNAPSHOT_BUTTON);
        let value_text = self.view.require_control(root::ID_TARGET_SNAPSHOT_NAME);
        let target = self.target;
        let hide_all = || {
            combo.hide();
            label.hide();
            button.hide();
            value_text.hide();
        };
        if target.category.get() == TargetCategory::Reaper {
            match target.r#type.get() {
                ReaperTargetType::FxParameter => {
                    label.set_text("Parameter");
                    label.show();
                    combo.show();
                    button.hide();
                    value_text.hide();
                    self.fill_target_fx_param_combo_box(combo);
                    self.set_target_fx_param_combo_box_value(combo);
                }
                ReaperTargetType::LoadFxSnapshot => {
                    label.set_text("Snapshot");
                    label.show();
                    combo.hide();
                    button.show();
                    let snapshot_label = if let Some(snapshot) = self.target.fx_snapshot.get_ref() {
                        snapshot.to_string()
                    } else {
                        "<Empty>".to_owned()
                    };
                    value_text.set_text(snapshot_label);
                    value_text.show();
                }
                _ if target.supports_track_exclusivity() => {
                    label.set_text("Exclusive");
                    label.show();
                    combo.show();
                    button.hide();
                    value_text.hide();
                    self.fill_target_track_exclusivity_combo_box(combo);
                    self.set_target_track_exclusivity_combo_box_value(combo);
                }
                _ => {
                    hide_all();
                }
            }
        } else {
            hide_all();
        }
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
            .select_combo_box_item_by_index(self.mode.r#type.get().into());
    }

    fn invalidate_mode_control_appearance(&self) {
        self.invalidate_mode_control_labels();
        self.invalidate_mode_control_visibilities();
    }

    fn mapping_uses_step_counts(&self) -> bool {
        self.mapping
            .with_context(self.session.context())
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
            .select_combo_box_item_by_index(self.mode.out_of_range_behavior.get().into());
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
                .track
                .changed()
                .merge(target.control_element_index.changed()),
            |view| {
                view.invalidate_target_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel
            .when_do_sync(target.transport_action.changed(), |view| {
                view.invalidate_target_line_two();
            });
        self.panel.when_do_sync(target.fx.changed(), |view| {
            view.invalidate_target_line_three();
            view.invalidate_target_line_four();
            view.invalidate_target_value_controls();
            view.invalidate_target_only_if_fx_has_focus_check_box();
            view.invalidate_mode_controls();
        });
        self.panel
            .when_do_sync(target.param_index.changed(), |view| {
                view.invalidate_target_line_four();
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            });
        self.panel.when_do_sync(target.action.changed(), |view| {
            view.invalidate_target_line_two();
        });
        self.panel.when_do_sync(
            target
                .bookmark_ref
                .changed()
                .merge(target.bookmark_type.changed())
                .merge(target.bookmark_anchor_type.changed()),
            |view| {
                view.invalidate_target_line_two();
            },
        );
        self.panel
            .when_do_sync(target.action_invocation_type.changed(), |view| {
                view.invalidate_target_line_three();
                view.invalidate_mode_controls();
            });
        self.panel.when_do_sync(
            target
                .solo_behavior
                .changed()
                .merge(target.touched_parameter_type.changed()),
            |view| {
                view.invalidate_target_line_three();
            },
        );
        self.panel
            .when_do_sync(target.fx_snapshot.changed(), |view| {
                view.invalidate_target_line_four();
            });
        self.panel
            .when_do_sync(target.track_exclusivity.changed(), |view| {
                view.invalidate_target_line_four();
                view.invalidate_mode_controls();
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

    fn fill_track_anchor_combo_box(&self, b: Window) {
        b.fill_combo_box(TrackAnchorType::into_enum_iter());
    }

    fn fill_target_bookmark_anchor_combo_box(&self, b: Window) {
        b.fill_combo_box(BookmarkAnchorType::into_enum_iter());
    }

    fn fill_target_fx_anchor_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_FX_ANCHOR_COMBO_BOX);
        b.fill_combo_box(FxAnchorType::into_enum_iter());
    }

    fn fill_target_category_combo_box(&self) {
        let b = self
            .view
            .require_control(root::ID_TARGET_CATEGORY_COMBO_BOX);
        b.fill_combo_box(TargetCategory::into_enum_iter());
    }

    fn fill_source_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_TYPE_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category.get() {
            Midi => b.fill_combo_box(MidiSourceType::into_enum_iter()),
            Virtual => b.fill_combo_box(VirtualControlElementType::into_enum_iter()),
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
                combo.fill_combo_box(SourceCharacter::into_enum_iter());
            }
            Osc => {
                combo.fill_combo_box(OscTypeTag::into_enum_iter());
            }
            Virtual => {}
        }
    }

    fn fill_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .fill_combo_box(MidiClockTransportMessage::into_enum_iter());
    }

    fn fill_mode_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_MODE_COMBO_BOX)
            .fill_combo_box(AbsoluteMode::into_enum_iter());
    }

    fn fill_mode_out_of_range_behavior_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_OUT_OF_RANGE_COMBOX_BOX)
            .fill_combo_box(OutOfRangeBehavior::into_enum_iter());
    }

    fn fill_target_type_combo_box(&self) {
        let b = self.view.require_control(root::ID_TARGET_TYPE_COMBO_BOX);
        use TargetCategory::*;
        match self.target.category.get() {
            Reaper => {
                b.fill_combo_box(ReaperTargetType::into_enum_iter());
            }
            Virtual => b.fill_combo_box(VirtualControlElementType::into_enum_iter()),
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
        use root::*;
        match resource_id {
            // Mapping
            ID_MAPPING_PREVENT_ECHO_FEEDBACK_CHECK_BOX => {
                self.write(|p| p.update_mapping_prevent_echo_feedback())
            }
            ID_MAPPING_SEND_FEEDBACK_AFTER_CONTROL_CHECK_BOX => {
                self.write(|p| p.update_mapping_send_feedback_after_control())
            }
            ID_MAPPING_ADVANCED_BUTTON => {
                self.edit_advanced_settings();
            }
            ID_MAPPING_FIND_IN_LIST_BUTTON => {
                self.force_scroll_to_mapping_in_main_panel();
            }
            // IDCANCEL is escape button
            ID_MAPPING_PANEL_OK | raw::IDCANCEL => {
                self.hide();
            }
            // Source
            ID_SOURCE_LEARN_BUTTON => self.toggle_learn_source(),
            ID_SOURCE_RPN_CHECK_BOX => self.write(|p| p.update_source_is_registered()),
            ID_SOURCE_14_BIT_CHECK_BOX => self.write(|p| p.update_source_is_14_bit()),
            // Mode
            ID_SETTINGS_ROTATE_CHECK_BOX => self.write(|p| p.update_mode_rotate()),
            ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX => self.write(|p| p.update_mode_make_absolute()),
            ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX => {
                self.write(|p| p.update_mode_round_target_value())
            }
            ID_SETTINGS_SCALE_MODE_CHECK_BOX => self.write(|p| p.update_mode_approach()),
            ID_SETTINGS_REVERSE_CHECK_BOX => self.write(|p| p.update_mode_reverse()),
            ID_SETTINGS_RESET_BUTTON => self.write(|p| p.reset_mode()),
            // Target
            ID_TARGET_INPUT_FX_CHECK_BOX => self.write(|p| p.update_target_is_input_fx()),
            ID_TARGET_FX_FOCUS_CHECK_BOX => self.write(|p| p.update_target_only_if_fx_has_focus()),
            ID_TARGET_TRACK_SELECTED_CHECK_BOX => {
                self.write(|p| p.update_target_only_if_track_is_selected())
            }
            ID_TARGET_LEARN_BUTTON => self.write(|p| p.toggle_learn_target()),
            ID_TARGET_OPEN_BUTTON => self.write(|p| p.open_target()),
            ID_TARGET_PICK_ACTION_BUTTON => {
                self.read(|p| p.pick_action()).unwrap();
            }
            ID_TARGET_TAKE_SNAPSHOT_BUTTON => {
                let _ = self.take_snapshot();
            }
            _ => unreachable!(),
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            // Source
            ID_SOURCE_CATEGORY_COMBO_BOX => self.write(|p| p.update_source_category()),
            ID_SOURCE_TYPE_COMBO_BOX => self.write(|p| p.update_source_type()),
            ID_SOURCE_CHANNEL_COMBO_BOX => {
                self.write(|p| p.update_source_channel_or_control_element())
            }
            ID_SOURCE_NUMBER_COMBO_BOX => self.write(|p| p.update_source_midi_message_number()),
            ID_SOURCE_CHARACTER_COMBO_BOX => self.write(|p| p.update_source_character()),
            ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX => {
                self.write(|p| p.update_source_midi_clock_transport_message_type())
            }
            // Mode
            ID_SETTINGS_MODE_COMBO_BOX => self.write(|p| p.update_mode_type()),
            ID_MODE_OUT_OF_RANGE_COMBOX_BOX => {
                self.write(|p| p.update_mode_out_of_range_behavior())
            }
            // Target
            ID_TARGET_CATEGORY_COMBO_BOX => self.write(|p| p.update_target_category()),
            ID_TARGET_TYPE_COMBO_BOX => self.write(|p| p.update_target_type()),
            ID_TARGET_TRACK_OR_COMMAND_COMBO_BOX => {
                self.write(|p| p.update_target_line_two_data()).unwrap();
            }
            ID_TARGET_TRACK_ANCHOR_COMBO_BOX => {
                self.write(|p| p.update_target_line_two_anchor()).unwrap();
            }
            ID_TARGET_FX_OR_SEND_COMBO_BOX | ID_TARGET_FX_ANCHOR_COMBO_BOX => {
                self.write(|p| p.update_target_from_combo_box_line_three());
            }
            ID_TARGET_FX_PARAMETER_COMBO_BOX => {
                self.write(|p| p.update_target_from_combo_box_line_four())
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
        use root::*;
        match resource_id {
            // Source
            ID_SOURCE_NUMBER_EDIT_CONTROL => {
                view.write(|p| p.update_source_parameter_number_message_number());
            }
            ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL => {
                view.write(|p| p.update_source_pattern());
            }
            // Mode
            ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL => {
                view.write(|p| p.update_mode_eel_control_transformation());
            }
            ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL => {
                view.write(|p| p.update_mode_eel_feedback_transformation());
            }
            // Target
            ID_TARGET_VALUE_EDIT_CONTROL => {
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
