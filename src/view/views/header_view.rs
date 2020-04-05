use crate::view::{OpenedData, View};
use c_str_macro::c_str;
use reaper_rs::high_level::Reaper;

pub struct HeaderView {}

impl HeaderView {
    pub fn new() -> HeaderView {
        HeaderView {}
    }
}

impl View for HeaderView {
    fn opened(&mut self, data: &OpenedData) {
        Reaper::get().show_console_msg(c_str!("Opened header view"));
    }

    fn button_clicked(&mut self, resource_id: u32) {
        Reaper::get().show_console_msg(c_str!("Clicked button"));
    }
}
