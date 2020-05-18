use std::cell::{Cell, RefCell};
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;

use crate::domain::Session;
use crate::infrastructure::common::bindings::root;
use swell_ui::{DialogUnits, Point, View, ViewContext, Window};

#[derive(Debug)]
pub struct MappingRowsPanel {
    view_context: ViewContext,
    session: Rc<RefCell<Session<'static>>>,
}

impl MappingRowsPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> MappingRowsPanel {
        MappingRowsPanel {
            view_context: Default::default(),
            session,
        }
    }
}

impl View for MappingRowsPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROWS_DIALOG
    }

    fn view_context(&self) -> &ViewContext {
        &self.view_context
    }

    fn opened(self: Rc<Self>, window: Window) -> bool {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(78)));
        true
    }
}
