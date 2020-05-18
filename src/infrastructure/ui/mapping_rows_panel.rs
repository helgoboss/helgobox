use std::cell::{Cell, RefCell};
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;

use crate::domain::Session;
use crate::infrastructure::common::bindings::root;
use swell_ui::{DialogUnits, Point, View, Window};

#[derive(Debug)]
pub struct MappingRowsPanel {
    session: Rc<RefCell<Session<'static>>>,
    window: Cell<Option<Window>>,
}

impl MappingRowsPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> MappingRowsPanel {
        MappingRowsPanel {
            session,
            window: None.into(),
        }
    }
}

impl View for MappingRowsPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROWS_DIALOG
    }

    fn window(&self) -> &Cell<Option<Window>> {
        &self.window
    }

    fn opened(self: Rc<Self>, window: Window) -> bool {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(78)));
        true
    }
}
