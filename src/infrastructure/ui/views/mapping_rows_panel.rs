use std::cell::RefCell;
use std::rc::Rc;

use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;

use crate::domain::Session;
use crate::infrastructure::common::bindings::root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX;
use crate::infrastructure::ui::framework::{Window, WindowListener};

#[derive(Debug)]
pub struct MappingRowsPanel {
    session: Rc<RefCell<Session<'static>>>,
}

impl MappingRowsPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> MappingRowsPanel {
        MappingRowsPanel { session }
    }
}

impl WindowListener for MappingRowsPanel {}
