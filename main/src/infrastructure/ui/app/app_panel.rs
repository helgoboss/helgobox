use crate::application::WeakSession;
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::AppHandle;
use anyhow::{anyhow, Context, Result};
use base::Global;
use c_str_macro::c_str;
use playtime_clip_engine::proto;
use playtime_clip_engine::proto::{
    event_reply, occasional_matrix_update, reply, ClipEngineReceivers, Empty, EventReply,
    GetOccasionalMatrixUpdatesReply, OccasionalMatrixUpdate, Reply,
};
use prost::Message;
use reaper_low::{raw, Swell};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{null_mut, NonNull};
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};
use validator::HasLen;

#[derive(Debug)]
pub struct AppPanel {
    view: ViewContext,
    session: WeakSession,
    open_state: RefCell<Option<OpenState>>,
}

#[derive(Debug)]
struct OpenState {
    app_handle: AppHandle,
    app_callback: Option<AppCallback>,
    // TODO-medium This is too specific.
    event_receivers: Option<ClipEngineReceivers>,
}

impl AppPanel {
    pub fn new(session: WeakSession) -> Self {
        Self {
            view: Default::default(),
            session,
            open_state: RefCell::new(None),
        }
    }

    pub fn send_to_app(&self, reply: &Reply) -> Result<()> {
        self.open_state
            .borrow()
            .as_ref()
            .context("app not open")?
            .send_to_app(reply)
    }

    pub fn toggle_full_screen(&self) -> Result<()> {
        // Because the full-screen windowing code is a mess and highly platform-specific, it's best
        // to use a platform-specific language to do the job. In case of macOS, Swift is the best
        // choice. The app itself has easy access to Swift, so let's just call into the app library
        // so it takes care of handling its host window.
        // TODO-low It's a bit weird to ask the app (a guest) to deal with a host window. Improve.
        App::get_or_load_app_library()?.toggle_full_screen(self.view.require_window())
    }

    pub fn notify_app_is_ready(&self, callback: AppCallback) {
        let mut open_state = self.open_state.borrow_mut();
        let Some(open_state) = open_state.as_mut() else {
            return;
        };
        // Handshake finished! The app has the host callback and we have the app callback.
        open_state.app_callback = Some(callback);
        // Now we can start passing events to the app callback
        self.start_timer();
    }

    fn start_timer(&self) {
        self.view
            .require_window()
            .set_timer(TIMER_ID, Duration::from_millis(30));
    }

    fn stop_timer(&self) {
        self.view.require_window().kill_timer(TIMER_ID);
    }

    fn open_internal(&self, window: Window) -> Result<()> {
        let app_library = App::get_or_load_app_library()?;
        let session_id = self
            .session
            .upgrade()
            .ok_or_else(|| anyhow!("session gone"))?
            .borrow()
            .id()
            .to_string();
        let app_handle = app_library.run_in_parent(window, session_id)?;
        let open_state = OpenState {
            app_handle,
            app_callback: None,
            event_receivers: Some(subscribe_to_events()),
        };
        *self.open_state.borrow_mut() = Some(open_state);
        Ok(())
    }

    fn close_internal(&self, window: Window) -> Result<()> {
        let open_state = self
            .open_state
            .borrow_mut()
            .take()
            .ok_or(anyhow!("app was already closed"))?;
        open_state.close_app(window)
    }
}

impl OpenState {
    pub fn send_to_app(&self, reply: &Reply) -> Result<()> {
        let app_callback = self.app_callback.context("app callback not known yet")?;
        send_to_app(app_callback, reply);
        Ok(())
    }

    pub fn send_pending_events(&mut self, session_id: &str) {
        let (Some(app_callback), Some(event_receivers)) = (self.app_callback, &mut self.event_receivers) else {
            return;
        };
        event_receivers.process_pending_updates(session_id, &|event_reply| {
            let reply = Reply {
                value: Some(reply::Value::EventReply(event_reply)),
            };
            let _ = send_to_app(app_callback, &reply);
        });
    }

    pub fn close_app(&self, window: Window) -> Result<()> {
        App::get_or_load_app_library()?.close(window, self.app_handle)
    }
}

impl View for AppPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_EMPTY_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        self.open_internal(window).is_ok()
    }

    fn close_requested(self: SharedView<Self>) -> bool {
        // Don't really close window (along with the app instance). Just hide it. It's a bit faster
        // when next opening the window.
        self.view.require_window().hide();
        true
    }

    fn closed(self: SharedView<Self>, window: Window) {
        self.close_internal(window).unwrap();
    }

    fn shown_or_hidden(self: SharedView<Self>, shown: bool) -> bool {
        if shown {
            // Send events to app again.
            if let Some(open_state) = self.open_state.borrow_mut().as_mut() {
                open_state.event_receivers = Some(subscribe_to_events());
            } else {
                // We also get called when the window is first opened, *before* `opened` is called!
                // In that case, `open_state` is not set yet. That's how we know it's the first opening,
                // not a subsequent show. We don't need to do anything in that case.
                return false;
            }
            // Start processing events again when shown
            self.start_timer();
            // Send a reset event to the app. That's not necessary when the app is first
            // shown because it resubscribes to everything on start anyway. But it's important for
            // subsequent shows because the app was not aware that it was not fed with events while
            // hidden.
            let _ = self.send_to_app(&Reply {
                value: Some(reply::Value::EventReply(EventReply {
                    value: Some(event_reply::Value::Reset(Empty {})),
                })),
            });
        } else {
            // Don't process events while hidden
            if let Some(open_state) = self.open_state.borrow_mut().as_mut() {
                open_state.event_receivers = None;
            }
            self.stop_timer();
        }
        true
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }

    /// On Windows, this is necessary to resize contained app.
    ///
    /// On macOS, this has no effect because the app window is not a child view (NSView) but a
    /// child window (NSWindow). Resizing according to the parent window (the SWELL window) is done
    /// on app side.
    #[cfg(target_os = "windows")]
    fn resized(self: SharedView<Self>) -> bool {
        crate::infrastructure::ui::egui_views::on_parent_window_resize(self.view.require_window())
    }

    fn timer(&self, id: usize) -> bool {
        if id != TIMER_ID {
            return false;
        }
        let mut open_state = self.open_state.borrow_mut();
        let Some(open_state) = open_state.as_mut() else {
            return false;
        };
        let Some(session) = self.session.upgrade() else {
            return false;
        };
        open_state.send_pending_events(session.borrow().id());
        true
    }

    /// On Windows, this is necessary to make keyboard input work for the contained app. We
    /// basically forward all keyboard messages (which come from the RealearnAccelerator) to the
    /// first child of the first child, which is the Flutter window.
    #[cfg(target_os = "windows")]
    fn get_keyboard_event_receiver(&self, _focused_window: Window) -> Option<Window> {
        self.view.window()?.first_child()?.first_child()
    }

    /// On macOS, the app window is a child *window* of this window, not a child *view*. In general,
    /// keyboard input is made possible there by allowing the child window to become a key window
    /// (= get real focus). This is done on app side. However, one corner case is that the user
    /// clicks the title bar of this window (= the parent window). In this case, the parent window
    /// becomes the key window and we need to forward keyboard events to the child window.
    #[cfg(target_os = "macos")]
    fn get_keyboard_event_receiver(&self, _focused_window: Window) -> Option<Window> {
        self.view.window()?.first_child_window()
    }
}

const TIMER_ID: usize = 322;

fn send_to_app(app_callback: AppCallback, reply: &Reply) {
    let vec = reply.encode_to_vec();
    let length = vec.length();
    let boxed_slice = vec.into_boxed_slice();
    // The app side is responsible for freeing the memory!
    // We really need to pass owned data to the app because it's written in Dart and Dart code
    // doesn't execute on the same thread. It will execute the code asynchronously in another
    // thread and at that point the data still needs to be valid.
    let raw_ptr = Box::into_raw(boxed_slice);
    unsafe {
        app_callback(raw_ptr as *const _, length as _);
    }
}

/// Signature of the function that's used from the host in order to call the external app.
pub type AppCallback = unsafe extern "C" fn(data: *const u8, length: i32);

fn subscribe_to_events() -> ClipEngineReceivers {
    App::get().clip_engine_hub().senders().subscribe_to_all()
}
