use std::cell::{Cell, RefCell};
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;

use crate::domain::Session;
use crate::infrastructure::common::bindings::root;
use crate::infrastructure::common::SharedSession;
use crate::infrastructure::ui::MappingRowPanel;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

pub struct MappingRowsPanel {
    view: ViewContext,
    session: SharedSession,
    rows: Vec<SharedView<MappingRowPanel>>,
    scroll_position: Cell<usize>,
}

impl MappingRowsPanel {
    pub fn new(session: SharedSession) -> MappingRowsPanel {
        MappingRowsPanel {
            view: Default::default(),
            rows: (0..5)
                .map(|i| MappingRowPanel::new(session.clone(), i).into())
                .collect(),
            session,
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
        true
    }
}
