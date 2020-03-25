use std::io::Error;
use std::os::raw::c_void;

use vst::editor::Editor;
use winapi::_core::mem::zeroed;
use winapi::_core::ptr::null_mut;
use winapi::shared::minwindef::HINSTANCE;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::wingdi::TextOutA;
use winapi::um::winuser::MAKEINTRESOURCEA;
use winapi::um::winuser::{
    BeginPaint, CreateDialogParamA, DefWindowProcW, PostQuitMessage, SW_SHOWDEFAULT, WM_DESTROY,
    WM_INITDIALOG, WM_PAINT,
};

use crate::bindings::root::{ID_MAIN_DIALOG, ID_MAPPINGS_DIALOG};

// See https://doc.rust-lang.org/std/sync/struct.Once.html why this is safe in combination with Once
pub(crate) static mut GLOBAL_HINSTANCE: HINSTANCE = null_mut();

pub(crate) fn get_global_hinstance() -> HINSTANCE {
    unsafe { GLOBAL_HINSTANCE }
}

pub struct RealearnEditor {
    open: bool,
}

impl RealearnEditor {
    pub fn new() -> RealearnEditor {
        RealearnEditor { open: false }
    }
}

impl Editor for RealearnEditor {
    fn size(&self) -> (i32, i32) {
        (800, 600)
    }

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn close(&mut self) {
        self.open = false;
    }

    fn open(&mut self, parent: *mut c_void) -> bool {
        // print_message("Moin");
        show_window(parent);
        self.open = true;
        true
    }

    fn is_open(&mut self) -> bool {
        self.open
    }
}

static SZ_TEXT: &'static [u8] = b"Hello, world!";

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_INITDIALOG => {
            CreateDialogParamA(
                get_global_hinstance(),
                MAKEINTRESOURCEA(ID_MAPPINGS_DIALOG as u16),
                hwnd,
                None,
                0,
            );
            1
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
fn show_window(parent: *mut c_void) {
    unsafe {
        CreateDialogParamA(
            get_global_hinstance(),
            MAKEINTRESOURCEA(ID_MAIN_DIALOG as u16),
            parent as HWND,
            Some(wnd_proc),
            0, // TODO self pointer
        );
    }
}

#[cfg(windows)]
fn print_message(msg: &str) -> Result<i32, Error> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::winuser::{MessageBoxW, MB_OK};
    let wide: Vec<u16> = OsStr::new(msg).encode_wide().chain(once(0)).collect();
    let ret = unsafe { MessageBoxW(null_mut(), wide.as_ptr(), wide.as_ptr(), MB_OK) };
    if ret == 0 {
        Err(Error::last_os_error())
    } else {
        Ok(ret)
    }
}
