//! This file is supposed to encapsulate most of the (ugly) win32 API glue code
use super::{Window, WindowListener};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

use reaper_high::Reaper;
use reaper_low::{raw, Swell};
use std::os::raw::c_void;
use std::panic::catch_unwind;
use std::rc::{Rc, Weak};
use std::sync::Once;

/// Creates a window according to the given dialog resource.
///
/// It's added as a child to the given parent window and attached to the specified listener.
///
/// Internally, this creates a new win32 dialog using the given resource ID. Uses the methods in the
/// given listener for all callbacks.
pub fn create_window(listener: Rc<dyn WindowListener>, resource_id: u32, parent_window: Window) {
    let swell = Swell::get();
    unsafe {
        // This will call the window procedure `listener_window_proc`. In order to still know which
        // of the many listener objects we are dealing with, we make use of the lparam parameter of
        // `CreateDialogParamA` by passing it an address which points to the concrete listener.
        // `listener_window_proc` with message WM_INITDIALOG will be called immediately, not async.
        // That's important because we must be sure that the given listener Rc reference is still
        // valid when it arrives in `listener_window_proc`.
        swell.CreateDialogParam(
            swell.plugin_context().h_instance(),
            resource_id as u16 as raw::ULONG_PTR as raw::LPSTR,
            parent_window.raw(),
            Some(listener_window_proc),
            convert_listener_ref_to_address(&listener),
        );
    }
}

/// This struct manages the mapping from windows to listeners.
///
/// This is necessary to get from "global" win32 world into beloved "local" Rust struct world.
#[derive(Default, Debug)]
struct WindowListenerManager {
    /// Holds a mapping from window handles (HWND) to listeners
    listener_map: HashMap<raw::HWND, Weak<dyn WindowListener>>,
}

impl WindowListenerManager {
    /// Returns the global window manager instance
    fn get() -> &'static RefCell<WindowListenerManager> {
        static mut WINDOW_LISTENER_MANAGER: Option<RefCell<WindowListenerManager>> = None;
        static INIT_WINDOW_LISTENER_MANAGER: Once = Once::new();
        // We need to initialize the manager lazily because it's impossible to do that using a const
        // function (at least in Rust stable).
        INIT_WINDOW_LISTENER_MANAGER.call_once(|| {
            unsafe {
                WINDOW_LISTENER_MANAGER = Some(RefCell::new(WindowListenerManager::default()))
            };
        });
        unsafe { WINDOW_LISTENER_MANAGER.as_mut().unwrap() }
    }

    /// Registers a new HWND-to-listener mapping
    fn register_listener(&mut self, hwnd: raw::HWND, listener: &Rc<dyn WindowListener>) {
        self.listener_map.insert(hwnd, Rc::downgrade(listener));
    }

    /// Looks up a listener by its corresponding HWND
    fn lookup_listener(&self, hwnd: raw::HWND) -> Option<&Weak<dyn WindowListener>> {
        self.listener_map.get(&hwnd)
    }

    /// Unregisters a HWND-to-Listener mapping
    fn unregister_listener(&mut self, hwnd: raw::HWND) {
        self.listener_map.remove(&hwnd);
    }
}

// Converts the given listener Rc reference to an address which can be transmitted as LPARAM.
// `Rc<dyn Listener>` is a so-called trait object, a *fat* pointer which is twice as large as a
// normal pointer (on 64-bit architectures 2 x 64 bit = 128 bit = 16 bytes). This is too big to
// encode within LPARAM. `&Rc<dyn Listener>` is *not* a trait object but a reference to the trait
// object. Therefore it is a thin pointer already.
fn convert_listener_ref_to_address(listener_trait_object_ref: &Rc<dyn WindowListener>) -> isize {
    let listener_trait_object_ptr = listener_trait_object_ref as *const _ as *const c_void;
    listener_trait_object_ptr as isize
}

// Converts the given address back to the original listener Rc reference.
fn interpret_address_as_listener_ref<'a>(
    listener_trait_object_address: isize,
) -> &'a Rc<dyn WindowListener> {
    let listener_trait_object_ptr = listener_trait_object_address as *const c_void;
    unsafe { &*(listener_trait_object_ptr as *const _) }
}

/// This is our window procedure. It's called by Windows (or the emulation layer). It basically
/// finds the particular `WindowListener` instance which matches the HWND and then delegates to its
/// methods.
unsafe extern "C" fn listener_window_proc(
    hwnd: raw::HWND,
    msg: raw::UINT,
    wparam: raw::WPARAM,
    lparam: raw::LPARAM,
) -> raw::LRESULT {
    catch_unwind(|| {
        let swell = Swell::get();
        let listener: Rc<dyn WindowListener> = if msg == raw::WM_INITDIALOG {
            // A listener window is initializing. At this point lparam contains the value which we
            // passed when calling CreateDialogParam. This contains the address of a
            // listener reference. At subsequent calls, this address is not passed anymore
            // but only the HWND. So we need to save a HWND-to-listener mapping now.
            let listener_ref = interpret_address_as_listener_ref(lparam as _);
            WindowListenerManager::get()
                .borrow_mut()
                .register_listener(hwnd, listener_ref);
            listener_ref.clone()
        } else {
            // Try to find listener corresponding to given HWND
            match WindowListenerManager::get().borrow().lookup_listener(hwnd) {
                None => {
                    // Listener is not (yet) registered. Just use the default handler.
                    return swell.DefWindowProc(hwnd, msg, wparam, lparam);
                }
                Some(v) => {
                    // Listener is registered. See if it's still existing. If not, the primary owner
                    // (most likely a parent listener) dropped it already. In that case we panic
                    // with an appropriate error description. Because requesting
                    // a listener which was dropped already means we have some
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
        // Found listener. Delegate to listener struct methods.
        match msg {
            raw::WM_INITDIALOG => {
                listener.opened(Window::new(hwnd).expect("window was null"));
                // TODO-low Is this really necessary?
                swell.ShowWindow(hwnd, raw::SW_SHOW);
                // TODO-low Let listener customize return value (decides if listener gets keyboard
                // default  focus)  (see https://docs.microsoft.com/en-us/windows/win32/dlgbox/wm-initdialog)
                1
            }
            raw::WM_DESTROY => {
                listener.closed();
                WindowListenerManager::get()
                    .borrow_mut()
                    .unregister_listener(hwnd);
                0
            }
            raw::WM_COMMAND => {
                let resource_id = (wparam & 0xffff) as u32;
                listener.button_clicked(resource_id);
                // TODO-low Return zero if the listener processes this message
                1
            }
            raw::WM_CLOSE => {
                // We never let the user confirm
                swell.DestroyWindow(hwnd);
                0
            }
            _ => swell.DefWindowProc(hwnd, msg, wparam, lparam),
        }
    })
    .unwrap_or_else(|_| Swell::get().DefWindowProc(hwnd, msg, wparam, lparam))
}
