use crate::view::bindings::root::ID_MAPPINGS_DIALOG;
use crate::view::views::HeaderView;
use crate::view::{open_view, OpenedData, View};
use c_str_macro::c_str;
use reaper_rs::high_level::Reaper;

pub struct MainView {
    header_view: HeaderView,
}

impl MainView {
    pub fn new() -> MainView {
        MainView {
            header_view: HeaderView::new(),
        }
    }
}

impl View for MainView {
    fn opened(&mut self, data: &OpenedData) {
        Reaper::get().show_console_msg(c_str!("Opened main view"));
        open_view(&mut self.header_view, ID_MAPPINGS_DIALOG, data.hwnd)
    }
}
