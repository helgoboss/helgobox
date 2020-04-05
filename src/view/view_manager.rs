//! This file is supposed to encapsulate most of the (ugly) win32 API glue code
use crate::view::{View, Window};
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::os::raw::c_void;
use std::sync::Once;
use winapi::_core::mem::zeroed;
use winapi::_core::ptr::null_mut;
use winapi::shared::minwindef::HINSTANCE;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::wingdi::TextOutA;
use winapi::um::winuser::{
    BeginPaint, CreateDialogParamA, DefWindowProcW, DestroyWindow, PostQuitMessage, SW_SHOWDEFAULT,
    WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_INITDIALOG, WM_PAINT,
};
use winapi::um::winuser::{ShowWindow, MAKEINTRESOURCEA, SW_SHOW};

/// When this dynamic library is loaded in Windows, this global variable will be filled with the
/// DLL's HMODULE/HINSTANCE address, which is necessary to access the dialog resources for the Win32
/// UI.
pub static mut GLOBAL_HINSTANCE: HINSTANCE = null_mut();

/// On Windows, this returns the DLL's HMODULE/HINSTANCE address as soon as the DLL is loaded,
/// otherwise null.
fn get_global_hinstance() -> HINSTANCE {
    unsafe { GLOBAL_HINSTANCE }
}

type ViewMap = RefCell<HashMap<HWND, isize>>;
/// Holds a mapping from window handles (HWND) to view addresses. Necessary to get from global win32
/// world into beloved "Rust struct" world.
static mut VIEW_ADDRESS_MAP: Option<ViewMap> = None;
static INIT_VIEW_ADDRESS_MAP: Once = Once::new();

/// Creates a new win32 dialog using the given resource ID. Uses the methods in the given view for
/// all callbacks.
pub(super) fn open_view<V: View>(view_ref: &mut V, resource_id: u32, parent_window: HWND) {
    let view_ptr = view_ref as *mut _ as *mut c_void;
    let view_ptr_address = view_ptr as isize;
    unsafe {
        CreateDialogParamA(
            get_global_hinstance(),
            MAKEINTRESOURCEA(resource_id as u16),
            parent_window,
            Some(view_window_proc::<V>),
            view_ptr_address,
        );
    }
}

/// Returns the global view address map
fn get_view_address_map() -> &'static ViewMap {
    // We need to initialize this map lazily because it's impossible to do that using a const
    // function (at least in Rust stable).
    INIT_VIEW_ADDRESS_MAP.call_once(|| {
        unsafe { VIEW_ADDRESS_MAP = Some(RefCell::new(HashMap::new())) };
    });
    unsafe { VIEW_ADDRESS_MAP.as_mut().unwrap() }
}

/// Registers a new "HWND" to "view address" mapping
fn register_view(hwnd: HWND, view_address: isize) {
    get_view_address_map()
        .borrow_mut()
        .insert(hwnd, view_address);
}

/// Unregisters a "HWND" to "view address" mapping
fn unregister_view(hwnd: HWND) {
    get_view_address_map().borrow_mut().remove(&hwnd);
}

/// Finds a view address given a HWND
fn find_view_address(hwnd: HWND) -> Option<isize> {
    get_view_address_map().borrow().get(&hwnd).map(|v| *v)
}

/// Finds a view reference given a HWND
fn find_view<'a, V: View>(hwnd: HWND) -> Option<&'a mut V> {
    find_view_address(hwnd).map(|view_ptr_address| {
        let view_ptr = view_ptr_address as *mut c_void;
        unsafe { &mut *(view_ptr as *mut _) }
    })
}

/// This is our window procedure. It's called by Windows (or the emulation layer). This function is
/// generic and will be generated for each struct that implements `View`. It basically finds the
/// particular view instance which matches the HWND and then delegates to appropriate `View`
/// methods.
unsafe extern "system" fn view_window_proc<V: View>(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // TODO Firewall
    if msg == WM_INITDIALOG {
        // A view window is initializing. At this point lparam contains the value which we passed
        // when calling CreateDialogParam. This contains an address of the View trait
        // object. At later calls, this address is not passed anymore but only the HWND. So
        // we need to save this mapping now.
        register_view(hwnd, lparam);
    }
    // Try to find view corresponding to given HWND
    let mut view = match find_view::<V>(hwnd) {
        None => {
            // Unknown view. Do default processing.
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        Some(v) => v,
    };
    // Found view. Delegate to view struct methods.
    match msg {
        WM_INITDIALOG => {
            view.opened(Window::new(hwnd));
            // TODO-low Is this really necessary?
            ShowWindow(hwnd, SW_SHOW);
            // TODO-low Let view customize return value (decides if view gets keyboard default
            //  focus)  (see https://docs.microsoft.com/en-us/windows/win32/dlgbox/wm-initdialog)
            1
        }
        WM_DESTROY => {
            view.closed();
            unregister_view(hwnd);
            0
        }
        WM_COMMAND => {
            let resource_id = (wparam & 0xffff) as u32;
            view.button_clicked(resource_id);
            // TODO
            1
        }
        WM_CLOSE => {
            // We never let the user confirm
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
