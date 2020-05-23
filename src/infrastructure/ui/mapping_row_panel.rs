use crate::domain::{MappingModel, SharedMappingModel};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use crate::infrastructure::ui::MappingPanelManager;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::Rc;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

pub type SharedMappingPanelManager = Rc<RefCell<MappingPanelManager>>;

/// Panel containing the summary data of one mapping and buttons such as "Remove".
pub struct MappingRowPanel {
    view: ViewContext,
    session: SharedSession,
    row_index: u32,
    // We use virtual scrolling in order to be able to show a large amount of rows without any
    // performance issues. That means there's a fixed number of mapping rows and they just
    // display different mappings depending on the current scroll position. If there are less
    // mappings than the fixed number, some rows remain unused. In this case their mapping is
    // `None`, which will make the row hide itself.
    mapping: RefCell<Option<SharedMappingModel>>,
    // Fires when a mapping is about to change.
    mapping_will_change_subject: RefCell<LocalSubject<'static, (), ()>>,
    mapping_panel_manager: SharedMappingPanelManager,
}

impl MappingRowPanel {
    pub fn new(
        session: SharedSession,
        row_index: u32,
        mapping_panel_manager: SharedMappingPanelManager,
    ) -> MappingRowPanel {
        MappingRowPanel {
            view: Default::default(),
            session,
            row_index,
            mapping_will_change_subject: Default::default(),
            mapping: None.into(),
            mapping_panel_manager,
        }
    }

    pub fn set_mapping(self: &SharedView<Self>, mapping: Option<SharedMappingModel>) {
        self.mapping_will_change_subject.borrow_mut().next(());
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
    }

    fn invalidate_name_label(&self, mapping: &MappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_GROUP_BOX)
            .set_text(mapping.name.get_ref().as_str());
    }

    fn invalidate_source_label(&self, mapping: &MappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_text(mapping.source_model.to_string());
    }

    fn invalidate_target_label(&self, mapping: &MappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_text(
                mapping
                    .target_model
                    .with_context(self.session.borrow().containing_fx())
                    .to_string(),
            );
    }

    fn invalidate_learn_source_button(&self, mapping: &MappingModel) {
        // TODO
    }

    fn invalidate_learn_target_button(&self, mapping: &MappingModel) {
        // TODO
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

    fn register_listeners(self: &SharedView<Self>, mapping: &MappingModel) {
        self.when(mapping.name.changed(), |view| {
            view.with_mapping(Self::invalidate_name_label);
        });
        self.when(mapping.source_model.changed(), |view| {
            view.with_mapping(Self::invalidate_source_label);
        });
        self.when(mapping.target_model.changed(), |view| {
            view.with_mapping(Self::invalidate_target_label);
        });
        self.when(mapping.control_is_enabled.changed(), |view| {
            view.with_mapping(Self::invalidate_control_check_box);
        });
        self.when(mapping.feedback_is_enabled.changed(), |view| {
            view.with_mapping(Self::invalidate_feedback_check_box);
        });
        // TODO currentlySourceLearningMapping
        // TODO currentlyTargetLearningMapping
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
            .merge(self.mapping_will_change_subject.borrow().clone())
    }

    fn require_mapping(&self) -> SharedMappingModel {
        self.mapping.borrow().clone().expect("no mapping")
    }

    fn edit_mapping(&self) {
        self.mapping_panel_manager
            .borrow_mut()
            .edit_mapping(self.require_mapping());
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static,
    ) {
        when_async(event, reaction, &self, self.closed_or_mapping_will_change());
    }
}

impl View for MappingRowPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROW_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(self.row_index * 48)));
        window.hide();
        false
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            root::ID_MAPPING_ROW_EDIT_BUTTON => self.edit_mapping(),
            _ => {}
        }
    }
}
