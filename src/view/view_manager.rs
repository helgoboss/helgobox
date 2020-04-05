use crate::view::{OpenedData, View};
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
use winapi::um::winuser::MAKEINTRESOURCEA;
use winapi::um::winuser::{
    BeginPaint, CreateDialogParamA, DefWindowProcW, PostQuitMessage, SW_SHOWDEFAULT, WM_COMMAND,
    WM_DESTROY, WM_INITDIALOG, WM_PAINT,
};

type ViewMap = RefCell<HashMap<HWND, isize>>;

/// Holds a mapping from window handles (HWND) to views (our trait objects of type View).
/// Necessary to get from the win32 world into beloved Rust world.
// See https://doc.rust-lang.org/std/sync/struct.Once.html why this is safe in combination with Once
static mut VIEW_ADDRESS_MAP: Option<ViewMap> = None;
static INIT_VIEW_ADDRESS_MAP: Once = Once::new();

// See https://doc.rust-lang.org/std/sync/struct.Once.html why this is safe in combination with Once
pub static mut GLOBAL_HINSTANCE: HINSTANCE = null_mut();

fn get_global_hinstance() -> HINSTANCE {
    unsafe { GLOBAL_HINSTANCE }
}

pub(super) fn open_view<V: View>(view_ref: &mut V, resource_id: u32, parent_window: HWND) {
    let view_ptr = view_ref as *mut _ as *mut c_void;
    let view_ptr_address = view_ptr as isize;
    unsafe {
        CreateDialogParamA(
            get_global_hinstance(),
            MAKEINTRESOURCEA(resource_id as u16),
            parent_window,
            Some(static_window_proc::<V>),
            view_ptr_address,
        );
    }
}

fn get_view_address_map() -> &'static ViewMap {
    INIT_VIEW_ADDRESS_MAP.call_once(|| {
        unsafe { VIEW_ADDRESS_MAP = Some(RefCell::new(HashMap::new())) };
    });
    unsafe { VIEW_ADDRESS_MAP.as_mut().unwrap() }
}

fn register_view(hwnd: HWND, view_address: isize) {
    get_view_address_map()
        .borrow_mut()
        .insert(hwnd, view_address);
}

fn find_view_address(hwnd: HWND) -> Option<isize> {
    get_view_address_map().borrow().get(&hwnd).map(|v| *v)
}

fn find_view<'a, V: View>(hwnd: HWND) -> Option<&'a mut V> {
    find_view_address(hwnd).map(|view_ptr_address| {
        let view_ptr = view_ptr_address as *mut c_void;
        unsafe { &mut *(view_ptr as *mut _) }
    })
}

/// Called by window system. Finds the view which matches the HWND and delegates.
unsafe extern "system" fn static_window_proc<V: View>(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // TODO Firewall
    if msg == WM_INITDIALOG {
        // A view is initializing. At this point lparam contains the value which we passed when
        // calling CreateDialogParam. This contains an address of the View trait object. At later
        // calls, this address is not passed anymore but only the HWND. So we need to save this
        // mapping now.
        register_view(hwnd, lparam);
    }
    if let Some(view) = find_view::<V>(hwnd) {
        window_proc::<V>(hwnd, msg, wparam, lparam, view)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Called by our code after having found the view which matches the HWND. Immediately delegates
/// to the nice View trait methods.
unsafe fn window_proc<V: View>(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
    view: &mut V,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            // TODO
            PostQuitMessage(0);
            view.closed();
            // TODO
            0
        }
        WM_INITDIALOG => {
            view.opened(&OpenedData { hwnd });
            // TODO
            1
        }
        WM_COMMAND => {
            let resource_id = (wparam & 0xffff) as u32;
            view.button_clicked(resource_id);
            // TODO
            1
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
