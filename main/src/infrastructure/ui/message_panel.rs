use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::fonts;
use derivative::Derivative;
use reaper_low::raw;
use std::cell::RefCell;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug, Default)]
pub struct MessagePanel {
    view: ViewContext,
    content: RefCell<MessagePanelContent>,
}

#[derive(Derivative)]
#[derivative(Debug)]
struct MessagePanelContent {
    title: String,
    message: String,
    #[derivative(Debug = "ignore")]
    on_close: Box<dyn FnOnce()>,
}

impl Default for MessagePanelContent {
    fn default() -> Self {
        Self {
            title: "".to_string(),
            message: "".to_string(),
            on_close: Box::new(|| ()),
        }
    }
}

impl MessagePanel {
    pub fn set_content(&self, title: String, message: String, on_close: impl FnOnce() + 'static) {
        let prev_content = self.content.replace(MessagePanelContent {
            title,
            message,
            on_close: Box::new(on_close),
        });
        (prev_content.on_close)();
        if self.is_open() {
            self.invalidate();
        }
    }

    fn invalidate(&self) {
        let content = self.content.borrow();
        self.view.require_window().set_text(content.title.as_str());
        self.view
            .require_control(root::ID_MESSAGE_TEXT)
            .set_text(content.message.as_str());
    }

    fn on_close(&self) {
        let prev_content = self.content.replace(Default::default());
        (prev_content.on_close)();
    }
}

impl View for MessagePanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MESSAGE_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        self.view
            .require_control(root::ID_MESSAGE_TEXT)
            .set_cached_font(fonts::normal_font(window, 20));
        self.invalidate();
        true
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        self.on_close();
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }
}
