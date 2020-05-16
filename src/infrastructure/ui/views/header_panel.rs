use crate::domain::Session;
use crate::infrastructure::common::bindings::root::{
    ID_MAPPINGS_DIALOG, ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX,
};
use crate::infrastructure::ui::framework::{create_window, View, Window};
use c_str_macro::c_str;
use helgoboss_midi::Channel;
use reaper_high::Reaper;
use reaper_low::Swell;
use rxrust::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    session: Rc<RefCell<Session<'static>>>,
    window: Cell<Option<Window>>,
}

impl HeaderPanel {
    pub fn new(session: Rc<RefCell<Session<'static>>>) -> HeaderPanel {
        HeaderPanel {
            session,
            window: None.into(),
        }
    }

    // TODO Remove
    fn change_text(&self, text: &str) {
        self.window
            .get()
            .expect("header panel doesn't have a window")
            .find_control(ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
            .expect("send-feedback-only-if-armed check box not found")
            .set_text(text);
    }
}

impl HeaderPanel {
    // TODO Remove
    fn setup_change_listener(self: Rc<Self>) {
        Reaper::get().show_console_msg(c_str!("Opened header ui\n"));
        let weak_self = Rc::downgrade(&self);
        self.session
            .borrow_mut()
            .get_dummy_source_model()
            .changed()
            .subscribe(move |_| {
                println!("Dummy source model changed");
                weak_self
                    .upgrade()
                    .expect("header panel not existing anymore")
                    .change_text("test");
            });
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        ID_MAPPINGS_DIALOG
    }

    fn window(&self) -> &Cell<Option<Window>> {
        &self.window
    }

    fn opened(self: Rc<Self>, window: Window) {
        self.setup_change_listener();
    }

    fn button_clicked(self: Rc<Self>, _resource_id: u32) {
        Reaper::get().show_console_msg(c_str!("Clicked button\n"));
        self.session
            .borrow_mut()
            .get_dummy_source_model()
            .channel
            .set(Some(Channel::new(14)));
    }
}
