use crate::application::{
    GroupId, MappingModel, SharedMapping, SharedSession, SourceCategory, TargetCategory,
    WeakSession,
};
use crate::core::when;
use crate::domain::MappingCompartment;

use crate::core::Global;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::bindings::root::{
    ID_MAPPING_ROW_CONTROL_CHECK_BOX, ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
};
use crate::infrastructure::ui::util::symbols;
use crate::infrastructure::ui::{util, IndependentPanelManager, SharedMainState};
use reaper_high::Reaper;
use reaper_low::raw;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use slog::debug;
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::{Rc, Weak};
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
        self.invalidate_name_label(&mapping);
        self.invalidate_source_label(&mapping);
        self.invalidate_target_label(&mapping);
        self.invalidate_learn_source_button(&mapping);
        self.invalidate_learn_target_button(&mapping);
        self.invalidate_control_check_box(&mapping);
        self.invalidate_feedback_check_box(&mapping);
        self.invalidate_on_indicator(&mapping);
        self.invalidate_button_enabled_states();
    }

    fn invalidate_divider(&self) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_DIVIDER)
            .set_visible(!self.is_last_row);
    }

    fn invalidate_name_label(&self, mapping: &MappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_GROUP_BOX)
            .set_text(mapping.name.get_ref().as_str());
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
                let first_mapping_name = first_mapping.name.get_ref().clone();
                if mappings.len() == 1 {
                    format!("{}\n({})", plain_label, first_mapping_name)
                } else {
                    format!(
                        "{}({} + {})",
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
        let target_model_string = mapping
            .target_model
            .with_context(self.session().borrow().context())
            .to_string();
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_text(target_model_string);
    }

    fn invalidate_learn_source_button(&self, mapping: &MappingModel) {
        let text = if self.session().borrow().mapping_is_learning_source(mapping) {
            "Stop"
        } else {
            "Learn source"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON)
            .set_text(text);
    }

    fn invalidate_learn_target_button(&self, mapping: &MappingModel) {
        let text = if self.session().borrow().mapping_is_learning_target(mapping) {
            "Stop"
        } else {
            "Learn target"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON)
            .set_text(text);
    }

    fn use_arrow_characters(&self) {
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
        let session = self.session();
        let session = session.borrow();
        session.is_learning_many_mappings()
            || (self.active_compartment() == MappingCompartment::MainMappings
                && session.main_preset_auto_load_is_active())
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
            view.with_mapping(Self::invalidate_name_label);
        });
        self.when(mapping.source_model.changed(), |view| {
            view.with_mapping(Self::invalidate_source_label);
        });
        self.when(
            mapping
                .target_model
                .changed()
                // We also want to reflect track name changes immediately.
                .merge(Global::control_surface_rx().track_name_changed().map_to(())),
            |view| {
                view.with_mapping(Self::invalidate_target_label);
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

    fn closed_or_mapping_will_change(&self) -> impl UnitEvent {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn require_mapping(&self) -> Ref<SharedMapping> {
        Ref::map(self.mapping.borrow(), |m| m.as_ref().unwrap())
    }

    fn require_mapping_address(&self) -> *const MappingModel {
        self.mapping.borrow().as_ref().unwrap().as_ptr()
    }

    fn edit_mapping(&self) {
        self.panel_manager()
            .borrow_mut()
            .edit_mapping(self.require_mapping().deref());
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn move_mapping_within_list(&self, increment: isize) {
        let within_same_group = self.main_state.borrow().group_filter.get().is_some();
        let _ = self.session().borrow_mut().move_mapping_within_list(
            self.active_compartment(),
            self.require_mapping().borrow().id(),
            within_same_group,
            increment,
        );
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
            .remove_mapping(self.active_compartment(), self.require_mapping_address());
    }

    fn duplicate_mapping(&self) {
        self.session()
            .borrow_mut()
            .duplicate_mapping(self.active_compartment(), self.require_mapping_address())
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
            .toggle_learning_target(&shared_session, self.require_mapping().deref());
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

    fn start_moving_mapping_to_other_group(
        &self,
        location: Point<Pixels>,
    ) -> Result<(), &'static str> {
        let (mapping_id, dest_group_id) = {
            let mapping = self.mapping.borrow();
            let mapping = mapping.as_ref().ok_or("row contains no mapping")?;
            let mapping = mapping.borrow();
            (
                mapping.id(),
                self.let_user_pick_destination_group(&mapping, location)?,
            )
        };
        self.session()
            .borrow_mut()
            .move_mapping_to_group(mapping_id, dest_group_id)
            .unwrap();
        Ok(())
    }

    fn let_user_pick_destination_group(
        &self,
        mapping: &MappingModel,
        location: Point<Pixels>,
    ) -> Result<GroupId, &'static str> {
        let current_group_id = mapping.group_id.get();
        let menu_bar =
            MenuBar::load(root::IDR_ROW_PANEL_CONTEXT_MENU).expect("menu bar couldn't be loaded");
        let menu = menu_bar.get_menu(0).expect("menu bar didn't have 1st menu");
        let session = self.session();
        let session = session.borrow();
        for (i, group) in session.groups().enumerate() {
            let group = group.borrow();
            let item_id = i as u32 + 2;
            menu.add_item(item_id, group.name.get_ref().to_string());
            // Disable group if it's the current one.
            if current_group_id == group.id() {
                menu.set_item_enabled(item_id, false);
            }
        }
        // Disable "<Default>" group if it's the current one.
        if current_group_id.is_default() {
            menu.set_item_enabled(1, false);
        }
        let picked_item_id = match self.view.require_window().open_popup_menu(menu, location) {
            None => return Err("user didn't pick any group"),
            Some(r) => r,
        };
        let desired_group_index = if picked_item_id == 1 {
            None
        } else {
            Some(picked_item_id - 2)
        };
        let desired_group_id = desired_group_index.map(|i| {
            session
                .find_group_id_by_index(i as _)
                .expect("no such group")
        });
        Ok(desired_group_id.unwrap_or_default())
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
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
        window.move_to(Point::new(DialogUnits(0), DialogUnits(self.row_index * 48)));
        self.use_arrow_characters();
        self.invalidate_divider();
        window.hide();
        false
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_MAPPING_ROW_EDIT_BUTTON => self.edit_mapping(),
            root::ID_UP_BUTTON => self.move_mapping_within_list(-1),
            root::ID_DOWN_BUTTON => self.move_mapping_within_list(1),
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
        let _ = self.start_moving_mapping_to_other_group(location);
    }

    fn erase_background(self: SharedView<Self>, hdc: raw::HDC) -> bool {
        util::view::erase_background_with(
            self.view.require_window().raw(),
            hdc,
            util::view::row_brush(),
        )
    }

    // On Linux, WM_ERASEBKGND is not called, so we need to do it in WM_PAINT.
    #[cfg(target_os = "linux")]
    fn paint(self: SharedView<Self>) -> bool {
        util::view::erase_background_with(
            self.view.require_window().raw(),
            std::ptr::null_mut(),
            util::view::rows_brush(),
        )
    }

    fn control_color_static(
        self: SharedView<Self>,
        hdc: raw::HDC,
        _hwnd: raw::HWND,
    ) -> raw::HBRUSH {
        util::view::control_color_static_with(hdc, util::view::row_brush())
    }
}

impl Drop for MappingRowPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping row panel...");
    }
}
