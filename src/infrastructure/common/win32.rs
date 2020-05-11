#[cfg(target_os = "windows")]
mod windows {
    pub use winapi::{
        shared::{
            minwindef::{HINSTANCE, LPARAM, LRESULT, UINT, WPARAM},
            windef::HWND,
        },
        um::winuser::{
            CreateDialogParamA as CreateDialogParam, DefWindowProcA as DefWindowProc,
            DestroyWindow, GetDlgItem, SetWindowTextA as SetWindowText, WM_CLOSE, WM_COMMAND,
            WM_DESTROY, WM_INITDIALOG,
        },
        um::winuser::{ShowWindow, MAKEINTRESOURCEA as MAKEINTRESOURCE, SW_SHOW},
    };
}

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "linux")]
mod linux {
    pub use crate::infrastructure::common::bindings::root::{
        DefWindowProc, DestroyWindow, GetDlgItem, ShowWindow, HINSTANCE, HWND, LPARAM, LRESULT,
        SW_SHOW, UINT, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_INITDIALOG, WPARAM,
    };

    use crate::infrastructure::common::bindings::root;

    pub unsafe fn CreateDialogParam(
        hinst: HINSTANCE,
        resid: *const ::std::os::raw::c_char,
        par: root::HWND,
        dlgproc: root::DLGPROC,
        param: root::LPARAM,
    ) -> root::HWND {
        root::SWELL_CreateDialog(
            root::SWELL_curmodule_dialogresource_head,
            resid,
            par,
            dlgproc,
            param,
        )
    }

    pub unsafe fn SetWindowText(
        hwnd: root::HWND,
        text: *const ::std::os::raw::c_char,
    ) -> root::BOOL {
        root::SetDlgItemText(hwnd, 0, text)
    }

    pub fn MAKEINTRESOURCE(x: root::WORD) -> root::LPSTR {
        x as root::ULONG_PTR as root::LPSTR
    }
}

#[cfg(target_os = "linux")]
pub use linux::*;
