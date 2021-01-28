use crate::application::{
    LearnManySubState, SharedSession, VirtualControlElementType, WeakSession,
};
use crate::core::when;
use crate::domain::MappingCompartment;
use crate::infrastructure::ui::bindings::root;
use reaper_low::raw;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use swell_ui::{SharedView, View, ViewContext, Window};
use wrap_debug::WrapDebug;

#[derive(Debug)]
pub struct MessagePanel {
    view: ViewContext,
    content: RefCell<MessagePanelContent>,
    on_close: WrapDebug<Box<dyn Fn()>>,
}

#[derive(Debug)]
struct MessagePanelContent {
    title: String,
    message: String,
}

impl MessagePanel {
    pub fn new(title: String, message: String, on_close: impl Fn() + 'static) -> MessagePanel {
        MessagePanel {
            view: Default::default(),
            content: RefCell::new(MessagePanelContent { title, message }),
            on_close: WrapDebug(Box::new(on_close)),
        }
    }

    pub fn set_title_and_message(&self, title: String, message: String) {
        self.content.replace(MessagePanelContent { title, message });
        self.invalidate();
    }

    fn invalidate(&self) {
        let content = self.content.borrow();
        self.view.require_window().set_text(content.title.as_str());
        self.view
            .require_control(root::ID_MESSAGE_TEXT)
            .set_text(content.message.as_str());
    }
}

impl View for MessagePanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MESSAGE_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.invalidate();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        (self.on_close)();
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => (self.on_close)(),
            _ => unreachable!(),
        }
    }
}
