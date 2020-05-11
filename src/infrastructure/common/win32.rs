#[cfg(target_os = "windows")]
pub use winapi::{
    shared::{
        minwindef::{HINSTANCE, LPARAM, LRESULT, UINT, WPARAM},
        windef::HWND,
    },
    um::winuser::{
        CreateDialogParamA as CreateDialogParam, DefWindowProcW as DefWindowProc, DestroyWindow,
        GetDlgItem, SetWindowTextW as SetWindowText, WM_CLOSE, WM_COMMAND, WM_DESTROY,
        WM_INITDIALOG,
    },
    um::winuser::{ShowWindow, MAKEINTRESOURCEA as MAKEINTRESOURCE, SW_SHOW},
};

#[cfg(target_os = "linux")]
pub use crate::infrastructure::common::bindings::root::{
    DefWindowProc, DestroyWindow, GetDlgItem, ShowWindow, HINSTANCE, HWND, LPARAM, LRESULT,
    SW_SHOW, UINT, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_INITDIALOG, WPARAM,
};

// CreateDialogParam
// #define CreateDialogParam(hinst,resid,par,dlgproc,param)
// SWELL_CreateDialog(SWELL_curmodule_dialogresource_head,(resid),par,dlgproc,param)

// DefWindowProc

// SetWindowText
// #define SetWindowText(hwnd,text) SetDlgItemText(hwnd,0,text)

// MAKEINTRESOURCE
// #define MAKEINTRESOURCE(x) ((const char *)(UINT_PTR)(x))
