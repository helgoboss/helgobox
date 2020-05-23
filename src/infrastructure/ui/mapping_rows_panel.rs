use std::cell::{Cell, RefCell};
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;

use crate::domain::{MappingModel, Session, SharedMappingModel};
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::scheduling::when_async;
use crate::infrastructure::ui::{
    MappingPanel, MappingPanelManager, MappingRowPanel, SharedMappingPanelManager,
};
use rx_util::UnitEvent;
use std::collections::HashMap;
use std::ops::DerefMut;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

pub struct MappingRowsPanel {
    view: ViewContext,
    session: SharedSession,
    rows: Vec<SharedView<MappingRowPanel>>,
    mapping_panel_manager: SharedMappingPanelManager,
    scroll_position: Cell<usize>,
}

impl MappingRowsPanel {
    pub fn new(session: SharedSession) -> MappingRowsPanel {
        let mapping_panel_manager = MappingPanelManager::new(session.clone());
        let mapping_panel_manager = Rc::new(RefCell::new(mapping_panel_manager));
        MappingRowsPanel {
            view: Default::default(),
            rows: (0..5)
                .map(|i| {
                    let panel =
                        MappingRowPanel::new(session.clone(), i, mapping_panel_manager.clone());
                    SharedView::new(panel)
                })
                .collect(),
            session,
            mapping_panel_manager,
            scroll_position: 0.into(),
        }
    }

    fn open_mapping_rows(&self, window: Window) {
        for row in self.rows.iter() {
            row.clone().open(window);
        }
    }

    /// Let mapping rows reflect the correct mappings.
    fn invalidate_mapping_rows(&self) {
        let mut row_index = 0;
        let mapping_count = self.session.borrow().mapping_count();
        for i in (self.scroll_position.get()..mapping_count) {
            if row_index >= self.rows.len() {
                break;
            }
            let mapping = self
                .session
                .borrow()
                .mapping_by_index(i)
                .expect("impossible");
            self.rows
                .get(row_index)
                .expect("impossible")
                .set_mapping(Some(mapping));
            row_index += 1;
        }
        // If there are unused rows, clear them
        for i in (row_index..self.rows.len()) {
            self.rows.get(i).expect("impossible").set_mapping(None);
        }
    }

    fn register_listeners(self: SharedView<Self>) {
        let session = self.session.borrow();
        self.when(session.mappings_changed(), |view| {
            view.invalidate_mapping_rows();
            view.mapping_panel_manager
                .borrow_mut()
                .close_orphan_panels();
        });
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static,
    ) {
        when_async(event, reaction, &self, self.view.closed());
    }
}

impl View for MappingRowsPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROWS_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(78)));
        self.open_mapping_rows(window);
        self.invalidate_mapping_rows();
        self.register_listeners();
        true
    }
}
