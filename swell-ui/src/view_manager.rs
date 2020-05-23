//! This file is supposed to encapsulate most of the (ugly) win32 API glue code
use crate::{SharedView, View, WeakView, Window};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

use reaper_low::{raw, Swell};
use rxrust::prelude::*;
use std::os::raw::c_void;
use std::panic::catch_unwind;
use std::rc::{Rc, Weak};
use std::sync::Once;

/// Creates a window according to the given dialog resource.
///
/// It's added as a child to the given parent window and attached to the specified view.
///
/// Internally, this creates a new win32 dialog using the given resource ID. Uses the methods in the
/// given view for all callbacks.
pub(crate) fn create_window(view: SharedView<dyn View>, resource_id: u32, parent_window: Window) {
    let swell = Swell::get();
    unsafe {
        // This will call the window procedure `view_window_proc`. In order to still know which
        // of the many view objects we are dealing with, we make use of the lparam parameter of
        // `CreateDialogParamA` by passing it an address which points to the concrete view.
        // `view_window_proc` with message WM_INITDIALOG will be called immediately, not async.
        // That's important because we must be sure that the given view Rc reference is still
        // valid when it arrives in `view_window_proc`.
        swell.CreateDialogParam(
            swell.plugin_context().h_instance(),
            resource_id as u16 as raw::ULONG_PTR as raw::LPSTR,
            parent_window.raw(),
            Some(view_window_proc),
            convert_view_ref_to_address(&view),
        );
    }
}

/// This struct manages the mapping from windows to views.
///
/// This is necessary to get from "global" win32 world into beloved "local" Rust struct world.
#[derive(Default)]
struct ViewManager {
    /// Holds a mapping from window handles (HWND) to views
    view_map: HashMap<raw::HWND, WeakView<dyn View>>,
}

impl ViewManager {
    /// Returns the global window manager instance
    fn get() -> &'static RefCell<ViewManager> {
        static mut VIEW_MANAGER: Option<RefCell<ViewManager>> = None;
        static INIT_VIEW_MANAGER: Once = Once::new();
        // We need to initialize the manager lazily because it's impossible to do that using a const
        // function (at least in Rust stable).
        INIT_VIEW_MANAGER.call_once(|| {
            unsafe { VIEW_MANAGER = Some(RefCell::new(ViewManager::default())) };
        });
        unsafe { VIEW_MANAGER.as_mut().unwrap() }
    }

    /// Registers a new HWND-to-view mapping
    fn register_view(&mut self, hwnd: raw::HWND, view: &SharedView<dyn View>) {
        self.view_map.insert(hwnd, SharedView::downgrade(view));
    }

    /// Looks up a view by its corresponding HWND
    fn lookup_view(&self, hwnd: raw::HWND) -> Option<&WeakView<dyn View>> {
        self.view_map.get(&hwnd)
    }

    /// Unregisters a HWND-to-View mapping
    fn unregister_view(&mut self, hwnd: raw::HWND) {
        self.view_map.remove(&hwnd);
    }
}

// Converts the given view Rc reference to an address which can be transmitted as LPARAM.
// `SharedView<dyn View>` is a so-called trait object, a *fat* pointer which is twice as large as a
// normal pointer (on 64-bit architectures 2 x 64 bit = 128 bit = 16 bytes). This is too big to
// encode within LPARAM. `&SharedView<dyn View>` is *not* a trait object but a reference to the
// trait object. Therefore it is a thin pointer already.
fn convert_view_ref_to_address(view_trait_object_ref: &SharedView<dyn View>) -> isize {
    let view_trait_object_ptr = view_trait_object_ref as *const _ as *const c_void;
    view_trait_object_ptr as isize
}

// Converts the given address back to the original view Rc reference.
fn interpret_address_as_view_ref<'a>(view_trait_object_address: isize) -> &'a SharedView<dyn View> {
    let view_trait_object_ptr = view_trait_object_address as *const c_void;
    unsafe { &*(view_trait_object_ptr as *const _) }
}

/// This is our window procedure. It's called by Windows (or the emulation layer). It basically
/// finds the particular `View` instance which matches the HWND and then delegates to its
/// methods.
unsafe extern "C" fn view_window_proc(
    hwnd: raw::HWND,
    msg: raw::UINT,
    wparam: raw::WPARAM,
    lparam: raw::LPARAM,
) -> raw::LRESULT {
    catch_unwind(|| {
        let swell = Swell::get();
        let view: SharedView<dyn View> = if msg == raw::WM_INITDIALOG {
            // A view window is initializing. At this point lparam contains the value which we
            // passed when calling CreateDialogParam. This contains the address of a
            // view reference. At subsequent calls, this address is not passed anymore
            // but only the HWND. So we need to save a HWND-to-view mapping now.
            let view_ref = interpret_address_as_view_ref(lparam as _);
            ViewManager::get()
                .borrow_mut()
                .register_view(hwnd, view_ref);
            view_ref.clone()
        } else {
            // Try to find view corresponding to given HWND
            match ViewManager::get().borrow().lookup_view(hwnd) {
                None => {
                    // View is not (yet) registered. Just use the default handler.
                    return swell.DefWindowProc(hwnd, msg, wparam, lparam);
                }
                Some(v) => {
                    // View is registered. See if it's still existing. If not, the primary owner
                    // (most likely a parent view) dropped it already. In that case we panic
                    // with an appropriate error description. Because requesting
                    // a view which was dropped already means we have some
                    // bug in programming - an logical issue related to
                    // lifetimes. It's important that we neither hide the issue nor cause a
                    // segmentation fault. That's the whole point of keeping
                    // weak pointers: To be able to fail as gracefully as we can
                    // do in such a situation (= panicking instead of crashing)
                    // while still notifying the user (or ideally developer) that there's an issue.
                    v.upgrade()
                        .ok_or("Requested ui is registered in ui map but has been dropped already")
                        .unwrap()
                }
            }
        };
        // Found view. Delegate to view struct methods.
        let window = Window::new(hwnd).expect("window was null");
        match msg {
            raw::WM_INITDIALOG => {
                view.view_context().window.replace(Some(window));
                window.show();
                let keyboard_focus_desired = view.opened(window);
                keyboard_focus_desired.into()
            }
            raw::WM_DESTROY => {
                let view_context = view.view_context();
                view_context.closed_subject.borrow_mut().next(());
                view_context.window.replace(None);
                view.closed();
                ViewManager::get().borrow_mut().unregister_view(hwnd);
                0
            }
            raw::WM_COMMAND => {
                let resource_id = (wparam & 0xffff) as u32;
                let hiword = ((wparam >> 16) & 0xffff) as u32;
                match hiword {
                    0 => {
                        view.button_clicked(resource_id);
                        // TODO For now we just say the click is handled. Don't know where this
                        // would not  be the case.
                        1
                    }
                    raw::CBN_SELCHANGE => {
                        view.option_selected(resource_id);
                        1
                    }
                    _ => 0,
                }
            }
            raw::WM_CLOSE => {
                // We never let the user confirm
                window.destroy();
                0
            }
            _ => swell.DefWindowProc(hwnd, msg, wparam, lparam),
        }
    })
    .unwrap_or_else(|_| Swell::get().DefWindowProc(hwnd, msg, wparam, lparam))
}
