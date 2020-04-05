use crate::view::Window;
use winapi::_core::mem::zeroed;
use winapi::_core::ptr::null_mut;
use winapi::shared::minwindef::HINSTANCE;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::wingdi::TextOutA;
use winapi::um::winuser::MAKEINTRESOURCEA;
use winapi::um::winuser::{
    BeginPaint, CreateDialogParamA, DefWindowProcW, PostQuitMessage, SW_SHOWDEFAULT, WM_COMMAND,
    WM_DESTROY, WM_INITDIALOG, WM_PAINT,
};

/// UI component. Has a 1:1 relationship with a window handle (as in HWND).
// TODO Rename to ViewListener or WindowHandler or anything in-between
pub trait View {
    fn opened(&mut self, window: Window) {}

    fn closed(&mut self) {}

    fn button_clicked(&mut self, resource_id: u32) {}
}
