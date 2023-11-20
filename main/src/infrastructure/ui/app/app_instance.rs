use crate::application::WeakSession;
use crate::infrastructure::plugin::App;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::AppHandle;
use anyhow::{anyhow, Context, Result};
use playtime_clip_engine::proto::{
    event_reply, reply, ClipEngineReceivers, Empty, EventReply, Reply,
};
use prost::Message;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};
use tokio::task::JoinHandle;
use validator::HasLen;

pub type SharedAppInstance = Rc<RefCell<dyn AppInstance>>;

pub trait AppInstance: Debug {
    fn is_running(&self) -> bool;

    fn start_or_show(&mut self, owning_window: Window) -> Result<()>;

    fn stop(&mut self) -> Result<()>;

    fn send(&self, reply: &Reply) -> Result<()>;

    fn notify_app_is_ready(&mut self, callback: AppCallback);
}

pub fn create_shared_app_instance(session: WeakSession) -> SharedAppInstance {
    fn share(value: impl AppInstance + 'static) -> SharedAppInstance {
        Rc::new(RefCell::new(value))
    }
    // I was experimenting with 2 different ways of embedding the app GUI into REAPER:
    //
    // - Parented mode: We create a new SWELL window on ReaLearn side (HWND on Windows, NSView on
    //   macOS) and the app renders its GUI *within* it.
    // - Standalone mode: The app fires up its own window.
    //
    // Embedding the app in "parented" mode is in theory preferable because:
    //
    // 1. Only "parented" mode makes it possible to dock the app GUI (in case we want to do that
    //    one day).
    // 2. ReaLearn has full control over the window and can listen to its events.
    // 3. We stop sending events when the app window is hidden, not wasting resources when the
    //    app is not shown anyway. (However, with just a bit more effort, we could implement this
    //    for standalone mode as well.)
    if cfg!(target_os = "windows") {
        // On Windows, parented mode works in general. With a few tricks (see AppPanel View
        // implementation). However, I had issues completely removing the window title bar, maybe
        // because SWELL windows are dialog windows and they work differently? Anyway, this was
        // the reason that I switched to a standalone window.
        // let app_panel = AppPanel::new(session);
        // let instance = ParentedAppInstance {
        //     panel: SharedView::new(app_panel),
        // };
        // share(instance)
        let instance = StandaloneAppInstance {
            session,
            running_state: None,
        };
        share(instance)
    } else if cfg!(target_os = "macos") {
        // On macOS, parented mode is possible only by using Cocoa child windows (see app side
        // embedding docs). This means that the app doesn't really render itself in the NSView
        // provided by ReaLearn but places an NSWindow on top of the NSWindow provided by ReaLearn.
        // It works but it needs a few keyboard tricks and - most importantly - it doesn't work
        // well if the app itself wants to control its window (e.g. going full screen or changing
        // the window opacity). It will try to control the child window, not the outer window.
        // This could be solved on app side by navigating up the child/parent window chain, but
        // it's not something I want to do now as long as we don't support docking anyway.
        // Therefore: Standalone mode on macOS!
        let instance = StandaloneAppInstance {
            session,
            running_state: None,
        };
        share(instance)
    } else {
        panic!("OS not supported yet");
    }
}

/// App will run within a SWELL window that is provided by ReaLearn (`AppPanel`).
///
/// This is currently only a good choice on Windows. On macOS, the app uses an NSViewController
/// that's supposed to be attached to the NSWindow in order to manage its content view (NSView).
/// But SWELL doesn't just provide the NSWindow, it also provides and manages the content view.
/// Letting the content view be managed by both the NSViewController and SWELL is not possible.
///
/// There's the possibility to use child windows on macOS but this means that if the app itself
/// tries to access and control its containing window, it's going to affect the *child* window
/// and not the window provided ReaLearn. It also needs more attention when it comes to
/// keyboard shortcut forwarding and comes with probably a whole bunch of other corner cases.
/// It's still an interesting possibility, especially when it comes to implementing docking.
#[derive(Debug)]
pub struct ParentedAppInstance {
    panel: SharedView<AppPanel>,
}

impl AppInstance for ParentedAppInstance {
    fn is_running(&self) -> bool {
        self.panel.is_open()
    }

    fn start_or_show(&mut self, owning_window: Window) -> Result<()> {
        if let Some(window) = self.panel.view_context().window() {
            // If window already open (and maybe just hidden), simply show it.
            window.show();
            return Ok(());
        }
        // Fail fast if library not available
        App::get_or_load_app_library()?;
        // Then open. This actually only opens the SWELL window. The real stuff is done
        // in the "opened" handler of the SWELL window.
        self.panel.clone().open(owning_window);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.panel.close();
        Ok(())
    }

    fn send(&self, reply: &Reply) -> Result<()> {
        self.panel.send_to_app(reply)
    }

    fn notify_app_is_ready(&mut self, callback: AppCallback) {
        self.panel.notify_app_is_ready(callback);
    }
}

/// App will run in its own window.
///
/// This is possible on all OS.
#[derive(Debug)]
struct StandaloneAppInstance {
    session: WeakSession,
    running_state: Option<StandaloneAppRunningState>,
}

#[derive(Debug)]
struct StandaloneAppRunningState {
    common_state: CommonAppRunningState,
    event_subscription_join_handle: Option<JoinHandle<()>>,
}

impl Drop for StandaloneAppRunningState {
    fn drop(&mut self) {
        if let Some(join_handle) = self.event_subscription_join_handle.take() {
            join_handle.abort();
        }
    }
}

impl AppInstance for StandaloneAppInstance {
    fn is_running(&self) -> bool {
        self.running_state.is_some()
    }

    fn start_or_show(&mut self, _owning_window: Window) -> Result<()> {
        let app_library = App::get_or_load_app_library()?;
        if let Some(running_state) = &self.running_state {
            app_library.show_app_instance(None, running_state.common_state.app_handle)?;
            return Ok(());
        }
        let session_id = extract_session_id(&self.session)?;
        let app_handle = app_library.start_app_instance(None, session_id)?;
        let running_state = StandaloneAppRunningState {
            common_state: CommonAppRunningState {
                app_handle,
                app_callback: None,
            },
            event_subscription_join_handle: None,
        };
        self.running_state = Some(running_state);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.running_state
            .take()
            .ok_or(anyhow!("app was already stopped"))?
            .common_state
            .stop(None)
    }

    fn send(&self, reply: &Reply) -> Result<()> {
        self.running_state
            .as_ref()
            .context("app not open")?
            .common_state
            .send(reply)
    }

    fn notify_app_is_ready(&mut self, callback: AppCallback) {
        let Some(running_state) = &mut self.running_state else {
            return;
        };
        let Ok(session_id) = extract_session_id(&self.session) else {
            return;
        };
        // Handshake finished! The app has the host callback and we have the app callback.
        running_state.common_state.app_callback = Some(callback);
        // Now we can start passing events to the app callback
        let mut receivers = subscribe_to_events();
        let join_handle = App::get().spawn_in_async_runtime(async move {
            receivers
                .keep_processing_updates(&session_id, &|event_reply| {
                    let reply = Reply {
                        value: Some(reply::Value::EventReply(event_reply)),
                    };
                    send_to_app(callback, &reply);
                })
                .await;
        });
        running_state.event_subscription_join_handle = Some(join_handle);
    }
}

#[derive(Debug)]
pub struct AppPanel {
    view: ViewContext,
    session: WeakSession,
    running_state: RefCell<Option<ParentedAppRunningState>>,
}

#[derive(Debug)]
struct ParentedAppRunningState {
    common_state: CommonAppRunningState,
    event_receivers: Option<ClipEngineReceivers>,
}

impl ParentedAppRunningState {
    pub fn send_pending_events(&mut self, session_id: &str) {
        let (Some(app_callback), Some(event_receivers)) =
            (self.common_state.app_callback, &mut self.event_receivers)
        else {
            return;
        };
        event_receivers.process_pending_updates(session_id, &|event_reply| {
            let reply = Reply {
                value: Some(reply::Value::EventReply(event_reply)),
            };
            send_to_app(app_callback, &reply);
        });
    }
}

#[derive(Debug)]
struct CommonAppRunningState {
    app_handle: AppHandle,
    app_callback: Option<AppCallback>,
}

impl AppPanel {
    pub fn new(session: WeakSession) -> Self {
        Self {
            view: Default::default(),
            session,
            running_state: RefCell::new(None),
        }
    }

    pub fn send_to_app(&self, reply: &Reply) -> Result<()> {
        self.running_state
            .borrow()
            .as_ref()
            .context("app not open")?
            .common_state
            .send(reply)
    }

    pub fn notify_app_is_ready(&self, callback: AppCallback) {
        let mut open_state = self.running_state.borrow_mut();
        let Some(open_state) = open_state.as_mut() else {
            return;
        };
        // Handshake finished! The app has the host callback and we have the app callback.
        open_state.common_state.app_callback = Some(callback);
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
        window.set_text("Playtime");
        let app_library = App::get_or_load_app_library()?;
        let session_id = extract_session_id(&self.session)?;
        let app_handle = app_library.start_app_instance(Some(window), session_id)?;
        let running_state = ParentedAppRunningState {
            common_state: CommonAppRunningState {
                app_handle,
                app_callback: None,
            },
            event_receivers: Some(subscribe_to_events()),
        };
        *self.running_state.borrow_mut() = Some(running_state);
        Ok(())
    }

    fn stop(&self, window: Window) -> Result<()> {
        self.running_state
            .borrow_mut()
            .take()
            .ok_or(anyhow!("app was already stopped"))?
            .common_state
            .stop(Some(window))
    }
}

impl CommonAppRunningState {
    pub fn send(&self, reply: &Reply) -> Result<()> {
        let app_callback = self.app_callback.context("app callback not known yet")?;
        send_to_app(app_callback, reply);
        Ok(())
    }

    pub fn stop(&self, window: Option<Window>) -> Result<()> {
        App::get_or_load_app_library()?.stop_app_instance(window, self.app_handle)
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
        self.stop(window).unwrap();
    }

    fn shown_or_hidden(self: SharedView<Self>, shown: bool) -> bool {
        if shown {
            // Send events to app again.
            if let Some(open_state) = self.running_state.borrow_mut().as_mut() {
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
            if let Some(open_state) = self.running_state.borrow_mut().as_mut() {
                open_state.event_receivers = None;
            }
            self.stop_timer();
        }
        true
    }

    /// On Windows, this is necessary to resize contained app.
    ///
    /// On macOS, this would have no effect because the app window is not a child view (NSView) but
    /// a child window (NSWindow). Resizing according to the parent window (the SWELL window) is
    /// done on app side.
    #[cfg(target_os = "windows")]
    fn resized(self: SharedView<Self>) -> bool {
        crate::infrastructure::ui::egui_views::on_parent_window_resize(self.view.require_window())
    }

    fn timer(&self, id: usize) -> bool {
        if id != TIMER_ID {
            return false;
        }
        let mut open_state = self.running_state.borrow_mut();
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

// TODO-high-ms4 We extract the session ID manually whenever we start the app instead of assigning
//  it to the AppInstance right at the start. Reason: The session ID can be changed by the user.
//  This is not ideal. It won't event prevent that the user changes the session ID during app
//  lifetime ... it just won't work anymore if that happens. I think we need to use the InstanceId
//  and hold a global mapping from session ID to instance ID in the app. Or maybe better: We use
//  the instance ID whenever we are embedded, not the session ID! Then the "matrix ID" refers
//  to the instance ID when embedded and to the session ID when remote.
fn extract_session_id(session: &WeakSession) -> Result<String> {
    Ok(session
        .upgrade()
        .ok_or_else(|| anyhow!("session gone"))?
        .borrow()
        .id()
        .to_string())
}
