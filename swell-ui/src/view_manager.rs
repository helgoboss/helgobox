//! This file is supposed to encapsulate most of the (ugly) win32 API glue code
use crate::{Pixels, Point, SharedView, View, WeakView, Window};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use reaper_low::{raw, Swell};
use rxrust::prelude::*;
use std::os::raw::c_void;
use std::panic::catch_unwind;
use std::ptr::{null_mut, NonNull};

use std::sync::Once;

/// Creates a window according to the given dialog resource.
///
/// It's added as a child to the given parent window and attached to the specified view.
///
/// Internally, this creates a new win32 dialog using the given resource ID. Uses the methods in the
/// given view for all callbacks.
pub(crate) fn create_window(
    view: SharedView<dyn View>,
    resource_id: u32,
    parent_window: Option<Window>,
) {
    let swell = Swell::get();
    unsafe {
        // This will call the dialog procedure `view_dialog_proc`. In order to still know which
        // of the many view objects we are dealing with, we make use of the lparam parameter of
        // `CreateDialogParamA` by passing it an address which points to the concrete view.
        // `view_dialog_proc` with message WM_INITDIALOG will be called immediately, not async.
        // That's important because we must be sure that the given view Rc reference is still
        // valid when it arrives in `view_dialog_proc`.
        swell.CreateDialogParam(
            swell.plugin_context().h_instance(),
            resource_id as u16 as raw::ULONG_PTR as raw::LPSTR,
            parent_window.map(|w| w.raw()).unwrap_or(null_mut()),
            Some(view_dialog_proc),
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

/// This is our dialog procedure.
///
/// It's called by Windows (or the emulation layer). It basically finds the particular `View`
/// instance which matches the HWND and then delegates to its methods. Please note that this is
/// a DialogProc, not a WindowProc. The difference is mainly the return value. A WindowProc
/// usually returns 0 if the message has been processed, or it delegates to `DefWindowProc()`.
/// If we do the latter in a DialogProc, non-child windows start to become always modal (not
/// returning focus) because it's wrong!
///
/// In DialogProc it's the opposite: It returns 1 if the message has been processed and 0 if not.
/// If we have a message where the return value has a special meaning (beyond processed or
/// unprocesssed), we need to "return" that via `SetWindowLong` instead, except for WM_INITDIALOG.
/// See https://docs.microsoft.com/en-us/windows/win32/api/winuser/nc-winuser-dlgproc.
unsafe extern "C" fn view_dialog_proc(
    hwnd: raw::HWND,
    msg: raw::UINT,
    wparam: raw::WPARAM,
    lparam: raw::LPARAM,
) -> raw::INT_PTR {
    catch_unwind(|| {
        DIALOG_PROC_ALREADY_ENTERED.with(|entered| {
            // Detect reentrancy
            let already_entered = entered.replace(true);
            scopeguard::defer! {
                if !already_entered {
                    entered.set(false);
                }
            }
            // Obtain view
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
                        // View is not (yet) registered. Do default stuff.
                        return 0;
                    }
                    Some(v) => {
                        // View is registered. See if it's still existing. If not, the primary owner
                        // (most likely a parent view) dropped it already. In that case we panic
                        // with an appropriate error description. Because requesting a view which
                        // was dropped already means we have some bug in
                        // programming - a logical issue related to
                        // lifetimes. It's important that we neither hide
                        // the issue nor cause a segmentation fault. That's
                        // the whole point of keeping weak pointers:
                        // To be able to fail as gracefully as we can do in such a
                        // situation (= panicking instead of crashing) while still
                        // notifying the user (or ideally developer) that there's an issue.
                        v.upgrade()
                            .ok_or(
                                "Requested ui is registered in ui map but has been dropped already",
                            )
                            .unwrap()
                    }
                }
            };
            // Found view.
            // Delegate to view struct methods.
            let window = Window::new(hwnd).expect("window was null");
            if let Some(result) = view.process_raw(window, msg, wparam, lparam) {
                return result;
            }
            match msg {
                raw::WM_INITDIALOG => {
                    view.view_context().window.replace(Some(window));
                    window.show();
                    let keyboard_focus_desired = view.opened(window);
                    // WM_INITDIALOG is special in a DialogProc in that we don't need to use
                    // `SetWindowLong()` for return values with special meaning.
                    keyboard_focus_desired.into()
                }
                raw::WM_DESTROY => {
                    let view_context = view.view_context();
                    view_context.closed_subject.borrow_mut().next(());
                    view_context.window.replace(None);
                    view.closed(window);
                    ViewManager::get().borrow_mut().unregister_view(hwnd);
                    1
                }
                raw::WM_COMMAND => {
                    let resource_id = loword(wparam);
                    match hiword(wparam) as u32 {
                        0 => {
                            view.button_clicked(resource_id as _);
                            // We just say the click is handled. Don't know where this  would not
                            // be the case.
                            1
                        }
                        raw::CBN_SELCHANGE => {
                            view.option_selected(resource_id as _);
                            // We just say the selection is handled. Don't know where this would not
                            // be the case.
                            1
                        }
                        raw::EN_KILLFOCUS => {
                            view.edit_control_focus_killed(resource_id as _).into()
                        }
                        raw::EN_CHANGE => {
                            // Edit control change event is fired even if we change an edit control
                            // text programmatically. We don't want this. In general.
                            if already_entered {
                                return 0;
                            }
                            view.edit_control_changed(resource_id as _).into()
                        }
                        _ => 0,
                    }
                }
                raw::WM_VSCROLL => {
                    let code = loword(wparam);
                    view.scrolled_vertically(code as _).into()
                }
                raw::WM_HSCROLL => {
                    if lparam <= 0 {
                        // This is not a slider. Not interested.
                        return 0;
                    }
                    let raw_slider = NonNull::new_unchecked(lparam as raw::HWND);
                    view.slider_moved(Window::from_non_null(raw_slider));
                    1
                }
                raw::WM_MOUSEWHEEL => {
                    let distance = hiword_signed(wparam);
                    view.mouse_wheel_turned(distance as _).into()
                }
                raw::WM_CLOSE => {
                    let processed = view.close_requested();
                    if !processed {
                        window.destroy();
                    }
                    1
                }
                raw::WM_CONTEXTMENU => {
                    let x = loword(lparam as _);
                    let y = hiword(lparam as _);
                    view.context_menu_wanted(Point::new(Pixels(x as _), Pixels(y as _)));
                    1
                }
                raw::WM_PAINT => {
                    if view.paint() {
                        1
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        })
    })
    .unwrap_or(0)
}

fn loword(wparam: usize) -> u16 {
    (wparam & 0xffff) as _
}

fn hiword(wparam: usize) -> u16 {
    ((wparam >> 16) & 0xffff) as _
}

fn hiword_signed(wparam: usize) -> i16 {
    hiword(wparam) as _
}

// Used for global dialog proc reentrancy check.
thread_local!(static DIALOG_PROC_ALREADY_ENTERED: Cell<bool> = Cell::new(false));
