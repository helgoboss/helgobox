use crate::view::bindings::root::ID_MAPPINGS_DIALOG;
use crate::view::views::HeaderView;
use crate::view::{open_view, OpenedData, View};
use c_str_macro::c_str;
use reaper_rs::high_level::Reaper;

pub struct MainView {
    header_view: Box<Box<dyn View>>,
}

impl MainView {
    pub fn new() -> MainView {
        MainView {
            header_view: Box::new(Box::new(HeaderView::new())),
        }
    }
}

impl View for MainView {
    fn opened(&mut self, data: &OpenedData) {
        Reaper::get().show_console_msg(c_str!("Opened main view"));
        open_view(self.header_view.as_mut(), ID_MAPPINGS_DIALOG, data.hwnd)
    }
}
