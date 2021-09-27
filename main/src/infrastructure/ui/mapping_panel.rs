use crate::base::{notification, when, Prop};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::{
    EelEditorPanel, ItemProp, MainPanel, MappingHeaderPanel, YamlEditorPanel,
};

use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    check_mode_applicability, format_percentage_without_unit, AbsoluteMode, AbsoluteValue,
    ButtonUsage, ControlValue, DetailedSourceCharacter, EncoderUsage, FireMode, GroupInteraction,
    MidiClockTransportMessage, ModeApplicabilityCheckInput, ModeParameter, OscTypeTag,
    OutOfRangeBehavior, PercentIo, SoftSymmetricUnitValue, SourceCharacter, TakeoverMode, Target,
    UnitValue, ValueSequence,
};
use helgoboss_midi::{Channel, ShortMessageType, U7};
use reaper_high::{
    BookmarkType, Fx, FxChain, Project, Reaper, SendPartnerType, Track, TrackRoutePartner,
};
use reaper_low::raw;
use reaper_medium::{InitialAction, PromptForActionResult, SectionId};
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::convert::TryInto;

use std::iter;

use std::ptr::null;
use std::rc::Rc;

use crate::application::{
    convert_factor_to_unit_value, convert_unit_value_to_factor, get_bookmark_label, get_fx_label,
    get_fx_param_label, get_non_present_bookmark_label, get_optional_fx_label, get_route_label,
    AutomationModeOverrideType, BookmarkAnchorType, ConcreteFxInstruction,
    ConcreteTrackInstruction, MappingModel, MidiSourceType, ModeModel, RealearnAutomationMode,
    RealearnTrackArea, ReaperSourceType, ReaperTargetType, Session, SharedMapping, SharedSession,
    SourceCategory, SourceModel, TargetCategory, TargetModel, TargetModelWithContext, TargetUnit,
    TrackRouteSelectorType, VirtualControlElementType, VirtualFxParameterType, VirtualFxType,
    VirtualTrackType, WeakSession,
};
use crate::base::Global;
use crate::domain::{
    control_element_domains, ClipInfo, ControlContext, Exclusivity, FeedbackSendBehavior,
    SendMidiDestination, SimpleExclusivity, SlotContent, WithControlContext, CLIP_SLOT_COUNT,
};
use crate::domain::{
    get_non_present_virtual_route_label, get_non_present_virtual_track_label,
    resolve_track_route_by_index, ActionInvocationType, CompoundMappingTarget,
    ExtendedProcessorContext, FeedbackResolution, FxDisplayType, MappingCompartment,
    QualifiedMappingId, RealearnTarget, ReaperTarget, SoloBehavior, TargetCharacter,
    TouchedParameterType, TrackExclusivity, TrackRouteType, TransportAction, VirtualControlElement,
    VirtualControlElementId, VirtualFx,
};
use itertools::Itertools;

use crate::domain::ui_util::parse_unit_value_from_percentage;
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::util::{
    format_tags_as_csv, open_in_browser, parse_tags_from_csv, symbols,
};
use std::collections::HashMap;
use std::time::Duration;
use swell_ui::{
    DialogUnits, MenuBar, Point, SharedView, SwellStringArg, View, ViewContext, WeakView, Window,
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
    session: &'a Session,
    mapping: &'a mut MappingModel,
    shared_mapping: &'a SharedMapping,
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
        let category = mapping.borrow().target_model.category.get();
        match category {
            TargetCategory::Reaper => {
                self.write(|p| p.handle_target_line_2_button_press());
            }
            TargetCategory::Virtual => {
                let control_element_type = mapping.borrow().target_model.control_element_type.get();
                let window = self.view.require_window();
                let text = prompt_for_predefined_control_element_name(
                    window,
                    control_element_type,
                    &HashMap::new(),
                )
                .ok_or("nothing picked")?;
                mapping
                    .borrow_mut()
                    .target_model
                    .control_element_id
                    .set(text.parse().unwrap_or_default());
            }
        };
        Ok(())
    }

    fn handle_target_line_3_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let target_type = mapping.borrow().target_model.r#type.get();
        match target_type {
            ReaperTargetType::SendMidi => {
                if let Some(preset) =
                    prompt_for_predefined_raw_midi_pattern(self.view.require_window())
                {
                    mapping
                        .borrow_mut()
                        .target_model
                        .raw_midi_pattern
                        .set(preset);
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
                    let slot_index = mapping.target_model.slot_index.get();
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
                    let slot_index = self.mapping().borrow().target_model.slot_index.get();
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
        let control_element_type = mapping.borrow().source_model.control_element_type.get();
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
        mapping
            .borrow_mut()
            .source_model
            .control_element_id
            .set(text.parse().unwrap_or_default());
        Ok(())
    }

    fn handle_target_line_4_button_press(&self) -> Result<(), &'static str> {
        let mapping = self.displayed_mapping().ok_or("no mapping set")?;
        let target_type = mapping.borrow().target_model.r#type.get();
        if target_type == ReaperTargetType::LoadFxSnapshot {
            // Important that neither session nor mapping is mutably borrowed while doing this
            // because state of our ReaLearn instance is not unlikely to be
            // queried as well!
            let compartment = mapping.borrow().compartment();
            let fx_snapshot = mapping
                .borrow()
                .target_model
                .take_fx_snapshot(self.session().borrow().extended_context(), compartment)?;
            mapping
                .borrow_mut()
                .target_model
                .fx_snapshot
                .set(Some(fx_snapshot));
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
        self.edit_eel(
            |m| m.source_model.midi_script.get_ref().clone(),
            |m, eel| m.source_model.midi_script.set(eel),
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
        self.edit_yaml(
            |m| m.advanced_settings().cloned(),
            |m, yaml| m.set_advanced_settings(yaml, true),
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
                    .map(|m| m.borrow().target_model.unit.get())
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
        let session = shared_session.borrow();
        let mut shared_mapping = self.mapping.borrow_mut();
        let shared_mapping = shared_mapping.as_mut().expect("mapping not filled");
        let mut mapping = shared_mapping.borrow_mut();
        let mut p = MutableMappingPanel {
            session: &session,
            mapping: &mut mapping,
            shared_mapping,
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

    fn handle_target_line_2_button_press(&mut self) {
        match self.target_category() {
            TargetCategory::Reaper => match self.reaper_target_type() {
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
                        (Some(mi), Some(ri)) => match self.mapping.target_model.bookmark_type.get()
                        {
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
        self.mapping.feedback_send_behavior.set(behavior);
    }

    fn update_mapping_is_enabled(&mut self) {
        self.mapping.is_enabled.set(
            self.view
                .require_control(root::IDC_MAPPING_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mapping_is_visible_in_projection(&mut self) {
        self.mapping.visible_in_projection.set(
            self.view
                .require_control(root::ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_hint(&self, mode_parameter: ModeParameter) {
        self.panel
            .last_touched_mode_parameter
            .borrow_mut()
            .set(Some(mode_parameter));
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
            Reaper | Virtual | Never => {}
        };
    }

    #[allow(clippy::single_match)]
    fn update_source_channel(&mut self) {
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
            Reaper | Virtual | Never => {}
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
            Reaper => self
                .mapping
                .source_model
                .reaper_source_type
                .set(i.try_into().expect("invalid REAPER source type")),
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
        let edit_control_id = root::ID_SOURCE_NUMBER_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        let text = c.text().unwrap_or_default();
        use SourceCategory::*;
        match self.mapping.source_model.category.get() {
            Midi => {
                let value = text.parse().ok();
                self.mapping
                    .source_model
                    .parameter_number_message_number
                    .set_with_initiator(value, Some(edit_control_id));
            }
            Osc => {
                let value = parse_osc_arg_index(&text);
                self.mapping
                    .source_model
                    .osc_arg_index
                    .set_with_initiator(value, Some(edit_control_id));
            }
            Virtual => {
                self.mapping
                    .source_model
                    .control_element_id
                    .set_with_initiator(text.parse().unwrap_or_default(), Some(edit_control_id));
            }
            Reaper | Never => {}
        };
    }

    fn update_source_pattern(&mut self) {
        let edit_control_id = root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL;
        let c = self.view.require_control(edit_control_id);
        if let Ok(value) = c.text() {
            use SourceCategory::*;
            match self.mapping.source_model.category.get() {
                Midi => match self.mapping.source_model.midi_source_type.get() {
                    MidiSourceType::Raw => {
                        self.mapping
                            .source_model
                            .raw_midi_pattern
                            .set_with_initiator(value, Some(edit_control_id));
                    }
                    MidiSourceType::Script => {
                        self.mapping
                            .source_model
                            .midi_script
                            .set_with_initiator(value, Some(edit_control_id));
                    }
                    _ => {}
                },
                Osc => {
                    self.mapping
                        .source_model
                        .osc_address_pattern
                        .set_with_initiator(value, Some(edit_control_id));
                }
                Reaper | Virtual | Never => {}
            }
        }
    }

    fn update_mode_rotate(&mut self) {
        self.update_mode_hint(ModeParameter::Rotate);
        self.mapping.mode_model.rotate.set(
            self.view
                .require_control(root::ID_SETTINGS_ROTATE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_mode_make_absolute(&mut self) {
        self.update_mode_hint(ModeParameter::MakeAbsolute);
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
        self.update_mode_hint(ModeParameter::SpecificOutOfRangeBehavior(behavior));
        self.mapping.mode_model.out_of_range_behavior.set(behavior);
    }

    fn update_mode_group_interaction(&mut self) {
        let interaction = self
            .view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid group interaction");
        self.update_mode_hint(ModeParameter::SpecificGroupInteraction(interaction));
        self.mapping.mode_model.group_interaction.set(interaction);
    }

    fn update_mode_fire_mode(&mut self) {
        let mode = self
            .view
            .require_control(root::ID_MODE_FIRE_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid fire mode");
        self.update_mode_hint(ModeParameter::SpecificFireMode(mode));
        self.mapping.mode_model.fire_mode.set(mode);
    }

    fn update_mode_round_target_value(&mut self) {
        self.update_mode_hint(ModeParameter::RoundTargetValue);
        self.mapping.mode_model.round_target_value.set(
            self.view
                .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_takeover_mode(&mut self) {
        self.update_mode_hint(ModeParameter::TakeoverMode);
        let mode = self
            .view
            .require_control(root::ID_MODE_TAKEOVER_MODE)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid takeover mode");
        self.mapping.mode_model.takeover_mode.set(mode);
    }

    fn update_button_usage(&mut self) {
        self.update_mode_hint(ModeParameter::ButtonFilter);
        let mode = self
            .view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid button usage");
        self.mapping.mode_model.button_usage.set(mode);
    }

    fn update_encoder_usage(&mut self) {
        self.update_mode_hint(ModeParameter::RelativeFilter);
        let mode = self
            .view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid encoder usage");
        self.mapping.mode_model.encoder_usage.set(mode);
    }

    fn update_mode_reverse(&mut self) {
        self.update_mode_hint(ModeParameter::Reverse);
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
        let mode = b
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid mode type");
        self.update_mode_hint(ModeParameter::SpecificAbsoluteMode(mode));
        self.mapping.mode_model.r#type.set(mode);
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
            .set_with_with_initiator(
                |prev| prev.with_min(value),
                Some(root::ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL),
            );
    }

    fn get_value_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.first_resolved_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        let control_context = self.session.control_context();
        match self.mapping.target_model.unit.get() {
            TargetUnit::Native => target.parse_as_value(text.as_str(), control_context).ok(),
            TargetUnit::Percent => parse_unit_value_from_percentage(&text).ok(),
        }
    }

    fn get_step_size_from_target_edit_control(&self, edit_control_id: u32) -> Option<UnitValue> {
        let target = self.first_resolved_target()?;
        let text = self.view.require_control(edit_control_id).text().ok()?;
        let control_context = self.session.control_context();
        match self.mapping.target_model.unit.get() {
            TargetUnit::Native => target
                .parse_as_step_size(text.as_str(), control_context)
                .ok(),
            TargetUnit::Percent => parse_unit_value_from_percentage(&text).ok(),
        }
    }

    fn update_mode_max_target_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_target_edit_control(root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with_with_initiator(
                |prev| prev.with_max(value),
                Some(root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL),
            );
    }

    fn update_mode_min_jump_from_edit_control(&mut self) {
        let value = self
            .get_step_size_from_target_edit_control(root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .jump_interval
            .set_with_with_initiator(
                |prev| prev.with_min(value),
                Some(root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL),
            );
    }

    fn update_mode_max_jump_from_edit_control(&mut self) {
        let value = self
            .get_step_size_from_target_edit_control(root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .jump_interval
            .set_with_with_initiator(
                |prev| prev.with_max(value),
                Some(root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL),
            );
    }

    fn update_mode_min_source_value_from_edit_control(&mut self) {
        let value = self
            .get_value_from_source_edit_control(root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MIN);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with_with_initiator(
                |prev| prev.with_min(value),
                Some(root::ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL),
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
        let value = self
            .get_value_from_source_edit_control(root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL)
            .unwrap_or(UnitValue::MAX);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with_with_initiator(
                |prev| prev.with_max(value),
                Some(root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL),
            );
    }

    fn update_mode_min_step_from_edit_control(&mut self) {
        let value = self
            .get_value_from_step_edit_control(root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL)
            .unwrap_or_else(|| UnitValue::MIN.to_symmetric());
        self.mapping
            .mode_model
            .step_interval
            .set_with_with_initiator(
                |prev| prev.with_min(value),
                Some(root::ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL),
            );
    }

    fn handle_mode_fire_line_2_edit_control_change(&mut self) {
        let value = self
            .get_value_from_duration_edit_control(root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL)
            .unwrap_or_else(|| Duration::from_millis(0));
        self.mapping
            .mode_model
            .press_duration_interval
            .set_with_with_initiator(
                |prev| prev.with_min(value),
                Some(root::ID_MODE_FIRE_LINE_2_EDIT_CONTROL),
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
        let value = self
            .get_value_from_step_edit_control(root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL)
            .unwrap_or(SoftSymmetricUnitValue::SOFT_MAX);
        self.mapping
            .mode_model
            .step_interval
            .set_with_with_initiator(
                |prev| prev.with_max(value),
                Some(root::ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL),
            );
    }

    fn handle_mode_fire_line_3_edit_control_change(&mut self) {
        let value = self
            .get_value_from_duration_edit_control(root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL)
            .unwrap_or_else(|| Duration::from_millis(0));
        self.handle_mode_fire_line_3_duration_change(
            value,
            Some(root::ID_MODE_FIRE_LINE_3_EDIT_CONTROL),
        );
    }

    fn update_mode_target_value_sequence(&mut self) {
        self.update_mode_hint(ModeParameter::TargetValueSequence);
        let text = self
            .view
            .require_control(root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        let sequence = match self.mapping.target_model.unit.get() {
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
        self.mapping
            .mode_model
            .target_value_sequence
            .set_with_initiator(sequence, Some(root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL));
    }

    fn update_mode_eel_control_transformation(&mut self) {
        self.update_mode_hint(ModeParameter::ControlTransformation);
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping
            .mode_model
            .eel_control_transformation
            .set_with_initiator(
                value,
                Some(root::ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL),
            );
    }

    fn update_mode_eel_feedback_transformation(&mut self) {
        self.update_mode_hint(ModeParameter::FeedbackTransformation);
        let value = self
            .view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        self.mapping
            .mode_model
            .eel_feedback_transformation
            .set_with_initiator(
                value,
                Some(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL),
            );
    }

    fn update_mode_min_target_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::TargetMinMax);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_target_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::TargetMinMax);
        self.mapping
            .mode_model
            .target_value_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
    }

    fn update_mode_min_source_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::SourceMinMax);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_source_value_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::SourceMinMax);
        self.mapping
            .mode_model
            .source_value_interval
            .set_with(|prev| prev.with_max(slider.slider_unit_value()));
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
        self.mapping
            .mode_model
            .step_interval
            .set_with(|prev| prev.with_min(value));
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
        self.mapping
            .mode_model
            .step_interval
            .set_with(|prev| prev.with_max(value));
    }

    fn handle_mode_fire_line_2_slider_change(&mut self, slider: Window) {
        self.mapping
            .mode_model
            .press_duration_interval
            .set_with(|prev| prev.with_min(slider.slider_duration()));
    }

    fn handle_mode_fire_line_3_slider_change(&mut self, slider: Window) {
        let value = slider.slider_duration();
        self.handle_mode_fire_line_3_duration_change(value, None);
    }

    fn handle_mode_fire_line_3_duration_change(&mut self, value: Duration, initiator: Option<u32>) {
        match self.mapping.mode_model.fire_mode.get() {
            FireMode::WhenButtonReleased | FireMode::OnSinglePress | FireMode::OnDoublePress => {
                self.mapping
                    .mode_model
                    .press_duration_interval
                    .set_with_with_initiator(|prev| prev.with_max(value), initiator);
            }
            FireMode::AfterTimeout => {}
            FireMode::AfterTimeoutKeepFiring => {
                self.mapping
                    .mode_model
                    .turbo_rate
                    .set_with_initiator(value, initiator);
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
        self.mapping
            .mode_model
            .jump_interval
            .set_with(|prev| prev.with_min(slider.slider_unit_value()));
    }

    fn update_mode_max_jump_from_slider(&mut self, slider: Window) {
        self.update_mode_hint(ModeParameter::JumpMinMax);
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
                t if t.supports_fx_chain() => {
                    self.mapping.target_model.fx_is_input_fx.set(is_checked);
                }
                t if t.supports_track_scrolling() => {
                    self.mapping
                        .target_model
                        .scroll_arrange_view
                        .set(is_checked);
                }
                ReaperTargetType::Seek => {
                    self.mapping.target_model.seek_play.set(is_checked);
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
                    self.mapping
                        .target_model
                        .enable_only_if_track_selected
                        .set(is_checked);
                }
                t if t.supports_track_scrolling() => {
                    self.mapping.target_model.scroll_mixer.set(is_checked);
                }
                ReaperTargetType::Seek => {
                    self.mapping.target_model.move_view.set(is_checked);
                }
                ReaperTargetType::LoadMappingSnapshot => {
                    self.mapping
                        .target_model
                        .active_mappings_only
                        .set(is_checked);
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
                    self.mapping
                        .target_model
                        .enable_only_if_fx_has_focus
                        .set(is_checked);
                }
                ReaperTargetType::Seek => {
                    self.mapping.target_model.use_project.set(is_checked);
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
                    self.mapping.target_model.use_regions.set(is_checked);
                }
                ReaperTargetType::ClipTransport => {
                    self.mapping.target_model.next_bar.set(is_checked);
                }
                t if t.supports_poll_for_feedback() => {
                    self.mapping.target_model.poll_for_feedback.set(is_checked);
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
                    self.mapping.target_model.use_loop_points.set(is_checked);
                }
                ReaperTargetType::ClipTransport => {
                    self.mapping.target_model.buffered.set(is_checked);
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
                    self.mapping.target_model.use_time_selection.set(is_checked);
                }
                _ => {}
            },
            TargetCategory::Virtual => {}
        }
    }

    fn handle_target_unit_button_press(&mut self) {
        use TargetUnit::*;
        let next_unit = match self.mapping.target_model.unit.get() {
            Native => Percent,
            Percent => Native,
        };
        self.mapping.target_model.unit.set(next_unit);
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
        use TargetCategory::*;
        match self.mapping.target_model.category.get() {
            Reaper => {
                let data = b.selected_combo_box_item_data() as usize;
                self.mapping
                    .target_model
                    .r#type
                    .set(data.try_into().expect("invalid REAPER target type"))
            }
            Virtual => self.mapping.target_model.control_element_type.set(
                b.selected_combo_box_item_index()
                    .try_into()
                    .expect("invalid virtual target type"),
            ),
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
                    self.mapping
                        .target_model
                        .bookmark_anchor_type
                        .set(bookmark_anchor_type);
                }
                t if t.supports_feedback_resolution() => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .feedback_resolution
                        .set(i.try_into().expect("invalid feedback resolution"));
                }
                _ if self.mapping.target_model.supports_track() => {
                    let track_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping.target_model.set_track_type_from_ui(
                        track_type,
                        self.session.context(),
                        Some(combo_id),
                    );
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
                    self.mapping.target_model.slot_index.set(slot_index);
                }
                t if t.supports_fx() => {
                    let fx_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping.target_model.set_fx_type_from_ui(
                        fx_type,
                        self.session.extended_context(),
                        self.mapping.compartment(),
                    );
                }
                t if t.supports_send() => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .route_type
                        .set(i.try_into().expect("invalid route type"));
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
                    self.mapping
                        .target_model
                        .osc_arg_type_tag
                        .set(i.try_into().expect("invalid OSC type tag"));
                }
                ReaperTargetType::FxParameter => {
                    let param_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping.target_model.param_type.set(param_type);
                }
                ReaperTargetType::NavigateWithinGroup => {
                    let exclusivity: SimpleExclusivity = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping
                        .target_model
                        .exclusivity
                        .set(exclusivity.into());
                }
                t if t.supports_exclusivity() => {
                    let exclusivity = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping.target_model.exclusivity.set(exclusivity);
                }
                t if t.supports_send() => {
                    let selector_type = combo
                        .selected_combo_box_item_index()
                        .try_into()
                        .unwrap_or_default();
                    self.mapping
                        .target_model
                        .route_selector_type
                        .set(selector_type);
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
                    let value: u32 = match self.mapping.target_model.bookmark_anchor_type.get() {
                        BookmarkAnchorType::Id => combo.selected_combo_box_item_data() as _,
                        BookmarkAnchorType::Index => combo.selected_combo_box_item_index() as _,
                    };
                    self.mapping.target_model.bookmark_ref.set(value);
                }
                ReaperTargetType::AutomationModeOverride => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .automation_mode_override_type
                        .set(i.try_into().expect("invalid automation mode override type"));
                }
                ReaperTargetType::Transport => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .transport_action
                        .set(i.try_into().expect("invalid transport action"));
                }
                ReaperTargetType::NavigateWithinGroup => {
                    let i = combo.selected_combo_box_item_index();
                    let group_id = self
                        .session
                        .find_group_by_index_sorted(self.mapping.compartment(), i)
                        .expect("group not existing")
                        .borrow()
                        .id();
                    self.mapping.target_model.group_id.set(group_id);
                }
                ReaperTargetType::SendMidi => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .send_midi_destination
                        .set(i.try_into().expect("invalid send MIDI destination"));
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
                    self.mapping.target_model.osc_dev_id.set(dev_id);
                }
                _ if self.mapping.target_model.supports_track() => {
                    let project = self.session.context().project_or_current_project();
                    let i = combo.selected_combo_box_item_index();
                    if let Some(track) = project.track_by_index(i as _) {
                        self.mapping.target_model.set_concrete_track(
                            ConcreteTrackInstruction::ByIdWithTrack(track),
                            false,
                            true,
                            Some(combo_id),
                        );
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
                        let chain = if self.mapping.target_model.fx_is_input_fx.get() {
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
                ReaperTargetType::TrackShow => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .track_area
                        .set(i.try_into().expect("invalid track area"));
                }
                ReaperTargetType::TrackAutomationMode
                | ReaperTargetType::AutomationModeOverride => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .track_automation_mode
                        .set(i.try_into().expect("invalid automation mode"));
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
                ReaperTargetType::ClipTransport => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .transport_action
                        .set(i.try_into().expect("invalid transport action"));
                }
                ReaperTargetType::FxParameter => {
                    if let Ok(fx) = self.target_with_context().first_fx() {
                        let i = combo.selected_combo_box_item_index();
                        let param = fx.parameter_by_index(i as _);
                        self.mapping.target_model.param_index.set(i as _);
                        // We also set name so that we can easily switch between types.
                        self.mapping
                            .target_model
                            .param_name
                            // Parameter names are not reliably UTF-8-encoded (e.g. "JS: Stereo Width")
                            .set(param.name().into_inner().to_string_lossy().to_string());
                    }
                }
                t if t.supports_track_exclusivity() => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .track_exclusivity
                        .set(i.try_into().expect("invalid track exclusivity"));
                }
                t if t.supports_fx_display_type() => {
                    let i = combo.selected_combo_box_item_index();
                    self.mapping
                        .target_model
                        .fx_display_type
                        .set(i.try_into().expect("invalid FX display type"));
                }
                t if t.supports_send() => {
                    if let Ok(track) = self.target_with_context().first_effective_track() {
                        let i = combo.selected_combo_box_item_index();
                        let route_type = self.mapping.target_model.route_type.get();
                        if let Ok(route) = resolve_track_route_by_index(&track, route_type, i as _)
                        {
                            if let Some(TrackRoutePartner::Track(t)) = route.partner() {
                                // Track send/receive. We use the partner track ID as stable ID!
                                self.mapping.target_model.route_id.set(Some(*t.guid()));
                            }
                            // We also set index and name. First because hardware output relies on
                            // the index as "ID", but also so we can easily switch between
                            // selector types.
                            self.mapping.target_model.route_index.set(i as _);
                            self.mapping
                                .target_model
                                .route_name
                                .set(route.name().into_string());
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
                    match self.mapping.target_model.track_type.get() {
                        VirtualTrackType::Dynamic => {
                            let expression = control.text().unwrap_or_default();
                            self.mapping
                                .target_model
                                .track_expression
                                .set_with_initiator(expression, Some(edit_control_id));
                        }
                        VirtualTrackType::ByName | VirtualTrackType::AllByName => {
                            let name = control.text().unwrap_or_default();
                            self.mapping
                                .target_model
                                .track_name
                                .set_with_initiator(name, Some(edit_control_id));
                        }
                        VirtualTrackType::ByIndex => {
                            let index = parse_position_as_index(control);
                            self.mapping
                                .target_model
                                .track_index
                                .set_with_initiator(index, Some(edit_control_id));
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            TargetCategory::Virtual => {
                let text = control.text().unwrap_or_default();
                self.mapping
                    .target_model
                    .control_element_id
                    .set_with_initiator(text.parse().unwrap_or_default(), Some(edit_control_id));
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
                    self.mapping
                        .target_model
                        .raw_midi_pattern
                        .set_with_initiator(text, Some(edit_control_id));
                }
                ReaperTargetType::SendOsc => {
                    let pattern = control.text().unwrap_or_default();
                    self.mapping
                        .target_model
                        .osc_address_pattern
                        .set_with_initiator(pattern, Some(edit_control_id));
                }
                t if t.supports_fx() => match self.mapping.target_model.fx_type.get() {
                    VirtualFxType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.mapping
                            .target_model
                            .fx_expression
                            .set_with_initiator(expression, Some(edit_control_id));
                    }
                    VirtualFxType::ByName | VirtualFxType::AllByName => {
                        let name = control.text().unwrap_or_default();
                        self.mapping
                            .target_model
                            .fx_name
                            .set_with_initiator(name, Some(edit_control_id));
                    }
                    VirtualFxType::ByIndex => {
                        let index = parse_position_as_index(control);
                        self.mapping
                            .target_model
                            .fx_index
                            .set_with_initiator(index, Some(edit_control_id));
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
                    self.mapping
                        .target_model
                        .osc_arg_index
                        .set_with_initiator(parse_osc_arg_index(&text), Some(edit_control_id));
                }
                ReaperTargetType::FxParameter => match self.mapping.target_model.param_type.get() {
                    VirtualFxParameterType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.mapping
                            .target_model
                            .param_expression
                            .set_with_initiator(expression, Some(edit_control_id));
                    }
                    VirtualFxParameterType::ByName => {
                        let name = control.text().unwrap_or_default();
                        self.mapping
                            .target_model
                            .param_name
                            .set_with_initiator(name, Some(edit_control_id));
                    }
                    VirtualFxParameterType::ByIndex => {
                        let index = parse_position_as_index(control);
                        self.mapping
                            .target_model
                            .param_index
                            .set_with_initiator(index, Some(edit_control_id));
                    }
                    VirtualFxParameterType::ById => {}
                },
                t if t.supports_send() => match self.mapping.target_model.route_selector_type.get()
                {
                    TrackRouteSelectorType::Dynamic => {
                        let expression = control.text().unwrap_or_default();
                        self.mapping
                            .target_model
                            .route_expression
                            .set_with_initiator(expression, Some(edit_control_id));
                    }
                    TrackRouteSelectorType::ByName => {
                        let name = control.text().unwrap_or_default();
                        self.mapping
                            .target_model
                            .route_name
                            .set_with_initiator(name, Some(edit_control_id));
                    }
                    TrackRouteSelectorType::ByIndex => {
                        let index = parse_position_as_index(control);
                        self.mapping
                            .target_model
                            .route_index
                            .set_with_initiator(index, Some(edit_control_id));
                    }
                    _ => {}
                },
                t if t.supports_tags() => {
                    let text = control.text().unwrap_or_default();
                    self.mapping
                        .target_model
                        .tags
                        .set_with_initiator(parse_tags_from_csv(&text), Some(edit_control_id));
                }
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
        self.fill_source_midi_message_number_combo_box();
        self.fill_source_midi_clock_transport_message_type_combo_box();
        self.fill_mode_out_of_range_behavior_combo_box();
        self.fill_mode_group_interaction_combo_box();
        self.fill_mode_takeover_mode_combo_box();
        self.fill_mode_button_usage_combo_box();
        self.fill_mode_encoder_usage_combo_box();
        self.fill_mode_fire_mode_combo_box();
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
            make_absolute: self.mapping.mode_model.make_absolute.get(),
            source_character,
            absolute_mode: self.mapping.mode_model.r#type.get(),
            mode_parameter,
            target_value_sequence_is_set: !self
                .mapping
                .mode_model
                .target_value_sequence
                .get_ref()
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
            .select_combo_box_item_by_index(self.mapping.feedback_send_behavior.get().into())
            .unwrap();
    }

    fn invalidate_mapping_enabled_check_box(&self) {
        self.view
            .require_control(root::IDC_MAPPING_ENABLED_CHECK_BOX)
            .set_checked(self.mapping.is_enabled.get());
    }

    fn invalidate_mapping_visible_in_projection_check_box(&self) {
        let cb = self
            .view
            .require_control(root::ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX);
        cb.set_checked(self.mapping.visible_in_projection.get());
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
        self.invalidate_source_channel();
        self.invalidate_source_14_bit_check_box();
        self.invalidate_source_is_registered_check_box();
        self.invalidate_source_midi_message_number_controls();
        self.invalidate_source_parameter_number_message_number_controls(None);
        self.invalidate_source_character_combo_box();
        self.invalidate_source_midi_clock_transport_message_type_combo_box();
        self.invalidate_source_osc_address_pattern_edit_control(None);
    }

    fn invalidate_source_control_appearance(&self) {
        self.fill_source_channel_combo_box();
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
                match self.source.midi_source_type.get() {
                    MidiSourceType::Raw => "Pattern",
                    MidiSourceType::Script => "Script",
                    _ => "",
                },
            ),
            Virtual => ("", "ID", "", ""),
            Osc => ("", "Argument", "Type", "Address"),
            Reaper | Never => ("", "", "", ""),
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
            source.supports_channel(),
            &[
                root::ID_SOURCE_CHANNEL_COMBO_BOX,
                root::ID_SOURCE_CHANNEL_LABEL,
            ],
        );
        self.show_if(
            source.supports_midi_message_number()
                || source.supports_parameter_number_message_number()
                || source.is_osc()
                || source.supports_control_element_name(),
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
            source.supports_parameter_number_message_number()
                || source.is_osc()
                || source.supports_control_element_name(),
            &[root::ID_SOURCE_NUMBER_EDIT_CONTROL],
        );
        self.show_if(
            source.supports_midi_message_number(),
            &[root::ID_SOURCE_NUMBER_COMBO_BOX],
        );
        self.show_if(
            source.is_raw_midi() || source.is_midi_script() || source.is_osc(),
            &[
                root::ID_SOURCE_OSC_ADDRESS_LABEL_TEXT,
                root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL,
            ],
        );
        self.show_if(
            source.is_midi_script(),
            &[root::ID_SOURCE_SCRIPT_DETAIL_BUTTON],
        );
        self.show_if(
            source.supports_control_element_name(),
            &[root::ID_SOURCE_LINE_4_BUTTON],
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
            Reaper => self.source.reaper_source_type.get().into(),
            Virtual => self.source.control_element_type.get().into(),
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

    #[allow(clippy::single_match)]
    fn invalidate_source_channel(&self) {
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
            Reaper | Virtual | Never => return,
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

    fn invalidate_source_parameter_number_message_number_controls(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_SOURCE_NUMBER_EDIT_CONTROL) {
            return;
        }
        let c = self
            .view
            .require_control(root::ID_SOURCE_NUMBER_EDIT_CONTROL);
        use SourceCategory::*;
        let text = match self.source.category.get() {
            Midi => match self.source.parameter_number_message_number.get() {
                None => "".to_owned(),
                Some(n) => n.to_string(),
            },
            Osc => format_osc_arg_index(self.source.osc_arg_index.get()),
            Virtual => self.source.control_element_id.get().to_string(),
            Reaper | Never => return,
        };
        c.set_text(text)
    }

    fn invalidate_source_osc_address_pattern_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL) {
            return;
        }
        let c = self
            .view
            .require_control(root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL);
        use SourceCategory::*;
        let (value_text, read_only) = match self.source.category.get() {
            Midi => match self.source.midi_source_type.get() {
                MidiSourceType::Raw => (self.source.raw_midi_pattern.get_ref().as_str(), false),
                MidiSourceType::Script => {
                    let midi_script = self.source.midi_script.get_ref();
                    (
                        midi_script.lines().next().unwrap_or_default(),
                        midi_script.lines().count() > 1,
                    )
                }
                _ => return,
            },
            Osc => (self.source.osc_address_pattern.get_ref().as_str(), false),
            Reaper | Virtual | Never => return,
        };
        c.set_text(value_text);
        c.set_enabled(!read_only);
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
            Reaper | Virtual | Never => return,
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
        let hint = match self.target.category.get() {
            Reaper => {
                let item_data: usize = self.target.r#type.get().into();
                combo
                    .select_combo_box_item_by_data(item_data as isize)
                    .unwrap();
                self.target.r#type.get().hint()
            }
            Virtual => {
                let item_index = self.target.control_element_type.get().into();
                combo.select_combo_box_item_by_index(item_index).unwrap();
                ""
            }
        };
        self.view
            .require_control(root::ID_TARGET_HINT)
            .set_text(hint);
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
                ReaperTargetType::AutomationModeOverride => Some("Behavior"),
                ReaperTargetType::GoToBookmark => match self.target.bookmark_type.get() {
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
            TargetCategory::Reaper => match self.target.r#type.get() {
                _ if self.target.supports_track() => {
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
                ReaperTargetType::LoadMappingSnapshot => {
                    combo.show();
                    combo.select_only_combo_box_item("Initial");
                }
                t if t.supports_feedback_resolution() => {
                    combo.show();
                    combo.fill_combo_box_indexed(FeedbackResolution::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.feedback_resolution.get().into(),
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
                            self.mapping.target_model.transport_action.get().into(),
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
                                .automation_mode_override_type
                                .get()
                                .into(),
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
                    let group_id = self.target.group_id.get();
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
                            self.mapping.target_model.send_midi_destination.get().into(),
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
                    if let Some(dev_id) = self.mapping.target_model.osc_dev_id.get() {
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
                        self.target.track_type.get(),
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
                    let text = match self.target.track_type.get() {
                        VirtualTrackType::Dynamic => self.target.track_expression.get_ref().clone(),
                        VirtualTrackType::ByIndex => {
                            let index = self.target.track_index.get();
                            (index + 1).to_string()
                        }
                        VirtualTrackType::ByName | VirtualTrackType::AllByName => {
                            self.target.track_name.get_ref().clone()
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
                let text = self.target.control_element_id.get().to_string();
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
                ReaperTargetType::Action => Some("Pick!"),
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
                    let text = format_osc_arg_index(self.target.osc_arg_index.get());
                    control.set_text(text.as_str());
                }
                ReaperTargetType::FxParameter => {
                    let text = match self.target.param_type.get() {
                        VirtualFxParameterType::Dynamic => {
                            Some(self.target.param_expression.get_ref().clone())
                        }
                        VirtualFxParameterType::ByName => {
                            Some(self.target.param_name.get_ref().clone())
                        }
                        VirtualFxParameterType::ByIndex => {
                            let index = self.target.param_index.get();
                            Some((index + 1).to_string())
                        }
                        VirtualFxParameterType::ById => None,
                    };
                    control.set_text_or_hide(text);
                }
                t if t.supports_send() => {
                    let text = match self.target.route_selector_type.get() {
                        TrackRouteSelectorType::Dynamic => {
                            self.target.route_expression.get_ref().clone()
                        }
                        TrackRouteSelectorType::ByName => self.target.route_name.get_ref().clone(),
                        TrackRouteSelectorType::ByIndex => {
                            let index = self.target.route_index.get();
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
                    let text = format_tags_as_csv(self.target.tags.get_ref());
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
                    let text = self.target.raw_midi_pattern.get_ref();
                    control.set_text(text.as_str());
                }
                ReaperTargetType::SendOsc => {
                    control.show();
                    let text = self.target.osc_address_pattern.get_ref();
                    control.set_text(text.as_str());
                }
                t if t.supports_fx() => {
                    let text = match self.target.fx_type.get() {
                        VirtualFxType::Dynamic => self.target.fx_expression.get_ref().clone(),
                        VirtualFxType::ByIndex => {
                            let index = self.target.fx_index.get();
                            (index + 1).to_string()
                        }
                        VirtualFxType::ByName | VirtualFxType::AllByName => {
                            self.target.fx_name.get_ref().clone()
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
                ReaperTargetType::FxParameter => Some("Parameter"),
                ReaperTargetType::LoadFxSnapshot => Some("Snapshot"),
                ReaperTargetType::SendOsc => Some("Argument"),
                ReaperTargetType::ClipTransport => Some("Action"),
                t if t.supports_track_exclusivity() => Some("Exclusive"),
                t if t.supports_fx_display_type() => Some("Display"),
                t if t.supports_tags() => Some("Tags"),
                t if t.supports_exclusivity() => Some("Exclusivity"),
                t if t.supports_send() => match self.target.route_type.get() {
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
                    let slot = instance_state.get_slot(self.target.slot_index.get()).ok();
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
                t if t.supports_slot() => {
                    combo.show();
                    combo.fill_combo_box_indexed(
                        (0..CLIP_SLOT_COUNT).map(|i| format!("Slot {}", i + 1)),
                    );
                    combo
                        .select_combo_box_item_by_index(self.target.slot_index.get())
                        .unwrap();
                }
                t if t.supports_fx() => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.fx_type.get().into())
                        .unwrap();
                }
                t if t.supports_send() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackRouteType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.route_type.get().into())
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
            TargetCategory::Reaper => match self.target.r#type.get() {
                ReaperTargetType::SendOsc => {
                    combo.show();
                    combo.fill_combo_box_indexed(OscTypeTag::into_enum_iter());
                    let tag = self.target.osc_arg_type_tag.get();
                    combo.select_combo_box_item_by_index(tag.into()).unwrap();
                }
                ReaperTargetType::FxParameter => {
                    combo.show();
                    combo.fill_combo_box_indexed(VirtualFxParameterType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.param_type.get().into())
                        .unwrap();
                }
                ReaperTargetType::NavigateWithinGroup => {
                    combo.show();
                    combo.fill_combo_box_indexed(SimpleExclusivity::into_enum_iter());
                    let simple_exclusivity: SimpleExclusivity =
                        self.target.exclusivity.get().into();
                    combo
                        .select_combo_box_item_by_index(simple_exclusivity.into())
                        .unwrap();
                }
                t if t.supports_exclusivity() => {
                    combo.show();
                    combo.fill_combo_box_indexed(Exclusivity::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.exclusivity.get().into())
                        .unwrap();
                }
                t if t.supports_send() => {
                    combo.show();
                    combo.fill_combo_box_indexed(TrackRouteSelectorType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.target.route_selector_type.get().into(),
                        )
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
                        self.target.fx_type.get(),
                        VirtualFxType::ById | VirtualFxType::ByIdOrIndex
                    ) {
                        combo.show();
                        if self.target.track_type.get().is_sticky() {
                            let context = self.session.extended_context();
                            if let Ok(track) = self
                                .target
                                .with_context(context, self.mapping.compartment())
                                .first_effective_track()
                            {
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
                ReaperTargetType::TrackShow => {
                    combo.show();
                    combo.fill_combo_box_indexed(RealearnTrackArea::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.track_area.get().into())
                        .unwrap();
                }
                _ if self.target.supports_automation_mode() => {
                    combo.show();
                    combo.fill_combo_box_indexed(RealearnAutomationMode::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.target.track_automation_mode.get().into(),
                        )
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
                ReaperTargetType::ClipTransport => {
                    combo.show();
                    combo.fill_combo_box_indexed(TransportAction::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(
                            self.mapping.target_model.transport_action.get().into(),
                        )
                        .unwrap();
                }
                ReaperTargetType::FxParameter
                    if self.target.param_type.get() == VirtualFxParameterType::ById =>
                {
                    combo.show();
                    let context = self.session.extended_context();
                    if let Ok(fx) = self
                        .target
                        .with_context(context, self.mapping.compartment())
                        .first_fx()
                    {
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
                t if t.supports_fx_display_type() => {
                    combo.show();
                    combo.fill_combo_box_indexed(FxDisplayType::into_enum_iter());
                    combo
                        .select_combo_box_item_by_index(self.target.fx_display_type.get().into())
                        .unwrap();
                }
                t if t.supports_send() => {
                    if self.target.route_selector_type.get() == TrackRouteSelectorType::ById {
                        combo.show();
                        let context = self.session.extended_context();
                        let target_with_context = self
                            .target
                            .with_context(context, self.mapping.compartment());
                        if let Ok(track) = target_with_context.first_effective_track() {
                            // Fill
                            let route_type = self.target.route_type.get();
                            combo.fill_combo_box_indexed_vec(send_combo_box_entries(
                                &track, route_type,
                            ));
                            // Set
                            if route_type == TrackRouteType::HardwareOutput {
                                // Hardware output uses indexes, not IDs.
                                let i = self.target.route_index.get();
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
        let state = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                t if t.supports_fx_chain() => {
                    if matches!(
                        self.target.fx_type.get(),
                        VirtualFxType::Focused | VirtualFxType::This
                    ) {
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
                t if t.supports_track_scrolling() => {
                    Some(("Scroll TCP", self.target.scroll_arrange_view.get()))
                }
                ReaperTargetType::GoToBookmark => {
                    let is_regions = self.target.bookmark_type.get() == BookmarkType::Region;
                    Some(("Regions", is_regions))
                }
                ReaperTargetType::Seek => Some(("Seek play", self.target.seek_play.get())),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_1, state);
    }

    fn invalidate_target_check_box_2(&self) {
        let state = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                ReaperTargetType::LoadMappingSnapshot => Some((
                    "Active mappings only",
                    self.target.active_mappings_only.get(),
                )),
                _ if self.mapping.target_model.supports_track_must_be_selected() => {
                    if self
                        .target
                        .track_type
                        .get()
                        .track_selected_condition_makes_sense()
                    {
                        Some((
                            "Track must be selected",
                            self.target.enable_only_if_track_selected.get(),
                        ))
                    } else {
                        None
                    }
                }
                t if t.supports_track_scrolling() => {
                    Some(("Scroll mixer", self.target.scroll_mixer.get()))
                }
                ReaperTargetType::Seek => Some(("Move view", self.target.move_view.get())),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_2, state);
    }

    fn invalidate_target_check_box_3(&self) {
        let state = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                t if t.supports_fx() => {
                    if self.target.fx_type.get() == VirtualFxType::Focused {
                        None
                    } else {
                        Some((
                            "FX must have focus",
                            self.target.enable_only_if_fx_has_focus.get(),
                        ))
                    }
                }
                ReaperTargetType::Seek => Some(("Use project", self.target.use_project.get())),
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_3, state);
    }

    fn invalidate_target_check_box_4(&self) {
        let state = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                ReaperTargetType::Seek => Some(("Use regions", self.target.use_regions.get())),
                ReaperTargetType::ClipTransport
                    if matches!(
                        self.target.transport_action.get(),
                        TransportAction::PlayStop
                            | TransportAction::PlayPause
                            | TransportAction::Stop
                    ) =>
                {
                    Some(("Next bar", self.target.next_bar.get()))
                }
                t if t.supports_poll_for_feedback() => {
                    Some(("Poll for feedback", self.target.poll_for_feedback.get()))
                }
                _ => None,
            },
            TargetCategory::Virtual => None,
        };
        self.invalidate_check_box(root::ID_TARGET_CHECK_BOX_4, state);
    }

    fn invalidate_target_check_box_5(&self) {
        let checkbox_id = root::ID_TARGET_CHECK_BOX_5;
        let state = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                ReaperTargetType::Seek => {
                    Some(("Use loop points", self.target.use_loop_points.get()))
                }
                ReaperTargetType::GoToBookmark => {
                    Some(("Set loop points", self.target.use_loop_points.get()))
                }
                ReaperTargetType::ClipTransport
                    if matches!(
                        self.target.transport_action.get(),
                        TransportAction::PlayStop | TransportAction::PlayPause
                    ) =>
                {
                    let is_enabled = !self.target.next_bar.get();
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
        let state = match self.target.category.get() {
            TargetCategory::Reaper => match self.target.r#type.get() {
                ReaperTargetType::Seek => {
                    Some(("Use time selection", self.target.use_time_selection.get()))
                }
                ReaperTargetType::GoToBookmark => {
                    Some(("Set time selection", self.target.use_time_selection.get()))
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
        let unit = self.mapping.target_model.unit.get();
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
        self.register_mapping_listeners();
        self.register_source_listeners();
        self.register_target_listeners();
        self.register_mode_listeners();
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

    fn register_mapping_listeners(&self) {
        self.panel.when(
            self.mapping.name.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_window_title();
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::Name, initiator);
            },
        );
        self.panel.when(
            self.mapping.tags.changed_with_initiator(),
            |view, initiator| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::Tags, initiator);
            },
        );
        self.panel
            .when(self.mapping.control_is_enabled.changed(), |view, _| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ControlEnabled, None);
                view.invalidate_mode_controls();
            });
        self.panel
            .when(self.mapping.feedback_is_enabled.changed(), |view, _| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::FeedbackEnabled, None);
                view.invalidate_mode_controls();
            });
        self.panel
            .when(self.mapping.is_enabled.changed(), |view, _| {
                view.invalidate_mapping_enabled_check_box();
            });
        self.panel
            .when(self.mapping.feedback_send_behavior.changed(), |view, _| {
                view.invalidate_mapping_feedback_send_behavior_combo_box();
            });
        self.panel
            .when(self.mapping.visible_in_projection.changed(), |view, _| {
                view.invalidate_mapping_visible_in_projection_check_box();
            });
        self.panel
            .when(self.mapping.advanced_settings_changed(), |view, _| {
                view.invalidate_mapping_advanced_settings_button();
            });
        self.panel.when(
            self.mapping
                .activation_condition_model
                .activation_type
                .changed(),
            |view, _| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ActivationType, None);
            },
        );
        self.panel.when(
            self.mapping
                .activation_condition_model
                .modifier_condition_1
                .changed(),
            |view, _| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ModifierCondition1, None);
            },
        );
        self.panel.when(
            self.mapping
                .activation_condition_model
                .modifier_condition_2
                .changed(),
            |view, _| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::ModifierCondition2, None);
            },
        );
        self.panel.when(
            self.mapping
                .activation_condition_model
                .bank_condition
                .changed(),
            |view, _| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::BankCondition, None);
            },
        );
        self.panel.when(
            self.mapping
                .activation_condition_model
                .eel_condition
                .changed_with_initiator(),
            |view, initiator| {
                view.panel
                    .mapping_header_panel
                    .invalidate_due_to_changed_prop(ItemProp::EelCondition, initiator);
            },
        );
    }

    fn register_source_listeners(&self) {
        let source = self.source;
        self.panel.when(
            source
                .category
                .changed()
                .merge(source.midi_source_type.changed())
                .merge(source.control_element_type.changed()),
            |view, _| {
                view.invalidate_source_controls();
                view.invalidate_mode_controls();
                view.invalidate_help();
            },
        );
        self.panel.when(source.channel.changed(), |view, _| {
            view.invalidate_source_control_visibilities();
            view.invalidate_source_channel();
        });
        self.panel.when(source.is_14_bit.changed(), |view, _| {
            view.invalidate_source_controls();
            view.invalidate_mode_controls();
        });
        self.panel
            .when(source.midi_message_number.changed(), |view, _| {
                view.invalidate_source_midi_message_number_controls();
            });
        self.panel.when(
            source
                .parameter_number_message_number
                .changed_with_initiator()
                .merge(source.osc_arg_index.changed_with_initiator())
                .merge(source.control_element_id.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_source_parameter_number_message_number_controls(initiator);
            },
        );
        self.panel.when(source.is_registered.changed(), |view, _| {
            view.invalidate_source_is_registered_check_box();
        });
        self.panel.when(
            source
                .custom_character
                .changed()
                .merge(source.osc_arg_type_tag.changed()),
            |view, _| {
                view.invalidate_source_character_combo_box();
                view.invalidate_mode_controls();
                view.invalidate_help();
            },
        );
        self.panel
            .when(source.midi_clock_transport_message.changed(), |view, _| {
                view.invalidate_source_midi_clock_transport_message_type_combo_box();
            });
        self.panel.when(
            source
                .osc_address_pattern
                .changed_with_initiator()
                .merge(source.raw_midi_pattern.changed_with_initiator())
                .merge(source.midi_script.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_source_osc_address_pattern_edit_control(initiator);
            },
        );
        self.panel
            .when(source.osc_arg_is_relative.changed(), |view, _| {
                view.invalidate_source_controls();
                view.invalidate_mode_controls();
                view.invalidate_help();
            });
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
            let show_feedback_transformation = is_relevant(ModeParameter::FeedbackTransformation);
            self.enable_if(
                show_feedback_transformation,
                &[
                    root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_LABEL,
                    root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL,
                ],
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
            self.mode.source_value_interval.get_ref().min_val(),
            initiator,
        );
    }

    fn invalidate_mode_max_source_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_mode_source_value_controls_internal(
            root::ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL,
            self.mode.source_value_interval.get_ref().max_val(),
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
            AbsoluteValue::Continuous(self.mode.target_value_interval.get_ref().min_val()),
            initiator,
            false,
        );
    }

    fn invalidate_mode_max_target_value_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.target_value_interval.get_ref().max_val()),
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
            self.target.unit.get(),
            self.session.control_context(),
        );
    }

    fn invalidate_mode_min_jump_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MIN_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MIN_TARGET_JUMP_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.jump_interval.get_ref().min_val()),
            initiator,
            true,
        );
    }

    fn invalidate_mode_max_jump_controls(&self, initiator: Option<u32>) {
        self.invalidate_target_controls_internal(
            root::ID_SETTINGS_MAX_TARGET_JUMP_SLIDER_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_EDIT_CONTROL,
            root::ID_SETTINGS_MAX_TARGET_JUMP_VALUE_TEXT,
            AbsoluteValue::Continuous(self.mode.jump_interval.get_ref().max_val()),
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
            self.mode.step_interval.get_ref().min_val(),
            initiator,
        );
    }

    fn invalidate_mode_fire_line_2_controls(&self, initiator: Option<u32>) {
        let label = match self.mapping.mode_model.fire_mode.get() {
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
                self.mode.press_duration_interval.get_ref().min_val(),
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
            self.mode.step_interval.get_ref().max_val(),
            initiator,
        );
    }

    fn invalidate_mode_fire_line_3_controls(&self, initiator: Option<u32>) {
        let option = match self.mapping.mode_model.fire_mode.get() {
            FireMode::WhenButtonReleased | FireMode::OnSinglePress => {
                Some(("Max", self.mode.press_duration_interval.get_ref().max_val()))
            }
            FireMode::AfterTimeout | FireMode::OnDoublePress => None,
            FireMode::AfterTimeoutKeepFiring => Some(("Rate", self.mode.turbo_rate.get())),
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

    fn invalidate_mode_group_interaction_combo_box(&self) {
        self.view
            .require_control(root::ID_MODE_GROUP_INTERACTION_COMBO_BOX)
            .select_combo_box_item_by_index(self.mode.group_interaction.get().into())
            .unwrap();
    }

    fn invalidate_mode_fire_mode_combo_box(&self) {
        let combo = self.view.require_control(root::ID_MODE_FIRE_COMBO_BOX);
        combo.set_enabled(self.target_category() != TargetCategory::Virtual);
        combo
            .select_combo_box_item_by_index(self.mapping.mode_model.fire_mode.get().into())
            .unwrap();
    }

    fn invalidate_mode_round_target_value_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX)
            .set_checked(self.mode.round_target_value.get());
    }

    fn invalidate_mode_takeover_mode_combo_box(&self) {
        let mode = self.mode.takeover_mode.get();
        self.view
            .require_control(root::ID_MODE_TAKEOVER_MODE)
            .select_combo_box_item_by_index(mode.into())
            .unwrap();
    }

    fn invalidate_mode_button_usage_combo_box(&self) {
        let usage = self.mode.button_usage.get();
        self.view
            .require_control(root::ID_MODE_BUTTON_FILTER_COMBO_BOX)
            .select_combo_box_item_by_index(usage.into())
            .unwrap();
    }

    fn invalidate_mode_encoder_usage_combo_box(&self) {
        let usage = self.mode.encoder_usage.get();
        self.view
            .require_control(root::ID_MODE_RELATIVE_FILTER_COMBO_BOX)
            .select_combo_box_item_by_index(usage.into())
            .unwrap();
    }

    fn invalidate_mode_reverse_check_box(&self) {
        self.view
            .require_control(root::ID_SETTINGS_REVERSE_CHECK_BOX)
            .set_checked(self.mode.reverse.get());
    }

    fn invalidate_mode_target_value_sequence_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL) {
            return;
        }
        let sequence = self.mode.target_value_sequence.get_ref();
        let formatted = match self.target.unit.get() {
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
            .set_text(self.mode.eel_control_transformation.get_ref().as_str());
    }

    fn invalidate_mode_eel_feedback_transformation_edit_control(&self, initiator: Option<u32>) {
        if initiator == Some(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL) {
            return;
        }
        self.view
            .require_control(root::ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL)
            .set_text(self.mode.eel_feedback_transformation.get_ref().as_str());
    }

    fn register_target_listeners(&self) {
        let target = self.target;
        self.panel.when(
            target
                .category
                .changed()
                .merge(target.r#type.changed())
                .merge(target.control_element_type.changed()),
            |view, _| {
                view.invalidate_window_title();
                view.invalidate_target_controls(None);
                view.invalidate_mode_controls();
                view.invalidate_help();
            },
        );
        self.panel.when(target.unit.changed(), |view, _| {
            view.invalidate_target_value_controls();
            view.invalidate_mode_controls();
        });
        self.panel.when(
            target
                .track_type
                .changed_with_initiator()
                .merge(target.track_index.changed_with_initiator())
                .merge(target.track_id.changed_with_initiator())
                .merge(target.track_name.changed_with_initiator())
                .merge(target.track_expression.changed_with_initiator())
                .merge(target.bookmark_type.changed_with_initiator())
                .merge(target.bookmark_anchor_type.changed_with_initiator())
                .merge(target.bookmark_ref.changed_with_initiator())
                .merge(target.transport_action.changed_with_initiator())
                .merge(target.action.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_window_title();
                view.invalidate_target_controls(initiator);
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            target.control_element_id.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_window_title();
                view.invalidate_target_line_2(initiator);
            },
        );
        self.panel.when(
            target
                .fx_type
                .changed_with_initiator()
                .merge(target.fx_index.changed_with_initiator())
                .merge(target.fx_id.changed_with_initiator())
                .merge(target.fx_name.changed_with_initiator())
                .merge(target.fx_expression.changed_with_initiator())
                .merge(target.fx_is_input_fx.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_target_controls(initiator);
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            target
                .route_selector_type
                .changed_with_initiator()
                .merge(target.route_type.changed_with_initiator())
                .merge(target.route_index.changed_with_initiator())
                .merge(target.route_id.changed_with_initiator())
                .merge(target.route_name.changed_with_initiator())
                .merge(target.route_expression.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_target_controls(initiator);
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            target
                .param_type
                .changed_with_initiator()
                .merge(target.param_name.changed_with_initiator())
                .merge(target.param_expression.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_target_controls(initiator);
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(target.param_index.changed(), |view, _| {
            view.invalidate_target_value_controls();
            view.invalidate_mode_controls();
        });
        self.panel
            .when(target.action_invocation_type.changed(), |view, _| {
                view.invalidate_target_line_3(None);
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            });
        self.panel.when(
            target
                .solo_behavior
                .changed()
                .merge(target.touched_parameter_type.changed())
                .merge(target.track_automation_mode.changed())
                .merge(target.automation_mode_override_type.changed())
                .merge(target.track_area.changed())
                .merge(target.slot_index.changed()),
            |view, _| {
                view.invalidate_target_line_3(None);
            },
        );
        self.panel.when(
            target
                .fx_snapshot
                .changed()
                .merge(target.fx_display_type.changed()),
            |view, _| {
                view.invalidate_target_line_4(None);
                view.invalidate_target_value_controls();
            },
        );
        self.panel.when(
            target.track_exclusivity.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_target_line_4(initiator);
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            target.group_id.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_target_line_2(initiator);
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            target
                .osc_arg_type_tag
                .changed_with_initiator()
                .merge(target.osc_arg_index.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_target_line_4(initiator);
                view.invalidate_target_value_controls();
                view.invalidate_mode_controls();
            },
        );
        self.panel.when(
            target
                .fx_is_input_fx
                .changed()
                .merge(target.bookmark_type.changed())
                .merge(target.scroll_arrange_view.changed())
                .merge(target.seek_play.changed()),
            |view, _| {
                view.invalidate_window_title();
                view.invalidate_target_check_boxes();
                view.invalidate_target_value_controls();
            },
        );
        self.panel.when(
            target
                .enable_only_if_track_selected
                .changed()
                .merge(target.scroll_mixer.changed())
                .merge(target.move_view.changed()),
            |view, _| {
                view.invalidate_target_check_boxes();
            },
        );
        self.panel.when(
            target
                .enable_only_if_fx_has_focus
                .changed()
                .merge(target.use_project.changed()),
            |view, _| {
                view.invalidate_target_check_boxes();
            },
        );
        self.panel.when(
            target
                .use_regions
                .changed()
                .merge(target.next_bar.changed()),
            |view, _| {
                view.invalidate_target_check_boxes();
            },
        );
        self.panel.when(
            target
                .use_loop_points
                .changed()
                .merge(target.buffered.changed())
                .merge(target.poll_for_feedback.changed()),
            |view, _| {
                view.invalidate_target_check_boxes();
            },
        );
        self.panel
            .when(target.use_time_selection.changed(), |view, _| {
                view.invalidate_target_check_boxes();
            });
        self.panel
            .when(target.active_mappings_only.changed(), |view, _| {
                view.invalidate_target_check_box_2();
            });
        self.panel.when(target.exclusivity.changed(), |view, _| {
            view.invalidate_target_line_4_combo_box_1();
        });
        self.panel
            .when(target.feedback_resolution.changed(), |view, _| {
                view.invalidate_target_line_2_combo_box_1();
            });
        self.panel.when(
            target
                .automation_mode_override_type
                .changed_with_initiator(),
            |view, initiator| {
                view.invalidate_window_title();
                view.invalidate_target_line_2_combo_box_2(initiator);
            },
        );
        self.panel.when(
            target
                .send_midi_destination
                .changed()
                .merge(target.osc_dev_id.changed()),
            |view, _| {
                view.invalidate_target_line_2(None);
            },
        );
        self.panel.when(
            target
                .raw_midi_pattern
                .changed_with_initiator()
                .merge(target.osc_address_pattern.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_target_line_3(initiator);
                view.invalidate_mode_controls();
            },
        );
        self.panel
            .when(target.tags.changed_with_initiator(), |view, initiator| {
                view.invalidate_target_line_4_edit_control(initiator);
            });
    }

    fn register_mode_listeners(&self) {
        let mode = self.mode;
        self.panel.when(mode.r#type.changed(), |view, _| {
            view.invalidate_mode_controls();
            view.invalidate_help();
        });
        self.panel.when(
            mode.target_value_interval.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_min_target_value_controls(initiator);
                view.invalidate_mode_max_target_value_controls(initiator);
            },
        );
        self.panel.when(
            mode.source_value_interval.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_source_value_controls(initiator);
            },
        );
        self.panel.when(
            mode.jump_interval.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_min_jump_controls(initiator);
                view.invalidate_mode_max_jump_controls(initiator);
            },
        );
        self.panel.when(
            mode.step_interval.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_step_controls(initiator);
            },
        );
        self.panel.when(
            mode.press_duration_interval
                .changed_with_initiator()
                .merge(mode.fire_mode.changed_with_initiator())
                .merge(mode.turbo_rate.changed_with_initiator()),
            |view, initiator| {
                view.invalidate_mode_fire_controls(initiator);
            },
        );
        self.panel
            .when(mode.out_of_range_behavior.changed(), |view, _| {
                view.invalidate_mode_out_of_range_behavior_combo_box();
            });
        self.panel
            .when(mode.group_interaction.changed(), |view, _| {
                view.invalidate_mode_group_interaction_combo_box();
            });
        self.panel
            .when(mode.round_target_value.changed(), |view, _| {
                view.invalidate_mode_round_target_value_check_box();
            });
        self.panel.when(mode.takeover_mode.changed(), |view, _| {
            view.invalidate_mode_takeover_mode_combo_box();
        });
        self.panel.when(mode.button_usage.changed(), |view, _| {
            view.invalidate_mode_button_usage_combo_box();
        });
        self.panel.when(mode.encoder_usage.changed(), |view, _| {
            view.invalidate_mode_encoder_usage_combo_box();
        });
        self.panel.when(mode.rotate.changed(), |view, _| {
            view.invalidate_mode_rotate_check_box();
        });
        self.panel.when(mode.make_absolute.changed(), |view, _| {
            view.invalidate_mode_controls();
            view.invalidate_help();
        });
        self.panel.when(mode.reverse.changed(), |view, _| {
            view.invalidate_mode_reverse_check_box();
        });
        self.panel.when(
            mode.target_value_sequence.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_target_value_sequence_edit_control(initiator);
            },
        );
        self.panel.when(
            mode.eel_control_transformation.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_eel_control_transformation_edit_control(initiator);
            },
        );
        self.panel.when(
            mode.eel_feedback_transformation.changed_with_initiator(),
            |view, initiator| {
                view.invalidate_mode_eel_feedback_transformation_edit_control(initiator);
            },
        );
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
        match self.source.category.get() {
            Midi => b.fill_combo_box_indexed(MidiSourceType::into_enum_iter()),
            Reaper => b.fill_combo_box_indexed(ReaperSourceType::into_enum_iter()),
            Virtual => b.fill_combo_box_indexed(VirtualControlElementType::into_enum_iter()),
            Osc | Never => {}
        };
    }

    #[allow(clippy::single_match)]
    fn fill_source_channel_combo_box(&self) {
        let b = self.view.require_control(root::ID_SOURCE_CHANNEL_COMBO_BOX);
        use SourceCategory::*;
        match self.source.category.get() {
            Midi => b.fill_combo_box_with_data_small(
                iter::once((-1isize, "<Any> (no feedback)".to_string()))
                    .chain((0..16).map(|i| (i as isize, (i + 1).to_string()))),
            ),
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
            Reaper | Virtual | Never => {}
        }
    }

    fn fill_source_midi_clock_transport_message_type_combo_box(&self) {
        self.view
            .require_control(root::ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX)
            .fill_combo_box_indexed(MidiClockTransportMessage::into_enum_iter());
    }

    fn fill_mode_type_combo_box(&self) {
        let target_category = self.mapping.target_model.category.get();
        let items = AbsoluteMode::into_enum_iter().map(|m| {
            let suffix =
                if target_category == TargetCategory::Virtual && m == AbsoluteMode::ToggleButtons {
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
        match self.target.category.get() {
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
            root::ID_SOURCE_RPN_CHECK_BOX => self.write(|p| p.update_source_is_registered()),
            root::ID_SOURCE_14_BIT_CHECK_BOX => self.write(|p| p.update_source_is_14_bit()),
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
            root::ID_SOURCE_TYPE_COMBO_BOX => self.write(|p| p.update_source_type()),
            root::ID_SOURCE_CHANNEL_COMBO_BOX => self.write(|p| p.update_source_channel()),
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
            root::ID_MODE_GROUP_INTERACTION_COMBO_BOX => {
                self.write(|p| p.update_mode_group_interaction())
            }
            root::ID_MODE_TAKEOVER_MODE => self.write(|p| p.update_takeover_mode()),
            root::ID_MODE_BUTTON_FILTER_COMBO_BOX => self.write(|p| p.update_button_usage()),
            root::ID_MODE_RELATIVE_FILTER_COMBO_BOX => self.write(|p| p.update_encoder_usage()),
            root::ID_MODE_FIRE_COMBO_BOX => self.write(|p| p.update_mode_fire_mode()),
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
            root::ID_SOURCE_NUMBER_EDIT_CONTROL => {
                view.write(|p| p.update_source_parameter_number_message_number());
            }
            root::ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL => {
                view.write(|p| p.update_source_pattern());
            }
            // Mode
            root::ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL => {
                view.write(|p| p.update_mode_target_value_sequence());
            }
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
        match m.target_model.category.get() {
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
    let text = match target.category.get() {
        TargetCategory::Reaper => {
            if target.r#type.get().supports_track()
                && target.track_type.get() == VirtualTrackType::Dynamic
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
    let text = match target.category.get() {
        TargetCategory::Reaper => {
            if target.r#type.get().supports_fx() && target.fx_type.get() == VirtualFxType::Dynamic {
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
    let text = match target.category.get() {
        TargetCategory::Reaper => match target.r#type.get() {
            ReaperTargetType::FxParameter
                if target.param_type.get() == VirtualFxParameterType::Dynamic =>
            {
                target
                    .virtual_fx_parameter()
                    .and_then(|p| p.calculated_fx_parameter_index(context, compartment))
                    .map(|i| i.to_string())
            }
            t if t.supports_send()
                && target.route_selector_type.get() == TrackRouteSelectorType::Dynamic =>
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
