use crate::model::RealearnSession;
use crate::view::bindings::root::ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX;
use crate::view::{View, Window};
use c_str_macro::c_str;
use helgoboss_midi::channel;
use reaper_rs::high_level::Reaper;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct HeaderView {
    session: Rc<RefCell<RealearnSession<'static>>>,
    window: RefCell<Option<Window>>,
}

impl HeaderView {
    pub fn new(session: Rc<RefCell<RealearnSession<'static>>>) -> HeaderView {
        HeaderView {
            session,
            window: RefCell::new(None),
        }
    }

    fn change_text(&self, text: &str) {
        self.window
            .borrow()
            .unwrap()
            .find_control(ID_SEND_FEEDBACK_ONLY_IF_ARMED_CHECK_BOX)
            .unwrap()
            .set_text(text);
    }
}

impl View for HeaderView {
    fn opened(self: Rc<Self>, window: Window) {
        *self.window.borrow_mut() = Some(window);
        Reaper::get().show_console_msg(c_str!("Opened header view\n"));
        let weak_self = Rc::downgrade(&self);
        self.session
            .borrow_mut()
            .get_dummy_source_model()
            .changed()
            .subscribe(move |_| {
                println!("Dummy source model changed");
                weak_self.upgrade().unwrap().change_text("test");
            });
    }

    fn closed(self: Rc<Self>) {
        *self.window.borrow_mut() = None;
    }

    fn button_clicked(self: Rc<Self>, resource_id: u32) {
        Reaper::get().show_console_msg(c_str!("Clicked button\n"));
        self.session
            .borrow_mut()
            .get_dummy_source_model()
            .channel
            .set(Some(channel(14)));
    }
}
