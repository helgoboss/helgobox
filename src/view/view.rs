use winapi::shared::windef::HWND;

/// UI component. Has a 1:1 relationship with a window handle (as in HWND).
pub trait View {
    fn opened(&mut self, data: &OpenedData) {}

    fn closed(&mut self) {}

    fn button_clicked(&mut self, resource_id: u32) {}
}

pub struct OpenedData {
    pub hwnd: HWND,
}
