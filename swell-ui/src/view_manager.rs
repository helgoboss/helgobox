//! This file is supposed to encapsulate most of the (ugly) win32 API glue code
use crate::{
    BrushCache, BrushDescriptor, Color, Pixels, Point, SharedView, View, WeakView, Window,
};
use std::cell::{Cell, RefCell};

use reaper_low::{raw, Swell};
use rxrust::prelude::*;
use std::os::raw::c_void;
use std::panic::catch_unwind;
use std::ptr::null_mut;

use base::hash_util::NonCryptoHashMap;
use fragile::Fragile;
use reaper_medium::{Hbrush, Hdc, Hwnd};
use std::sync::OnceLock;

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
) -> Option<Window> {
    let swell = Swell::get();
    let hwnd = unsafe {
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
        )
    };
    Window::new(hwnd)
}

/// This struct manages the mapping from windows to views.
///
/// This is necessary to get from "global" win32 world into beloved "local" Rust struct world.
#[derive(Default)]
pub struct ViewManager {
    /// Holds a mapping from window handles (HWND) to views
    view_map: RefCell<NonCryptoHashMap<raw::HWND, WeakView<dyn View>>>,
    brush_cache: BrushCache,
}

impl ViewManager {
    pub fn get_solid_brush(&'static self, color: Color) -> Option<Hbrush> {
        self.brush_cache.get_brush(BrushDescriptor::solid(color))
    }

    /// If the given window is one of ours (one that drives our views) and the associated view
    /// still exists, it returns that associated view.
    pub fn get_associated_view(&self, window: Window) -> Option<SharedView<dyn View>> {
        let view_map = self.view_map.borrow();
        let view = view_map.get(&window.raw())?;
        view.upgrade()
    }

    /// Returns the global window manager instance
    pub fn get() -> &'static ViewManager {
        static VIEW_MANAGER: OnceLock<Fragile<ViewManager>> = OnceLock::new();
        // We need to initialize the manager lazily because it's impossible to do that using a const
        // function (at least in Rust stable).
        VIEW_MANAGER
            .get_or_init(|| Fragile::new(ViewManager::default()))
            .get()
    }

    /// Registers a new HWND-to-view mapping
    fn register_view(&self, hwnd: raw::HWND, view: &SharedView<dyn View>) {
        self.view_map
            .borrow_mut()
            .insert(hwnd, SharedView::downgrade(view));
    }

    /// Looks up a view by its corresponding HWND
    fn lookup_view(&self, hwnd: raw::HWND) -> Option<SharedView<dyn View>> {
        let view_map = self.view_map.borrow();
        let weak_view = view_map.get(&hwnd)?;
        weak_view.upgrade().or_else(|| {
            // Not existing. The primary owner (most likely a parent view) dropped
            // it already.
            tracing::warn!("Requested ui is registered in ui map but has been dropped already");
            None
        })
    }

    /// Unregisters a HWND-to-View mapping
    fn unregister_view(&self, hwnd: raw::HWND) {
        self.view_map.borrow_mut().remove(&hwnd);
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
                ViewManager::get().register_view(hwnd, view_ref);
                view_ref.clone()
            } else {
                // Try to find view corresponding to given HWND
                match ViewManager::get().lookup_view(hwnd) {
                    None => {
                        // View is not (yet) registered. Do default stuff.
                        return 0;
                    }
                    Some(v) => v,
                }
            };
            // Found view.
            // Delegate to view struct methods.
            let window = Window::new(hwnd).expect("window was null");
            if let Some(result) = view.process_raw(window, msg, wparam, lparam) {
                return result;
            }
            const KEYBOARD_MSG_FOR_X_BRIDGE: u32 = raw::WM_USER + 100;
            match msg {
                raw::WM_INITDIALOG => {
                    view.view_context().window.replace(Some(window));
                    if view.show_window_on_init() {
                        window.show();
                        let keyboard_focus_desired = view.opened(window);
                        // WM_INITDIALOG is special in a DialogProc in that we don't need to use
                        // `SetWindowLong()` for return values with special meaning.
                        keyboard_focus_desired.into()
                    } else {
                        0
                    }
                }
                raw::WM_SHOWWINDOW => view.shown_or_hidden(wparam == 1).into(),
                raw::WM_DESTROY => {
                    let view_context = view.view_context();
                    view_context.closed_subject.borrow_mut().next(());
                    view_context.window.replace(None);
                    view.on_destroy(window);
                    ViewManager::get().unregister_view(hwnd);
                    1
                }
                // This is called on Linux when receiving a keyboard message via SendMessage.
                // The only time we do this is when we want to forward keyboard interaction from
                // the RealearnAccelerator to egui. egui runs in an XBridge window - a sort
                // of child window of the SWELL window. Returning 0 here makes sure that the
                // messages are passed through to the XBridge window. This is SWELL functionality
                // (check the SWELL code).
                KEYBOARD_MSG_FOR_X_BRIDGE => 0,
                raw::WM_SIZE => view.resized().into(),
                raw::WM_SETFOCUS => view.focused().into(),
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
                        raw::EN_SETFOCUS => view.edit_control_focus_set(resource_id as _).into(),
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
                    let raw_slider = Hwnd::new(lparam as raw::HWND).expect("slider hwnd is null");
                    view.slider_moved(Window::from_hwnd(raw_slider));
                    1
                }
                raw::WM_MOUSEWHEEL => {
                    let distance = hiword_signed(wparam);
                    view.mouse_wheel_turned(distance as _).into()
                }
                raw::WM_KEYDOWN => view.key_down(wparam as _).into(),
                raw::WM_KEYUP => view.key_up(wparam as _).into(),
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
                raw::WM_PAINT => isize::from(view.paint()),
                raw::WM_ERASEBKGND => isize::from(view.erase_background(
                    Hdc::new(wparam as raw::HDC).expect("HDC in WM_ERASEBKGND is null"),
                )),
                raw::WM_CTLCOLORSTATIC => {
                    let brush = view.control_color_static(
                        Hdc::new(wparam as raw::HDC).expect("HDC in WM_CTLCOLORSTATIC is null"),
                        Window::new(lparam as raw::HWND)
                            .expect("WM_CTLCOLORSTATIC control is null"),
                    );
                    brush.map(|b| b.as_ptr()).unwrap_or(null_mut()) as _
                }
                raw::WM_CTLCOLORDLG => {
                    let brush = view.control_color_dialog(
                        Hdc::new(wparam as raw::HDC).expect("HDC in WM_CTLCOLORDLG is null"),
                        Window::new(lparam as raw::HWND).expect("WM_CTLCOLORDLG control is null"),
                    );
                    brush.map(|b| b.as_ptr()).unwrap_or(null_mut()) as _
                }
                raw::WM_TIMER => {
                    if view.timer(wparam) {
                        0
                    } else {
                        1
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
