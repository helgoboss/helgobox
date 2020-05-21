use crate::domain::{MappingModel, SharedMappingModel};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use rx_util::UnitEvent;
use std::cell::RefCell;
use std::rc::Rc;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

/// Panel containing the summary data of one mapping and buttons such as "Remove".
pub struct MappingRowPanel {
    view: ViewContext,
    session: SharedSession,
    row_index: u32,
    mapping: Option<SharedMappingModel>,
}

impl MappingRowPanel {
    pub fn new(session: SharedSession, row_index: u32) -> MappingRowPanel {
        MappingRowPanel {
            view: Default::default(),
            session,
            row_index,
            mapping: None,
        }
    }
}

impl MappingRowPanel {
    fn invalidate_all_controls(&self) {
        match &self.mapping {
            None => self.view.require_window().hide(),
            Some(mapping) => {
                self.view.require_window().show();
                self.invalidate_name_label(&mapping);
                self.invalidate_source_label(&mapping);
                self.invalidate_target_label(&mapping);
                self.invalidate_learn_source_button(&mapping);
                self.invalidate_learn_target_button(&mapping);
                self.invalidate_control_check_box(&mapping);
                self.invalidate_feedback_check_box(&mapping);
            }
        }
    }

    fn invalidate_name_label(&self, mapping: &SharedMappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_GROUP_BOX)
            .set_text(mapping.borrow().name.get_ref().as_str());
    }

    fn invalidate_source_label(&self, mapping: &SharedMappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_text(mapping.borrow().source_model.to_string());
    }

    fn invalidate_target_label(&self, mapping: &SharedMappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_text(mapping.borrow().target_model.to_string());
    }

    fn invalidate_learn_source_button(&self, mapping: &SharedMappingModel) {
        // TODO
    }

    fn invalidate_learn_target_button(&self, mapping: &SharedMappingModel) {
        // TODO
    }

    fn invalidate_control_check_box(&self, mapping: &SharedMappingModel) {
        // TODO
    }

    fn invalidate_feedback_check_box(&self, mapping: &SharedMappingModel) {
        // TODO
    }

    fn register_listeners(self: SharedView<Self>) {
        let session = self.session.borrow();
        // TODO Also do registrations done in afterMappingChange
        // self.when(session.let_matched_events_through.changed(), |view| {
        //     view.invalidate_let_matched_events_through_check_box()
        // });
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static,
    ) {
        when_async(event, reaction, &self, self.view.closed());
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
        self.invalidate_all_controls();
        self.register_listeners();
        false
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            _ => unreachable!(),
        }
    }
}
