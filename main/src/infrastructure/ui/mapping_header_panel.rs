use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::symbols;

use enum_iterator::IntoEnumIterator;
use std::cell::{Cell, RefCell};
use std::convert::TryInto;

use std::rc::{Rc, Weak};

use crate::application::{
    ActivationType, GroupModel, MappingModel, ModifierConditionModel, ProgramConditionModel,
    SharedSession, WeakSession,
};
use crate::domain::{MappingCompartment, COMPARTMENT_PARAMETER_COUNT, PLUGIN_PARAMETER_COUNT};
use std::fmt::Debug;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

type SharedItem = Rc<RefCell<dyn Item>>;
type WeakItem = Weak<RefCell<dyn Item>>;

#[derive(Debug)]
pub struct MappingHeaderPanel {
    view: ViewContext,
    session: WeakSession,
    item: RefCell<Option<WeakItem>>,
    is_invoked_programmatically: Cell<bool>,
    position: Point<DialogUnits>,
}

pub trait Item: Debug {
    fn compartment(&self) -> MappingCompartment;
    fn supports_name_change(&self) -> bool;
    fn supports_activation(&self) -> bool;
    fn name(&self) -> &str;
    fn set_name(&mut self, name: String);
    fn control_is_enabled(&self) -> bool;
    fn set_control_is_enabled(&mut self, value: bool);
    fn feedback_is_enabled(&self) -> bool;
    fn set_feedback_is_enabled(&mut self, value: bool);
    fn activation_type(&self) -> ActivationType;
    fn set_activation_type(&mut self, value: ActivationType);
    fn modifier_condition_1(&self) -> ModifierConditionModel;
    fn set_modifier_condition_1(&mut self, value: ModifierConditionModel);
    fn modifier_condition_2(&self) -> ModifierConditionModel;
    fn set_modifier_condition_2(&mut self, value: ModifierConditionModel);
    fn program_condition(&self) -> ProgramConditionModel;
    fn set_program_condition(&mut self, value: ProgramConditionModel);
    fn eel_condition(&self) -> &str;
    fn set_eel_condition(&mut self, value: String);
}

pub enum ItemProp {
    Name,
    ControlEnabled,
    FeedbackEnabled,
    ActivationType,
    ModifierCondition1,
    ModifierCondition2,
    ProgramCondition,
    EelCondition,
}

impl MappingHeaderPanel {
    pub fn new(
        session: WeakSession,
        position: Point<DialogUnits>,
        initial_item: Option<WeakItem>,
    ) -> MappingHeaderPanel {
        MappingHeaderPanel {
            view: Default::default(),
            session,
            item: RefCell::new(initial_item),
            is_invoked_programmatically: false.into(),
            position,
        }
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
        self.invalidate_name_edit_control(item);
        self.invalidate_control_enabled_check_box(item);
        self.invalidate_feedback_enabled_check_box(item);
        self.invalidate_activation_controls(item);
    }

    fn init_controls(&self) {
        self.view
            .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
            .set_text(format!("{} Control enabled", symbols::arrow_right_symbol()));
        self.view
            .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
            .set_text(format!("{} Feedback enabled", symbols::arrow_left_symbol()));
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX)
            .fill_combo_box_indexed(ActivationType::into_enum_iter());
        self.invalidate_controls();
    }

    fn invalidate_name_edit_control(&self, item: &dyn Item) {
        let c = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL);
        c.set_text_if_not_focused(item.name());
        c.set_enabled(item.supports_name_change());
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
        self.invalidate_activation_control_appearance(item);
        self.invalidate_activation_type_combo_box(item);
        self.invalidate_activation_setting_1_controls(item);
        self.invalidate_activation_setting_2_controls(item);
        self.invalidate_activation_eel_condition_edit_control(item);
    }

    fn invalidate_activation_control_appearance(&self, item: &dyn Item) {
        self.invalidate_activation_control_labels(item);
        self.fill_activation_combo_boxes(item);
        self.invalidate_activation_control_visibilities(item);
    }

    fn invalidate_activation_control_labels(&self, item: &dyn Item) {
        use ActivationType::*;
        let label = match item.activation_type() {
            Always => None,
            Modifiers => Some(("Modifier A", "Modifier B")),
            Program => Some(("Bank", "Program")),
            Eel => None,
        };
        if let Some((first, second)) = label {
            self.view
                .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_LABEL_TEXT)
                .set_text(first);
            self.view
                .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_LABEL_TEXT)
                .set_text(second);
        }
    }

    fn fill_activation_combo_boxes(&self, item: &dyn Item) {
        use ActivationType::*;
        let compartment = item.compartment();
        match item.activation_type() {
            Modifiers => {
                self.fill_combo_box_with_realearn_params(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    true,
                    compartment,
                );
                self.fill_combo_box_with_realearn_params(
                    root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
                    true,
                    compartment,
                );
            }
            Program => {
                self.fill_combo_box_with_realearn_params(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    false,
                    compartment,
                );
                self.view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX)
                    .fill_combo_box_with_data_vec(
                        (0..=99).map(|i| (i as isize, i.to_string())).collect(),
                    )
            }
            _ => {}
        };
    }

    fn invalidate_activation_control_visibilities(&self, item: &dyn Item) {
        let show = item.supports_activation();
        let activation_type = item.activation_type();
        self.show_if(
            show,
            &[
                root::ID_MAPPING_ACTIVATION_LABEL,
                root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX,
            ],
        );
        self.show_if(
            show && (activation_type == ActivationType::Modifiers
                || activation_type == ActivationType::Program),
            &[
                root::ID_MAPPING_ACTIVATION_SETTING_1_LABEL_TEXT,
                root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                root::ID_MAPPING_ACTIVATION_SETTING_2_LABEL_TEXT,
                root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
            ],
        );
        self.show_if(
            show && activation_type == ActivationType::Modifiers,
            &[
                root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX,
                root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX,
            ],
        );
        self.show_if(
            show && activation_type == ActivationType::Eel,
            &[
                root::ID_MAPPING_ACTIVATION_EEL_LABEL_TEXT,
                root::ID_MAPPING_ACTIVATION_EDIT_CONTROL,
            ],
        );
    }

    fn invalidate_activation_type_combo_box(&self, item: &dyn Item) {
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX)
            .select_combo_box_item_by_index(item.activation_type().into())
            .unwrap();
    }

    fn invalidate_activation_setting_1_controls(&self, item: &dyn Item) {
        use ActivationType::*;
        match item.activation_type() {
            Modifiers => {
                self.invalidate_mapping_activation_modifier_controls(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX,
                    item.modifier_condition_1(),
                );
            }
            Program => {
                let param_index = item.program_condition().param_index();
                self.view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX)
                    .select_combo_box_item_by_index(param_index as _)
                    .unwrap();
            }
            _ => {}
        };
    }

    fn invalidate_activation_setting_2_controls(&self, item: &dyn Item) {
        use ActivationType::*;
        match item.activation_type() {
            Modifiers => {
                self.invalidate_mapping_activation_modifier_controls(
                    root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
                    root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX,
                    item.modifier_condition_2(),
                );
            }
            Program => {
                let program_index = item.program_condition().program_index();
                self.view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX)
                    .select_combo_box_item_by_index(program_index as _)
                    .unwrap();
            }
            _ => {}
        };
    }

    fn invalidate_mapping_activation_modifier_controls(
        &self,
        combo_box_id: u32,
        check_box_id: u32,
        modifier_condition: ModifierConditionModel,
    ) {
        let b = self.view.require_control(combo_box_id);
        match modifier_condition.param_index() {
            None => {
                b.select_combo_box_item_by_data(-1).unwrap();
            }
            Some(i) => {
                b.select_combo_box_item_by_data(i as _).unwrap();
            }
        };
        self.view
            .require_control(check_box_id)
            .set_checked(modifier_condition.is_on());
    }

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }

    fn update_control_enabled(&self, item: &mut dyn Item) {
        item.set_control_is_enabled(
            self.view
                .require_control(root::ID_MAPPING_CONTROL_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_feedback_enabled(&self, item: &mut dyn Item) {
        item.set_feedback_is_enabled(
            self.view
                .require_control(root::ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_activation_setting_1_on(&self, item: &mut dyn Item) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX)
            .is_checked();
        item.set_modifier_condition_1(item.modifier_condition_1().with_is_on(checked));
    }

    fn update_activation_setting_2_on(&self, item: &mut dyn Item) {
        let checked = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX)
            .is_checked();
        item.set_modifier_condition_2(item.modifier_condition_2().with_is_on(checked));
    }

    fn update_name(&self, item: &mut dyn Item) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_NAME_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        item.set_name(value);
    }

    fn update_activation_eel_condition(&self, item: &mut dyn Item) {
        let value = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL)
            .text()
            .unwrap_or_else(|_| "".to_string());
        item.set_eel_condition(value);
    }

    fn update_activation_type(&self, item: &mut dyn Item) {
        let b = self
            .view
            .require_control(root::ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX);
        item.set_activation_type(
            b.selected_combo_box_item_index()
                .try_into()
                .expect("invalid activation type"),
        );
    }

    fn update_activation_setting_1_option(&self, item: &mut dyn Item) {
        use ActivationType::*;
        match item.activation_type() {
            Modifiers => {
                self.update_activation_setting_option(
                    root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX,
                    item,
                    |it| it.modifier_condition_1(),
                    |it, c| it.set_modifier_condition_1(c),
                );
            }
            Program => {
                let b = self
                    .view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX);
                let value = b.selected_combo_box_item_index() as u32;
                item.set_program_condition(item.program_condition().with_param_index(value));
            }
            _ => {}
        };
    }

    fn update_activation_setting_2_option(&self, item: &mut dyn Item) {
        use ActivationType::*;
        match item.activation_type() {
            Modifiers => {
                self.update_activation_setting_option(
                    root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX,
                    item,
                    |it| it.modifier_condition_2(),
                    |it, c| it.set_modifier_condition_2(c),
                );
            }
            Program => {
                let b = self
                    .view
                    .require_control(root::ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX);
                let value = b.selected_combo_box_item_index() as u32;
                item.set_program_condition(item.program_condition().with_program_index(value));
            }
            _ => {}
        };
    }

    fn update_activation_setting_option(
        &self,
        combo_box_id: u32,
        item: &mut dyn Item,
        get: impl FnOnce(&dyn Item) -> ModifierConditionModel,
        set: impl FnOnce(&mut dyn Item, ModifierConditionModel),
    ) {
        let b = self.view.require_control(combo_box_id);
        let value = match b.selected_combo_box_item_data() {
            -1 => None,
            id => Some(id as u32),
        };
        let current = get(item);
        set(item, current.with_param_index(value));
    }

    fn invalidate_activation_eel_condition_edit_control(&self, item: &dyn Item) {
        self.view
            .require_control(root::ID_MAPPING_ACTIVATION_EDIT_CONTROL)
            .set_text_if_not_focused(item.eel_condition());
    }

    fn show_if(&self, condition: bool, control_resource_ids: &[u32]) {
        for id in control_resource_ids {
            self.view.require_control(*id).set_visible(condition);
        }
    }

    fn with_item_if_set(&self, f: impl FnOnce(&Self, &dyn Item)) {
        if let Some(weak_item) = self.item.borrow().as_ref() {
            if let Some(item) = weak_item.upgrade() {
                f(self, &*item.borrow());
            }
        }
    }

    fn with_mutable_item(&self, f: impl FnOnce(&Self, &mut dyn Item)) {
        let opt_item = self.item.borrow();
        let weak_item = opt_item.as_ref().expect("item not set");
        let item = weak_item.upgrade().expect("item gone");
        f(self, &mut *item.borrow_mut());
    }

    pub fn invalidate_due_to_changed_prop(&self, prop: ItemProp) {
        self.with_item_if_set(|_, item| {
            self.invoke_programmatically(|| {
                use ItemProp::*;
                match prop {
                    Name => self.invalidate_name_edit_control(item),
                    ControlEnabled => self.invalidate_control_enabled_check_box(item),
                    FeedbackEnabled => self.invalidate_feedback_enabled_check_box(item),
                    ActivationType => self.invalidate_activation_controls(item),
                    ModifierCondition1 => self.invalidate_activation_setting_1_controls(item),
                    ModifierCondition2 => self.invalidate_activation_setting_2_controls(item),
                    ProgramCondition => {
                        self.invalidate_activation_setting_1_controls(item);
                        self.invalidate_activation_setting_2_controls(item);
                    }
                    EelCondition => self.invalidate_activation_eel_condition_edit_control(item),
                };
            });
        });
    }

    fn fill_combo_box_with_realearn_params(
        &self,
        control_id: u32,
        with_none: bool,
        compartment: MappingCompartment,
    ) {
        let b = self.view.require_control(control_id);
        let start = if with_none {
            vec![(-1isize, "<None>".to_string())]
        } else {
            vec![]
        };
        let session = self.session();
        let session = session.borrow();
        b.fill_combo_box_with_data_small(start.into_iter().chain(
            (0..COMPARTMENT_PARAMETER_COUNT).map(|i| {
                (
                    i as isize,
                    format!(
                        "{}. {}",
                        i + 1,
                        session.get_qualified_parameter_name(compartment, i)
                    ),
                )
            }),
        ));
    }

    fn session(&self) -> SharedSession {
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
        window.move_to(self.position);
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
                self.with_mutable_item(Self::update_control_enabled);
            }
            ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX => {
                self.with_mutable_item(Self::update_feedback_enabled);
            }
            ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX => {
                self.with_mutable_item(Self::update_activation_setting_1_on);
            }
            ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX => {
                self.with_mutable_item(Self::update_activation_setting_2_on);
            }
            _ => unreachable!(),
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX => {
                self.with_mutable_item(Self::update_activation_type);
            }
            ID_MAPPING_ACTIVATION_SETTING_1_COMBO_BOX => {
                self.with_mutable_item(Self::update_activation_setting_1_option);
            }
            ID_MAPPING_ACTIVATION_SETTING_2_COMBO_BOX => {
                self.with_mutable_item(Self::update_activation_setting_2_option);
            }
            _ => unreachable!(),
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
                self.with_mutable_item(Self::update_name);
            }
            ID_MAPPING_ACTIVATION_EDIT_CONTROL => {
                self.with_mutable_item(Self::update_activation_eel_condition);
            }
            _ => return false,
        };
        true
    }

    fn edit_control_focus_killed(self: SharedView<Self>, _resource_id: u32) -> bool {
        // This is also called when the window is hidden.
        // The edit control which is currently edited by the user doesn't get invalidated during
        // `edit_control_changed()`, for good reasons. But as soon as the edit control loses
        // focus, we should invalidate it. This is especially important if the user
        // entered an invalid value. Because we are lazy and edit controls are not
        // manipulated very frequently, we just invalidate all controls.
        // If this fails (because the mapping is not filled anymore), it's not a problem.
        self.with_item_if_set(Self::invalidate_controls_internal);
        false
    }
}

impl Item for MappingModel {
    fn compartment(&self) -> MappingCompartment {
        self.compartment()
    }

    fn supports_name_change(&self) -> bool {
        true
    }

    fn supports_activation(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.name.get_ref()
    }

    fn set_name(&mut self, name: String) {
        self.name.set(name);
    }

    fn control_is_enabled(&self) -> bool {
        self.control_is_enabled.get()
    }

    fn set_control_is_enabled(&mut self, value: bool) {
        self.control_is_enabled.set(value);
    }

    fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled.get()
    }

    fn set_feedback_is_enabled(&mut self, value: bool) {
        self.feedback_is_enabled.set(value);
    }

    fn activation_type(&self) -> ActivationType {
        self.activation_condition_model.activation_type.get()
    }

    fn set_activation_type(&mut self, value: ActivationType) {
        self.activation_condition_model.activation_type.set(value);
    }

    fn modifier_condition_1(&self) -> ModifierConditionModel {
        self.activation_condition_model.modifier_condition_1.get()
    }

    fn set_modifier_condition_1(&mut self, value: ModifierConditionModel) {
        self.activation_condition_model
            .modifier_condition_1
            .set(value);
    }

    fn modifier_condition_2(&self) -> ModifierConditionModel {
        self.activation_condition_model.modifier_condition_2.get()
    }

    fn set_modifier_condition_2(&mut self, value: ModifierConditionModel) {
        self.activation_condition_model
            .modifier_condition_2
            .set(value);
    }

    fn program_condition(&self) -> ProgramConditionModel {
        self.activation_condition_model.program_condition.get()
    }

    fn set_program_condition(&mut self, value: ProgramConditionModel) {
        self.activation_condition_model.program_condition.set(value);
    }

    fn eel_condition(&self) -> &str {
        self.activation_condition_model.eel_condition.get_ref()
    }

    fn set_eel_condition(&mut self, value: String) {
        self.activation_condition_model.eel_condition.set(value);
    }
}

impl Item for GroupModel {
    fn compartment(&self) -> MappingCompartment {
        self.compartment()
    }

    fn supports_name_change(&self) -> bool {
        !self.is_default_group()
    }

    fn supports_activation(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        if self.is_default_group() {
            "<Default>"
        } else {
            self.name.get_ref()
        }
    }

    fn set_name(&mut self, name: String) {
        self.name.set(name);
    }

    fn control_is_enabled(&self) -> bool {
        self.control_is_enabled.get()
    }

    fn set_control_is_enabled(&mut self, value: bool) {
        self.control_is_enabled.set(value);
    }

    fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled.get()
    }

    fn set_feedback_is_enabled(&mut self, value: bool) {
        self.feedback_is_enabled.set(value);
    }

    fn activation_type(&self) -> ActivationType {
        self.activation_condition_model.activation_type.get()
    }

    fn set_activation_type(&mut self, value: ActivationType) {
        self.activation_condition_model.activation_type.set(value);
    }

    fn modifier_condition_1(&self) -> ModifierConditionModel {
        self.activation_condition_model.modifier_condition_1.get()
    }

    fn set_modifier_condition_1(&mut self, value: ModifierConditionModel) {
        self.activation_condition_model
            .modifier_condition_1
            .set(value);
    }

    fn modifier_condition_2(&self) -> ModifierConditionModel {
        self.activation_condition_model.modifier_condition_2.get()
    }

    fn set_modifier_condition_2(&mut self, value: ModifierConditionModel) {
        self.activation_condition_model
            .modifier_condition_2
            .set(value);
    }

    fn program_condition(&self) -> ProgramConditionModel {
        self.activation_condition_model.program_condition.get()
    }

    fn set_program_condition(&mut self, value: ProgramConditionModel) {
        self.activation_condition_model.program_condition.set(value);
    }

    fn eel_condition(&self) -> &str {
        self.activation_condition_model.eel_condition.get_ref()
    }

    fn set_eel_condition(&mut self, value: String) {
        self.activation_condition_model.eel_condition.set(value);
    }
}
