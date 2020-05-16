use std::cell::{Cell, RefCell};
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;

use crate::domain::Session;
use crate::infrastructure::common::bindings::root::{
    ID_MAPPING_ROWS_DIALOG, ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX,
};
use crate::infrastructure::ui::framework::{create_window, DialogUnits, Point, View, Window};

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

    pub fn open(self: Rc<Self>, parent_window: Window) {
        create_window(self, ID_MAPPING_ROWS_DIALOG, parent_window);
    }
}

impl View for MappingRowsPanel {
    fn dialog_resource_id(&self) -> u32 {
        ID_MAPPING_ROWS_DIALOG
    }

    fn window(&self) -> &Cell<Option<Window>> {
        &self.window
    }

    fn opened(self: Rc<Self>, window: Window) {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(78)))
    }
}
