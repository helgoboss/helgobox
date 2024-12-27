use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{parse_tags_from_csv, symbols};

use std::cell::{Cell, RefCell};
use std::convert::TryInto;

use std::rc::{Rc, Weak};

use crate::application::{
    ActivationConditionCommand, ActivationConditionProp, ActivationType, BankConditionModel,
    GroupCommand, GroupModel, MappingCommand, MappingModel, ModifierConditionModel,
    SharedUnitModel, UnitModel, WeakUnitModel,
};
use crate::domain::ui_util::format_tags_as_csv;
use crate::domain::{CompartmentKind, MappingId, Tag};
use crate::infrastructure::ui::menus;
use derivative::Derivative;
use std::fmt::Debug;
use strum::IntoEnumIterator;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, WeakView, Window};

type SharedItem = Rc<RefCell<dyn Item>>;
type WeakItem = Weak<RefCell<dyn Item>>;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct MappingHeaderPanel {
    view: ViewContext,
    session: WeakUnitModel,
    item: RefCell<Option<WeakItem>>,
    is_invoked_programmatically: Cell<bool>,
    position: Point<DialogUnits>,
    #[derivative(Debug = "ignore")]
    parent_view: RefCell<Option<WeakView<dyn View>>>,
}

pub trait Item: Debug {
    fn compartment(&self) -> CompartmentKind;
    fn supports_name_change(&self) -> bool;
    fn name(&self) -> &str;
    fn set_name(&mut self, session: WeakUnitModel, name: String, initiator: u32);
    fn tags(&self) -> &[Tag];
    fn set_tags(&mut self, session: WeakUnitModel, tags: Vec<Tag>, initiator: u32);
    fn control_is_enabled(&self) -> bool;
    fn set_control_is_enabled(&mut self, session: WeakUnitModel, value: bool);
    fn feedback_is_enabled(&self) -> bool;
    fn set_feedback_is_enabled(&mut self, session: WeakUnitModel, value: bool);
    fn activation_type(&self) -> ActivationType;
    fn set_activation_type(&mut self, session: WeakUnitModel, value: ActivationType);
    fn modifier_condition_1(&self) -> ModifierConditionModel;
    fn set_modifier_condition_1(&mut self, session: WeakUnitModel, value: ModifierConditionModel);
    fn modifier_condition_2(&self) -> ModifierConditionModel;
    fn set_modifier_condition_2(&mut self, session: WeakUnitModel, value: ModifierConditionModel);
    fn bank_condition(&self) -> BankConditionModel;
    fn set_bank_condition(&mut self, session: WeakUnitModel, value: BankConditionModel);
    fn script(&self) -> &str;
    fn set_script(&mut self, session: WeakUnitModel, value: String, initiator: u32);
    fn mapping_id(&self) -> Option<MappingId>;
    fn set_mapping_id(&mut self, session: WeakUnitModel, value: Option<MappingId>);
}

pub enum ItemProp {
    Name,
    Tags,
    ControlEnabled,
    FeedbackEnabled,
    ActivationType,
    ModifierCondition1,
    ModifierCondition2,
    BankCondition,
    Script,
    MappingId,
}

impl ItemProp {
    pub fn from_activation_condition_prop(prop: &ActivationConditionProp) -> Self {
        use ActivationConditionProp as S;
        match prop {
            S::ActivationType => Self::ActivationType,
            S::ModifierCondition1 => Self::ModifierCondition1,
            S::ModifierCondition2 => Self::ModifierCondition2,
            S::BankCondition => Self::BankCondition,
            S::Script => Self::Script,
            S::MappingId => Self::MappingId,
        }
    }
}

impl MappingHeaderPanel {
    pub fn new(
        session: WeakUnitModel,
        position: Point<DialogUnits>,
        initial_item: Option<WeakItem>,
    ) -> MappingHeaderPanel {
        MappingHeaderPanel {
            view: Default::default(),
            session,
            item: RefCell::new(initial_item),
            is_invoked_programmatically: false.into(),
            position,
            parent_view: Default::default(),
        }
    }

    pub fn set_parent_view(&self, parent_view: WeakView<dyn View>) {
        self.parent_view.replace(Some(parent_view));
    }

    fn get_parent_view(&self) -> Option<SharedView<dyn View>> {
        self.parent_view.borrow().as_ref().and_then(|v| v.upgrade())
    }

    pub fn set_invoked_programmatically(&self, value: bool) {
        self.is_invoked_programmatically.set(value);
    }

    pub fn clear_item(&self) {
        self.item.replace(None);
    }

    pub fn set_item(self: SharedView<Self>, item: SharedItem) {
        self.invoke_programmatically(|| {
            self.invalidate_controls_internal(&*item.borrow());
            self.item.replace(Some(Rc::downgrade(&item)));
            // If this is the first time the window is opened, the following is unnecessary, but if
            // we reuse a window it's important to reset focus for better keyboard control.
            self.view
                .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL)
                .focus();
        });
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

    pub fn invalidate_controls(&self) {
        self.with_item_if_set(Self::invalidate_controls_internal);
    }

    fn invalidate_controls_internal(&self, item: &dyn Item) {
        self.invalidate_name_edit_control(item, None);
        self.invalidate_tags_edit_control(item, None);
        self.invalidate_control_enabled_check_box(item);
        self.invalidate_feedback_enabled_check_box(item);
        self.invalidate_activation_controls(item);
    }

    fn init_controls(&self) {
        self.view
            .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
            .set_text(format!("{} Control", symbols::arrow_right_symbol()));
        self.view
            .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
            .set_text(format!("{} Feedback", symbols::arrow_left_symbol()));
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX)
            .fill_combo_box_indexed(ActivationType::iter());
        self.invalidate_controls();
    }

    fn invalidate_name_edit_control(&self, item: &dyn Item, initiator: Option<u32>) {
        if initiator == Some(root::ID_MAPPING_NAME_EDIT_CONTROL) {
            return;
        }
        let c = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL);
        c.set_text(item.name());
        c.set_enabled(item.supports_name_change());
    }

    fn invalidate_tags_edit_control(&self, item: &dyn Item, initiator: Option<u32>) {
        if initiator == Some(root::ID_MAPPING_TAGS_EDIT_CONTROL) {
            return;
        }
        let c = self
            .view
            .require_control(root::ID_MAPPING_TAGS_EDIT_CONTROL);
        c.set_text(format_tags_as_csv(item.tags()));
    }

    fn invalidate_control_enabled_check_box(&self, item: &dyn Item) {
        self.view
            .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
            .set_checked(item.control_is_enabled());
    }

    fn invalidate_feedback_enabled_check_box(&self, item: &dyn Item) {
        self.view
            .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
            .set_checked(item.feedback_is_enabled());
    }

    fn invalidate_activation_controls(&self, item: &dyn Item) {
        self.invalidate_activation_type_combo_box(item);
        self.invalidate_activation_setting_1_controls(item);
        self.invalidate_activation_setting_2_controls(item, None);
    }

    fn invalidate_activation_type_combo_box(&self, item: &dyn Item) {
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX)
            .select_combo_box_item_by_index(item.activation_type().into());
    }

    fn invalidate_activation_setting_1_controls(&self, item: &dyn Item) {
        let session = self.session();
        let session = session.borrow();
        let button = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_BUTTON);
        let check_box = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX);
        use ActivationType::*;
        let label = match item.activation_type() {
            Modifiers => {
                self.invalidate_mapping_activation_modifier_controls(
                    &session,
                    button,
                    check_box,
                    item.compartment(),
                    item.modifier_condition_1(),
                );
                Some("Modifier A")
            }
            Bank => {
                button.show();
                check_box.hide();
                let text = menus::get_optional_param_name(
                    &session,
                    item.compartment(),
                    Some(item.bank_condition().param_index()),
                );
                button.set_text(text);
                Some("Parameter")
            }
            TargetValue => {
                button.show();
                check_box.hide();
                let text = if let Some(mapping_id) = item.mapping_id() {
                    if let Some(mapping) =
                        session.find_mapping_by_id(item.compartment(), mapping_id)
                    {
                        let mapping = mapping.borrow();
                        let group = session.find_group_by_id_including_default_group(
                            item.compartment(),
                            mapping.group_id(),
                        );
                        let group_name = if let Some(group) = group {
                            group.borrow().effective_name().to_string()
                        } else {
                            "<Invalid>".to_string()
                        };
                        format!("{} - {}", group_name, mapping.effective_name())
                    } else {
                        "<Invalid>".to_string()
                    }
                } else {
                    menus::NONE.to_string()
                };
                button.set_text(text);
                Some("Mapping")
            }
            _ => {
                button.hide();
                check_box.hide();
                None
            }
        };
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_LABEL_TEXT)
            .set_text_or_hide(label);
    }

    fn invalidate_activation_setting_2_controls(&self, item: &dyn Item, initiator: Option<u32>) {
        if initiator == Some(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL) {
            return;
        }
        let session = self.session();
        let session = session.borrow();
        let button = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_BUTTON);
        let check_box = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX);
        let edit_control = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL);
        use ActivationType::*;
        let label = match item.activation_type() {
            Modifiers => {
                self.invalidate_mapping_activation_modifier_controls(
                    &session,
                    button,
                    check_box,
                    item.compartment(),
                    item.modifier_condition_2(),
                );
                edit_control.hide();
                Some("Modifier B")
            }
            Bank => {
                button.show();
                check_box.hide();
                edit_control.hide();
                let bank_index = item.bank_condition().bank_index();
                let text = menus::get_bank_name(
                    &session,
                    item,
                    item.bank_condition().param_index,
                    bank_index,
                );
                button.set_text(text);
                Some("Bank")
            }
            TargetValue => {
                button.hide();
                check_box.hide();
                edit_control.show();
                edit_control.set_text(item.script());
                Some("Ex: y > 0")
            }
            Eel => {
                button.hide();
                check_box.hide();
                edit_control.show();
                edit_control.set_text(item.script());
                Some("Ex: y = p1 > 0")
            }
            Expression => {
                button.hide();
                check_box.hide();
                edit_control.show();
                edit_control.set_text(item.script());
                Some("Ex: p[0] == 2")
            }
            Always => {
                button.hide();
                check_box.hide();
                edit_control.hide();
                None
            }
        };
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_LABEL_TEXT)
            .set_text_or_hide(label);
    }

    fn invalidate_mapping_activation_modifier_controls(
        &self,
        session: &UnitModel,
        button: Window,
        check_box: Window,
        compartment: CompartmentKind,
        modifier_condition: ModifierConditionModel,
    ) {
        check_box.show();
        button.show();
        let text =
            menus::get_optional_param_name(session, compartment, modifier_condition.param_index());
        button.set_text(text);
        check_box.set_checked(modifier_condition.is_on());
    }

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }

    fn update_control_enabled(&self, session: WeakUnitModel, item: &mut dyn Item) {
        item.set_control_is_enabled(
            session,
            self.view
                .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_feedback_enabled(&self, session: WeakUnitModel, item: &mut dyn Item) {
        item.set_feedback_is_enabled(
            session,
            self.view
                .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_activation_setting_1_on(&self, session: WeakUnitModel, item: &mut dyn Item) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX)
            .is_checked();
        item.set_modifier_condition_1(session, item.modifier_condition_1().with_is_on(checked));
    }

    fn update_activation_setting_2_on(&self, session: WeakUnitModel, item: &mut dyn Item) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX)
            .is_checked();
        item.set_modifier_condition_2(session, item.modifier_condition_2().with_is_on(checked));
    }

    fn update_name(&self, session: WeakUnitModel, item: &mut dyn Item) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        item.set_name(session, value, root::ID_MAPPING_NAME_EDIT_CONTROL);
    }

    fn update_tags(&self, session: WeakUnitModel, item: &mut dyn Item) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_TAGS_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        item.set_tags(
            session,
            parse_tags_from_csv(&value),
            root::ID_MAPPING_TAGS_EDIT_CONTROL,
        );
    }

    fn update_activation_script(&self, session: WeakUnitModel, item: &mut dyn Item) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        item.set_script(session, value, root::ID_MAPPING_ACTIVATION_EDIT_CONTROL);
    }

    fn update_activation_type(&self, session: WeakUnitModel, item: &mut dyn Item) {
        let b = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX);
        item.set_activation_type(
            session,
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid activation type"),
        );
    }

    fn pick_activation_setting_1_option(&self, session: WeakUnitModel, item: SharedItem) {
        use ActivationType::*;
        let activation_type = item.borrow().activation_type();
        match activation_type {
            Modifiers => self.pick_modifier_condition_param(
                session,
                item,
                |item| item.modifier_condition_1(),
                |session, item, value| item.set_modifier_condition_1(session, value),
            ),
            Bank => {
                let (bank_condition, menu) = {
                    let item = item.borrow();
                    let bank_condition = item.bank_condition();
                    let menu = menus::menu_containing_realearn_params(
                        &session,
                        item.compartment(),
                        bank_condition.param_index,
                    );
                    (bank_condition, menu)
                };
                let result = self
                    .view
                    .require_window()
                    .open_popup_menu(menu, Window::cursor_pos());
                if let Some(param_index) = result {
                    item.borrow_mut()
                        .set_bank_condition(session, bank_condition.with_param_index(param_index));
                }
            }
            TargetValue => {
                let menu = {
                    let item = item.borrow();
                    menus::menu_containing_mappings(&session, item.compartment(), item.mapping_id())
                };
                let result = self
                    .view
                    .require_window()
                    .open_popup_menu(menu, Window::cursor_pos());
                if let Some(mapping_id) = result {
                    item.borrow_mut().set_mapping_id(session, mapping_id);
                }
            }
            _ => {}
        }
    }

    fn pick_activation_setting_2_option(&self, session: WeakUnitModel, item: SharedItem) {
        use ActivationType::*;
        let activation_type = item.borrow().activation_type();
        match activation_type {
            Modifiers => self.pick_modifier_condition_param(
                session,
                item,
                |item| item.modifier_condition_2(),
                |session, item, value| item.set_modifier_condition_2(session, value),
            ),
            Bank => {
                let (bank_condition, menu) = {
                    let item = item.borrow();
                    let bank_condition = item.bank_condition();
                    let menu = menus::menu_containing_banks(
                        &session,
                        item.compartment(),
                        bank_condition.param_index,
                        bank_condition.bank_index,
                    );
                    (bank_condition, menu)
                };
                let result = self
                    .view
                    .require_window()
                    .open_popup_menu(menu, Window::cursor_pos());
                if let Some(bank_index) = result {
                    item.borrow_mut()
                        .set_bank_condition(session, bank_condition.with_bank_index(bank_index));
                }
            }
            _ => {}
        };
    }

    fn pick_modifier_condition_param(
        &self,
        session: WeakUnitModel,
        item: SharedItem,
        get: impl FnOnce(&dyn Item) -> ModifierConditionModel,
        set: impl FnOnce(WeakUnitModel, &mut dyn Item, ModifierConditionModel),
    ) {
        let (modifier_condition, menu) = {
            let item = item.borrow();
            let modifier_condition = get(&*item);
            let menu = menus::menu_containing_realearn_params_optional(
                &session,
                item.compartment(),
                modifier_condition.param_index,
            );
            (modifier_condition, menu)
        };
        let result = self
            .view
            .require_window()
            .open_popup_menu(menu, Window::cursor_pos());
        if let Some(param_index) = result {
            set(
                session,
                &mut *item.borrow_mut(),
                modifier_condition.with_param_index(param_index),
            );
        }
    }

    fn with_item_if_set(&self, f: impl FnOnce(&Self, &dyn Item)) {
        if let Some(weak_item) = self.item.borrow().as_ref() {
            if let Some(item) = weak_item.upgrade() {
                f(self, &*item.borrow());
            }
        }
    }

    fn with_session_and_item(&self, f: impl FnOnce(&Self, WeakUnitModel, &mut dyn Item)) {
        self.with_session_and_unborrowed_item(|panel, session, item| {
            f(panel, session, &mut *item.borrow_mut())
        });
    }

    fn with_session_and_unborrowed_item(&self, f: impl FnOnce(&Self, WeakUnitModel, SharedItem)) {
        let opt_item = self.item.borrow();
        let weak_item = opt_item.as_ref().expect("item not set");
        let item = weak_item.upgrade().expect("item gone");
        f(self, self.session.clone(), item);
    }

    pub fn invalidate_due_to_changed_prop(&self, prop: ItemProp, initiator: Option<u32>) {
        self.with_item_if_set(|_, item| {
            self.invoke_programmatically(|| {
                use ItemProp::*;
                match prop {
                    Name => self.invalidate_name_edit_control(item, initiator),
                    Tags => self.invalidate_tags_edit_control(item, initiator),
                    ControlEnabled => self.invalidate_control_enabled_check_box(item),
                    FeedbackEnabled => self.invalidate_feedback_enabled_check_box(item),
                    ActivationType => self.invalidate_activation_controls(item),
                    ModifierCondition1 => self.invalidate_activation_setting_1_controls(item),
                    ModifierCondition2 => {
                        self.invalidate_activation_setting_2_controls(item, initiator)
                    }
                    BankCondition => {
                        self.invalidate_activation_setting_1_controls(item);
                        self.invalidate_activation_setting_2_controls(item, initiator);
                    }
                    Script => self.invalidate_activation_setting_2_controls(item, initiator),
                    MappingId => self.invalidate_activation_setting_1_controls(item),
                };
            });
        });
    }

    fn session(&self) -> SharedUnitModel {
        self.session.upgrade().expect("session gone")
    }
}

impl View for MappingHeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_SHARED_GROUP_MAPPING_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.move_to_dialog_units(self.position);
        self.init_controls();
        true
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        self.view.require_window().parent().unwrap().close();
        // If we return false, the child window is closed.
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_MAPPING_CONTROL_ENABLED_CHECK_BOX => {
                self.with_session_and_item(Self::update_control_enabled);
            }
            ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX => {
                self.with_session_and_item(Self::update_feedback_enabled);
            }
            ID_MAPPING_ACTIVATION_SETTING_1_BUTTON => {
                self.with_session_and_unborrowed_item(Self::pick_activation_setting_1_option);
            }
            ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX => {
                self.with_session_and_item(Self::update_activation_setting_1_on);
            }
            ID_MAPPING_ACTIVATION_SETTING_2_BUTTON => {
                self.with_session_and_unborrowed_item(Self::pick_activation_setting_2_option);
            }
            ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX => {
                self.with_session_and_item(Self::update_activation_setting_2_on);
            }
            _ => {}
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX => {
                self.with_session_and_item(Self::update_activation_type);
            }
            _ => {}
        }
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
        use root::*;
        match resource_id {
            ID_MAPPING_NAME_EDIT_CONTROL => {
                self.with_session_and_item(Self::update_name);
            }
            ID_MAPPING_TAGS_EDIT_CONTROL => {
                self.with_session_and_item(Self::update_tags);
            }
            ID_MAPPING_ACTIVATION_EDIT_CONTROL => {
                self.with_session_and_item(Self::update_activation_script);
            }
            _ => return false,
        };
        true
    }

    fn edit_control_focus_killed(self: SharedView<Self>, resource_id: u32) -> bool {
        // This is also called when the window is hidden.
        // The edit control which is currently edited by the user doesn't get invalidated during
        // `edit_control_changed()`, for good reasons. But as soon as the edit control loses
        // focus, we should invalidate it. This is especially important if the user
        // entered an invalid value. Because we are lazy and edit controls are not
        // manipulated very frequently, we just invalidate all controls.
        // If this fails (because the mapping is not filled anymore), it's not a problem.
        self.with_item_if_set(|s, item| match resource_id {
            root::ID_MAPPING_NAME_EDIT_CONTROL => s.invalidate_name_edit_control(item, None),
            root::ID_MAPPING_TAGS_EDIT_CONTROL => s.invalidate_tags_edit_control(item, None),
            _ => {}
        });
        false
    }

    fn help_requested(&self) -> bool {
        if let Some(parent_view) = self.get_parent_view() {
            parent_view.help_requested()
        } else {
            false
        }
    }

    fn mouse_test(self: SharedView<Self>, position: Point<i32>) -> bool {
        if let Some(parent_view) = self.get_parent_view() {
            parent_view.mouse_test(position)
        } else {
            false
        }
    }
}

impl Item for MappingModel {
    fn compartment(&self) -> CompartmentKind {
        self.compartment()
    }

    fn supports_name_change(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn set_name(&mut self, session: WeakUnitModel, name: String, initiator: u32) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::SetName(name),
            Some(initiator),
        );
    }

    fn tags(&self) -> &[Tag] {
        self.tags()
    }

    fn set_tags(&mut self, session: WeakUnitModel, tags: Vec<Tag>, initiator: u32) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::SetTags(tags),
            Some(initiator),
        );
    }

    fn control_is_enabled(&self) -> bool {
        self.control_is_enabled()
    }

    fn set_control_is_enabled(&mut self, session: WeakUnitModel, value: bool) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::SetControlIsEnabled(value),
            None,
        );
    }

    fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled()
    }

    fn set_feedback_is_enabled(&mut self, session: WeakUnitModel, value: bool) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::SetFeedbackIsEnabled(value),
            None,
        );
    }

    fn activation_type(&self) -> ActivationType {
        self.activation_condition_model().activation_type()
    }

    fn set_activation_type(&mut self, session: WeakUnitModel, value: ActivationType) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::ChangeActivationCondition(
                ActivationConditionCommand::SetActivationType(value),
            ),
            None,
        );
    }

    fn modifier_condition_1(&self) -> ModifierConditionModel {
        self.activation_condition_model().modifier_condition_1()
    }

    fn set_modifier_condition_1(&mut self, session: WeakUnitModel, value: ModifierConditionModel) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::ChangeActivationCondition(
                ActivationConditionCommand::SetModifierCondition1(value),
            ),
            None,
        );
    }

    fn modifier_condition_2(&self) -> ModifierConditionModel {
        self.activation_condition_model().modifier_condition_2()
    }

    fn set_modifier_condition_2(&mut self, session: WeakUnitModel, value: ModifierConditionModel) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::ChangeActivationCondition(
                ActivationConditionCommand::SetModifierCondition2(value),
            ),
            None,
        );
    }

    fn bank_condition(&self) -> BankConditionModel {
        self.activation_condition_model().bank_condition()
    }

    fn set_bank_condition(&mut self, session: WeakUnitModel, value: BankConditionModel) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::ChangeActivationCondition(
                ActivationConditionCommand::SetBankCondition(value),
            ),
            None,
        );
    }

    fn script(&self) -> &str {
        self.activation_condition_model().script()
    }

    fn set_script(&mut self, session: WeakUnitModel, value: String, initiator: u32) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::ChangeActivationCondition(ActivationConditionCommand::SetScript(value)),
            Some(initiator),
        );
    }

    fn mapping_id(&self) -> Option<MappingId> {
        self.activation_condition_model().mapping_id()
    }

    fn set_mapping_id(&mut self, session: WeakUnitModel, value: Option<MappingId>) {
        UnitModel::change_mapping_from_ui_simple(
            session,
            self,
            MappingCommand::ChangeActivationCondition(ActivationConditionCommand::SetMappingId(
                value,
            )),
            None,
        );
    }
}

impl Item for GroupModel {
    fn compartment(&self) -> CompartmentKind {
        self.compartment()
    }

    fn supports_name_change(&self) -> bool {
        !self.is_default_group()
    }

    fn name(&self) -> &str {
        self.effective_name()
    }

    fn set_name(&mut self, session: WeakUnitModel, name: String, initiator: u32) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::SetName(name),
            Some(initiator),
        );
    }

    fn tags(&self) -> &[Tag] {
        self.tags()
    }

    fn set_tags(&mut self, session: WeakUnitModel, tags: Vec<Tag>, initiator: u32) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::SetTags(tags),
            Some(initiator),
        );
    }

    fn control_is_enabled(&self) -> bool {
        self.control_is_enabled()
    }

    fn set_control_is_enabled(&mut self, session: WeakUnitModel, value: bool) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::SetControlIsEnabled(value),
            None,
        );
    }

    fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled()
    }

    fn set_feedback_is_enabled(&mut self, session: WeakUnitModel, value: bool) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::SetFeedbackIsEnabled(value),
            None,
        );
    }

    fn activation_type(&self) -> ActivationType {
        self.activation_condition_model().activation_type()
    }

    fn set_activation_type(&mut self, session: WeakUnitModel, value: ActivationType) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::ChangeActivationCondition(ActivationConditionCommand::SetActivationType(
                value,
            )),
            None,
        );
    }

    fn modifier_condition_1(&self) -> ModifierConditionModel {
        self.activation_condition_model().modifier_condition_1()
    }

    fn set_modifier_condition_1(&mut self, session: WeakUnitModel, value: ModifierConditionModel) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::ChangeActivationCondition(
                ActivationConditionCommand::SetModifierCondition1(value),
            ),
            None,
        );
    }

    fn modifier_condition_2(&self) -> ModifierConditionModel {
        self.activation_condition_model().modifier_condition_2()
    }

    fn set_modifier_condition_2(&mut self, session: WeakUnitModel, value: ModifierConditionModel) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::ChangeActivationCondition(
                ActivationConditionCommand::SetModifierCondition2(value),
            ),
            None,
        );
    }

    fn bank_condition(&self) -> BankConditionModel {
        self.activation_condition_model().bank_condition()
    }

    fn set_bank_condition(&mut self, session: WeakUnitModel, value: BankConditionModel) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::ChangeActivationCondition(ActivationConditionCommand::SetBankCondition(
                value,
            )),
            None,
        );
    }

    fn script(&self) -> &str {
        self.activation_condition_model().script()
    }

    fn set_script(&mut self, session: WeakUnitModel, value: String, initiator: u32) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::ChangeActivationCondition(ActivationConditionCommand::SetScript(value)),
            Some(initiator),
        );
    }

    fn mapping_id(&self) -> Option<MappingId> {
        self.activation_condition_model().mapping_id()
    }

    fn set_mapping_id(&mut self, session: WeakUnitModel, value: Option<MappingId>) {
        UnitModel::change_group_from_ui_simple(
            session,
            self,
            GroupCommand::ChangeActivationCondition(ActivationConditionCommand::SetMappingId(
                value,
            )),
            None,
        );
    }
}
