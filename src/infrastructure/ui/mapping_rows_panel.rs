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
use swell_ui::{DialogUnits, Point, View, ViewContext, Window};

pub struct MappingRowsPanel {
    view: ViewContext,
    session: SharedSession,
}

impl MappingRowsPanel {
    pub fn new(session: SharedSession) -> MappingRowsPanel {
        MappingRowsPanel {
            view: Default::default(),
            session,
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

    fn opened(self: Rc<Self>, window: Window) -> bool {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(78)));
        true
    }
}
