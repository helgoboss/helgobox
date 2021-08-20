use crate::application::{
    MappingModel, SharedMapping, SharedSession, SourceCategory, TargetCategory,
    TargetModelFormatMultiLine, WeakSession,
};
use crate::base::when;
use crate::domain::{GroupId, MappingCompartment, MappingId, QualifiedMappingId, ReaperTarget};

use crate::infrastructure::data::{
    MappingModelData, ModeModelData, SourceModelData, TargetModelData,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::bindings::root::{
    ID_MAPPING_ROW_CONTROL_CHECK_BOX, ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
};
use crate::infrastructure::ui::util::symbols;
use crate::infrastructure::ui::{
    copy_object_to_clipboard, get_object_from_clipboard, util, ClipboardObject,
    IndependentPanelManager, Item, SharedMainState,
};
use reaper_high::Reaper;
use reaper_low::raw;
use rxrust::prelude::*;
use slog::debug;
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::time::Duration;
use swell_ui::{DialogUnits, MenuBar, Pixels, Point, SharedView, View, ViewContext, Window};

pub type SharedIndependentPanelManager = Rc<RefCell<IndependentPanelManager>>;

/// Panel containing the summary data of one mapping and buttons such as "Remove".
#[derive(Debug)]
pub struct MappingRowPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    row_index: u32,
    is_last_row: bool,
    // We use virtual scrolling in order to be able to show a large amount of rows without any
    // performance issues. That means there's a fixed number of mapping rows and they just
    // display different mappings depending on the current scroll position. If there are less
    // mappings than the fixed number, some rows remain unused. In this case their mapping is
    // `None`, which will make the row hide itself.
    mapping: RefCell<Option<SharedMapping>>,
    // Fires when a mapping is about to change.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
    panel_manager: Weak<RefCell<IndependentPanelManager>>,
}

impl MappingRowPanel {
    pub fn new(
        session: WeakSession,
        row_index: u32,
        panel_manager: Weak<RefCell<IndependentPanelManager>>,
        main_state: SharedMainState,
        is_last_row: bool,
    ) -> MappingRowPanel {
        MappingRowPanel {
            view: Default::default(),
            session,
            main_state,
            row_index,
            party_is_over_subject: Default::default(),
            mapping: None.into(),
            panel_manager,
            is_last_row,
        }
    }

    pub fn handle_matched_mapping(&self) {
        self.source_match_indicator_control().enable();
        self.view
            .require_window()
            .set_timer(SOURCE_MATCH_INDICATOR_TIMER_ID, Duration::from_millis(50));
    }

    fn source_match_indicator_control(&self) -> Window {
        self.view
            .require_control(root::IDC_MAPPING_ROW_MATCHED_INDICATOR_TEXT)
    }

    pub fn mapping_id(&self) -> Option<MappingId> {
        let mapping = self.optional_mapping()?;
        let mapping = mapping.borrow();
        Some(mapping.id())
    }

    pub fn set_mapping(self: &SharedView<Self>, mapping: Option<SharedMapping>) {
        self.party_is_over_subject.borrow_mut().next(());
        match &mapping {
            None => self.view.require_window().hide(),
            Some(m) => {
                self.view.require_window().show();
                self.invalidate_all_controls(m.borrow().deref());
                self.register_listeners(m.borrow().deref());
            }
        }
        self.mapping.replace(mapping);
    }

    fn invalidate_all_controls(&self, mapping: &MappingModel) {
        self.invalidate_name_labels(mapping);
        self.invalidate_source_label(mapping);
        self.invalidate_target_label(mapping);
        self.invalidate_learn_source_button(mapping);
        self.invalidate_learn_target_button(mapping);
        self.invalidate_control_check_box(mapping);
        self.invalidate_feedback_check_box(mapping);
        self.invalidate_on_indicator(mapping);
        self.invalidate_button_enabled_states();
    }

    fn invalidate_divider(&self) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_DIVIDER)
            .set_visible(!self.is_last_row);
    }

    fn invalidate_name_labels(&self, mapping: &MappingModel) {
        let main_state = self.main_state.borrow();
        let group_name = if main_state
            .displayed_group_for_active_compartment()
            .is_some()
        {
            None
        } else {
            // All groups are shown. Add more context!
            let group_id = mapping.group_id.get();
            let compartment = main_state.active_compartment.get();
            let session = self.session();
            let label = if group_id.is_default() {
                "<Default>".to_owned()
            } else if let Some(group) = session.borrow().find_group_by_id(compartment, group_id) {
                group.borrow().name().to_owned()
            } else {
                "<group not present>".to_owned()
            };
            Some(label)
        };
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_MAPPING_LABEL)
            .set_text(mapping.effective_name());
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_GROUP_LABEL)
            .set_text_or_hide(group_name);
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn invalidate_source_label(&self, mapping: &MappingModel) {
        let plain_label = mapping.source_model.to_string();
        let rich_label = if mapping.source_model.category.get() == SourceCategory::Virtual {
            let session = self.session();
            let session = session.borrow();
            let controller_mappings = session.mappings(MappingCompartment::ControllerMappings);
            let mappings: Vec<_> = controller_mappings
                .filter(|m| {
                    let m = m.borrow();
                    m.target_model.category.get() == TargetCategory::Virtual
                        && m.target_model.create_control_element()
                            == mapping.source_model.create_control_element()
                })
                .collect();
            if mappings.is_empty() {
                plain_label
            } else {
                let first_mapping = mappings[0].borrow();
                let first_mapping_name = first_mapping.effective_name();
                if mappings.len() == 1 {
                    format!("{}\n({})", plain_label, first_mapping_name)
                } else {
                    format!(
                        "{}\n({} + {})",
                        plain_label,
                        first_mapping_name,
                        mappings.len() - 1
                    )
                }
            }
        } else {
            plain_label
        };
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_text(rich_label);
    }

    fn invalidate_target_label(&self, mapping: &MappingModel) {
        let session = self.session();
        let session = session.borrow();
        let context = session.extended_context();
        if !context
            .context()
            .project_or_current_project()
            .is_available()
        {
            // Prevent error on project close
            return;
        }
        let target_model_string =
            TargetModelFormatMultiLine::new(&mapping.target_model, context, mapping.compartment())
                .to_string();
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_text(target_model_string);
    }

    fn invalidate_learn_source_button(&self, mapping: &MappingModel) {
        let text = if self
            .session()
            .borrow()
            .mapping_is_learning_source(mapping.qualified_id())
        {
            "Stop"
        } else {
            "Learn source"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON)
            .set_text(text);
    }

    fn invalidate_learn_target_button(&self, mapping: &MappingModel) {
        let text = if self
            .session()
            .borrow()
            .mapping_is_learning_target(mapping.qualified_id())
        {
            "Stop"
        } else {
            "Learn target"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON)
            .set_text(text);
    }

    fn init_symbol_controls(&self) {
        self.view
            .require_control(root::ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .set_text(symbols::arrow_right_symbol().to_string());
        self.view
            .require_control(root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .set_text(symbols::arrow_left_symbol().to_string());
        self.view
            .require_control(root::ID_UP_BUTTON)
            .set_text(symbols::arrow_up_symbol().to_string());
        self.view
            .require_control(root::ID_DOWN_BUTTON)
            .set_text(symbols::arrow_down_symbol().to_string());
        let indicator = self
            .view
            .require_control(root::IDC_MAPPING_ROW_MATCHED_INDICATOR_TEXT);
        indicator.set_text(symbols::indicator_symbol().to_string());
        indicator.disable();
    }

    fn invalidate_control_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .set_checked(mapping.control_is_enabled.get());
    }

    fn invalidate_feedback_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .set_checked(mapping.feedback_is_enabled.get());
    }

    fn invalidate_on_indicator(&self, mapping: &MappingModel) {
        let is_on = self.session().borrow().mapping_is_on(mapping.id());
        self.view
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_enabled(is_on);
        self.view
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_enabled(is_on);
    }

    fn mappings_are_read_only(&self) -> bool {
        self.session()
            .borrow()
            .mappings_are_read_only(self.active_compartment())
    }

    fn invalidate_button_enabled_states(&self) {
        let enabled = !self.mappings_are_read_only();
        let buttons = [
            root::ID_UP_BUTTON,
            root::ID_DOWN_BUTTON,
            root::ID_MAPPING_ROW_CONTROL_CHECK_BOX,
            root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
            root::ID_MAPPING_ROW_EDIT_BUTTON,
            root::ID_MAPPING_ROW_DUPLICATE_BUTTON,
            root::ID_MAPPING_ROW_REMOVE_BUTTON,
            root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON,
            root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON,
        ];
        for b in buttons.iter() {
            self.view.require_control(*b).set_enabled(enabled);
        }
    }

    fn register_listeners(self: &SharedView<Self>, mapping: &MappingModel) {
        let session = self.session();
        let session = session.borrow();
        self.when(mapping.name.changed(), |view| {
            view.with_mapping(Self::invalidate_name_labels);
        });
        self.when(mapping.source_model.changed(), |view| {
            view.with_mapping(Self::invalidate_source_label);
        });
        self.when(
            mapping
                .target_model
                .changed()
                .merge(ReaperTarget::potential_static_change_events()),
            |view| {
                view.with_mapping(|p, m| {
                    p.invalidate_name_labels(m);
                    p.invalidate_target_label(m);
                });
            },
        );
        self.when(mapping.control_is_enabled.changed(), |view| {
            view.with_mapping(Self::invalidate_control_check_box);
        });
        self.when(mapping.feedback_is_enabled.changed(), |view| {
            view.with_mapping(Self::invalidate_feedback_check_box);
        });
        self.when(session.mapping_which_learns_source_changed(), |view| {
            view.with_mapping(Self::invalidate_learn_source_button);
        });
        self.when(session.mapping_which_learns_target_changed(), |view| {
            view.with_mapping(Self::invalidate_learn_target_button);
        });
        self.when(session.on_mappings_changed(), |view| {
            view.with_mapping(Self::invalidate_on_indicator);
        });
        self.when(
            session
                .main_preset_auto_load_mode
                .changed()
                .merge(session.learn_many_state_changed()),
            |view| {
                view.invalidate_button_enabled_states();
            },
        );
    }

    fn with_mapping(&self, use_mapping: impl Fn(&Self, &MappingModel)) {
        let mapping = self.mapping.borrow();
        if let Some(m) = mapping.as_ref() {
            use_mapping(self, m.borrow().deref())
        }
    }

    fn closed_or_mapping_will_change(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn require_mapping(&self) -> Ref<SharedMapping> {
        Ref::map(self.mapping.borrow(), |m| m.as_ref().unwrap())
    }

    fn optional_mapping(&self) -> Option<SharedMapping> {
        self.mapping.clone().into_inner()
    }

    fn require_qualified_mapping_id(&self) -> QualifiedMappingId {
        self.require_mapping().borrow().qualified_id()
    }

    fn edit_mapping(&self) {
        self.main_state.borrow_mut().stop_filter_learning();
        self.panel_manager()
            .borrow_mut()
            .edit_mapping(self.require_mapping().deref());
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn move_mapping_within_list(&self, increment: isize) -> Result<(), &'static str> {
        // When we route keyboard input to ReaLearn and press space, it presses the "Up" button,
        // even if we don't display the rows. Don't know why, but suppress a panic here.
        let mapping = self.optional_mapping().ok_or("row has no mapping")?;
        let within_same_group = self
            .main_state
            .borrow()
            .displayed_group_for_active_compartment()
            .is_some();
        let _ = self.session().borrow_mut().move_mapping_within_list(
            self.active_compartment(),
            mapping.borrow().id(),
            within_same_group,
            increment,
        );
        Ok(())
    }

    fn active_compartment(&self) -> MappingCompartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn remove_mapping(&self) {
        if !self
            .view
            .require_window()
            .confirm("ReaLearn", "Do you really want to remove this mapping?")
        {
            return;
        }
        self.session()
            .borrow_mut()
            .remove_mapping(self.require_qualified_mapping_id());
    }

    fn duplicate_mapping(&self) {
        self.session()
            .borrow_mut()
            .duplicate_mapping(self.require_qualified_mapping_id())
            .unwrap();
    }

    fn toggle_learn_source(&self) {
        let shared_session = self.session();
        shared_session
            .borrow_mut()
            .toggle_learning_source(&shared_session, self.require_mapping().deref());
    }

    fn toggle_learn_target(&self) {
        let shared_session = self.session();
        shared_session
            .borrow_mut()
            .toggle_learning_target(&shared_session, self.require_qualified_mapping_id());
    }

    fn update_control_is_enabled(&self) {
        self.require_mapping().borrow_mut().control_is_enabled.set(
            self.view
                .require_control(ID_MAPPING_ROW_CONTROL_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_feedback_is_enabled(&self) {
        self.require_mapping().borrow_mut().feedback_is_enabled.set(
            self.view
                .require_control(ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
                .is_checked(),
        );
    }

    fn open_context_menu(&self, location: Point<Pixels>) -> Result<(), &'static str> {
        let menu_bar = MenuBar::new_popup_menu();
        let pure_menu = {
            use std::iter::once;
            use swell_ui::menu_tree::*;
            let shared_session = self.session();
            let session = shared_session.borrow();
            let mapping = self.mapping.borrow();
            let mapping = mapping.as_ref().ok_or("row contains no mapping")?;
            let mapping = mapping.borrow();
            let compartment = mapping.compartment();
            let mapping_id = mapping.id();
            let clipboard_object = get_object_from_clipboard();
            let clipboard_object_2 = clipboard_object.clone();
            // TODO-medium Since ReaLearn 2.8.0-pre5, menu items can return values, so we could
            //  easily refactor this clone hell if we return e.g. MenuAction enum values.
            let group_id = mapping.group_id.get();
            let session_1 = shared_session.clone();
            let session_2 = shared_session.clone();
            let session_3 = shared_session.clone();
            let session_4 = shared_session.clone();
            let session_5 = shared_session.clone();
            let session_6 = shared_session.clone();
            let session_7 = shared_session.clone();
            let session_8 = shared_session.clone();
            let session_9 = shared_session.clone();
            let entries = vec![
                item("Copy", move || {
                    let _ = copy_mapping_object(
                        session_1,
                        compartment,
                        mapping_id,
                        ObjectType::Mapping,
                    );
                }),
                {
                    let desc = match clipboard_object {
                        Some(ClipboardObject::Mapping(m)) => Some((
                            format!("Paste mapping \"{}\" (replace)", &m.name),
                            ClipboardObject::Mapping(m),
                        )),
                        Some(ClipboardObject::Source(s)) => Some((
                            format!("Paste source ({})", s.category),
                            ClipboardObject::Source(s),
                        )),
                        Some(ClipboardObject::Mode(m)) => {
                            Some(("Paste mode".to_owned(), ClipboardObject::Mode(m)))
                        }
                        Some(ClipboardObject::Target(t)) => Some((
                            format!("Paste target ({})", t.category),
                            ClipboardObject::Target(t),
                        )),
                        None | Some(ClipboardObject::Mappings(_)) => None,
                    };
                    if let Some((label, obj)) = desc {
                        item(label, move || {
                            let _ = paste_object_in_place(
                                obj,
                                session_2,
                                compartment,
                                mapping_id,
                                group_id,
                            );
                        })
                    } else {
                        disabled_item("Paste (replace)")
                    }
                },
                {
                    let desc = match clipboard_object_2 {
                        Some(ClipboardObject::Mapping(m)) => Some((
                            format!("Paste mapping \"{}\" (insert below)", &m.name),
                            vec![*m],
                        )),
                        Some(ClipboardObject::Mappings(vec)) => {
                            Some((format!("Paste {} mappings below", vec.len()), vec))
                        }
                        _ => None,
                    };
                    if let Some((label, datas)) = desc {
                        item(label, move || {
                            let _ = paste_mappings(
                                datas,
                                session_8,
                                compartment,
                                Some(mapping_id),
                                group_id,
                            );
                        })
                    } else {
                        disabled_item("Paste (insert below)")
                    }
                },
                menu(
                    "Copy part",
                    vec![
                        item("Copy source", move || {
                            let _ = copy_mapping_object(
                                session_5,
                                compartment,
                                mapping_id,
                                ObjectType::Source,
                            );
                        }),
                        item("Copy mode", move || {
                            let _ = copy_mapping_object(
                                session_6,
                                compartment,
                                mapping_id,
                                ObjectType::Mode,
                            );
                        }),
                        item("Copy target", move || {
                            let _ = copy_mapping_object(
                                session_7,
                                compartment,
                                mapping_id,
                                ObjectType::Target,
                            );
                        }),
                    ],
                ),
                menu(
                    "Move to group",
                    once(item_with_opts(
                        "<Default>",
                        ItemOpts {
                            enabled: !group_id.is_default(),
                            checked: false,
                        },
                        move || {
                            move_mapping_to_group(
                                session_3,
                                compartment,
                                mapping_id,
                                GroupId::default(),
                            )
                        },
                    ))
                    .chain(session.groups_sorted(compartment).map(move |g| {
                        let session = session_4.clone();
                        let g = g.borrow();
                        let g_id = g.id();
                        item_with_opts(
                            g.name.get_ref().to_owned(),
                            ItemOpts {
                                enabled: group_id != g_id,
                                checked: false,
                            },
                            move || move_mapping_to_group(session, compartment, mapping_id, g_id),
                        )
                    }))
                    .collect(),
                ),
                item("Log debug info", move || {
                    session_9.borrow().log_mapping(compartment, mapping_id);
                }),
            ];
            let mut root_menu = root_menu(entries);
            root_menu.index(1);
            fill_menu(menu_bar.menu(), &root_menu);
            root_menu
        };
        let result_index = self
            .view
            .require_window()
            .open_popup_menu(menu_bar.menu(), location)
            .ok_or("no entry selected")?;
        pure_menu
            .find_item_by_id(result_index)
            .expect("selected menu item not found")
            .invoke_handler();
        Ok(())
    }

    fn when(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = (), Err = ()> + 'static,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when(event.take_until(self.closed_or_mapping_will_change()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, _| reaction(panel));
    }
}

impl View for MappingRowPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROW_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.hide();
        window.move_to(Point::new(DialogUnits(0), DialogUnits(self.row_index * 48)));
        self.init_symbol_controls();
        self.invalidate_divider();
        false
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_MAPPING_ROW_EDIT_BUTTON => self.edit_mapping(),
            root::ID_UP_BUTTON => {
                let _ = self.move_mapping_within_list(-1);
            }
            root::ID_DOWN_BUTTON => {
                let _ = self.move_mapping_within_list(1);
            }
            root::ID_MAPPING_ROW_REMOVE_BUTTON => self.remove_mapping(),
            root::ID_MAPPING_ROW_DUPLICATE_BUTTON => self.duplicate_mapping(),
            root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON => self.toggle_learn_source(),
            root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON => self.toggle_learn_target(),
            root::ID_MAPPING_ROW_CONTROL_CHECK_BOX => self.update_control_is_enabled(),
            root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX => self.update_feedback_is_enabled(),
            _ => unreachable!(),
        }
    }

    fn context_menu_wanted(self: SharedView<Self>, location: Point<Pixels>) {
        let _ = self.open_context_menu(location);
    }

    fn control_color_static(self: SharedView<Self>, hdc: raw::HDC, _: Window) -> raw::HBRUSH {
        util::view::control_color_static_default(hdc, util::view::mapping_row_background_brush())
    }

    fn control_color_dialog(self: SharedView<Self>, hdc: raw::HDC, _: raw::HWND) -> raw::HBRUSH {
        util::view::control_color_dialog_default(hdc, util::view::mapping_row_background_brush())
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

impl Drop for MappingRowPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping row panel...");
    }
}

fn move_mapping_to_group(
    session: SharedSession,
    compartment: MappingCompartment,
    mapping_id: MappingId,
    group_id: GroupId,
) {
    session
        .borrow_mut()
        .move_mappings_to_group(compartment, &[mapping_id], group_id)
        .unwrap();
}

fn copy_mapping_object(
    session: SharedSession,
    compartment: MappingCompartment,
    mapping_id: MappingId,
    object_type: ObjectType,
) -> Result<(), &'static str> {
    let session = session.borrow();
    let (_, mapping) = session
        .find_mapping_and_index_by_id(compartment, mapping_id)
        .ok_or("mapping not found")?;
    use ObjectType::*;
    let mapping = mapping.borrow();
    let object = match object_type {
        Mapping => ClipboardObject::Mapping(Box::new(MappingModelData::from_model(&mapping))),
        Source => {
            ClipboardObject::Source(Box::new(SourceModelData::from_model(&mapping.source_model)))
        }
        Mode => ClipboardObject::Mode(Box::new(ModeModelData::from_model(&mapping.mode_model))),
        Target => {
            ClipboardObject::Target(Box::new(TargetModelData::from_model(&mapping.target_model)))
        }
    };
    copy_object_to_clipboard(object)
}

enum ObjectType {
    Mapping,
    Source,
    Mode,
    Target,
}

pub fn paste_object_in_place(
    obj: ClipboardObject,
    session: SharedSession,
    compartment: MappingCompartment,
    mapping_id: MappingId,
    group_id: GroupId,
) -> Result<(), &'static str> {
    let session = session.borrow();
    let (_, mapping) = session
        .find_mapping_and_index_by_id(compartment, mapping_id)
        .ok_or("mapping not found")?;
    let mut mapping = mapping.borrow_mut();
    match obj {
        ClipboardObject::Mapping(mut m) => {
            m.group_id = group_id;
            m.apply_to_model(&mut mapping, session.extended_context());
        }
        ClipboardObject::Source(s) => {
            s.apply_to_model(&mut mapping.source_model, compartment);
        }
        ClipboardObject::Mode(m) => {
            m.apply_to_model(&mut mapping.mode_model);
        }
        ClipboardObject::Target(t) => {
            t.apply_to_model(
                &mut mapping.target_model,
                compartment,
                session.extended_context(),
            );
        }
        ClipboardObject::Mappings(_) => return Err("can't paste a list of mappings in place"),
    };
    Ok(())
}

/// If `below_mapping_id` not given, it's added at the end.
pub fn paste_mappings(
    mapping_datas: Vec<MappingModelData>,
    session: SharedSession,
    compartment: MappingCompartment,
    below_mapping_id: Option<MappingId>,
    group_id: GroupId,
) -> Result<(), &'static str> {
    let mut session = session.borrow_mut();
    let index = if let Some(id) = below_mapping_id {
        session
            .find_mapping_and_index_by_id(compartment, id)
            .ok_or("mapping not found")?
            .0
    } else {
        session.mapping_count(compartment)
    };
    let new_mappings: Vec<_> = mapping_datas
        .into_iter()
        .map(|mut data| {
            data.group_id = group_id;
            data.to_model(compartment, session.extended_context())
        })
        .collect();
    session.insert_mappings_at(compartment, index + 1, new_mappings.into_iter());
    Ok(())
}

const SOURCE_MATCH_INDICATOR_TIMER_ID: usize = 571;
