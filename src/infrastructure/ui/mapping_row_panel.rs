use crate::domain::SharedMappingModel;
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use rx_util::UnitEvent;
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
        // self.invalidate_all_controls();
        // self.register_listeners();
        false
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        use root::*;
        match resource_id {
            _ => unreachable!(),
        }
    }
}
