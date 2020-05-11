#[cfg(target_os = "windows")]
pub use winapi::{
    shared::{
        minwindef::{HINSTANCE, LPARAM, LRESULT, UINT, WPARAM},
        windef::HWND,
    },
    um::winuser::{
        CreateDialogParamA, DefWindowProcW, DestroyWindow, GetDlgItem, SetWindowTextW, WM_CLOSE,
        WM_COMMAND, WM_DESTROY, WM_INITDIALOG,
    },
    um::winuser::{ShowWindow, MAKEINTRESOURCEA, SW_SHOW},
};
