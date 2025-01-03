use crate::domain::InstanceId;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::proto::{
    event_reply, occasional_global_update, reply, EventReply, GetOccasionalGlobalUpdatesReply,
    OccasionalGlobalUpdate, ProtoReceivers, Reply,
};
use crate::infrastructure::ui::AppHandle;
use anyhow::{anyhow, bail, Context, Result};
use base::hash_util::NonCryptoHashMap;
use fragile::Fragile;
use once_cell::sync::Lazy;
use prost::Message;
use reaper_medium::Hwnd;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use swell_ui::Window;
use tokio::task::JoinHandle;

pub type SharedAppInstance = Rc<RefCell<dyn AppInstance>>;

pub trait AppInstance: Debug {
    fn is_running(&self) -> bool;

    fn has_focus(&self) -> bool;

    fn is_visible(&self) -> bool;

    fn start_or_show(&mut self, owning_window: Window, location: Option<String>) -> Result<()>;

    fn hide(&mut self) -> Result<()>;

    fn stop(&mut self) -> Result<()>;

    fn send(&self, reply: &Reply) -> Result<()>;

    fn notify_app_is_ready(&mut self, callback: AppCallback);

    /// Returns the app window.
    ///
    /// On Windows, that is the app handle (HWND), which is the **parent** window of whatever REAPER passes into
    /// the `HwndInfo` hook.
    ///
    /// On macOS, this is the content view (NSView) of the app handle (NSWindow), which is exactly what
    /// REAPER passes into the `HwndInfo` hook.
    fn window(&self) -> Option<Hwnd>;

    fn notify_app_is_in_text_entry_mode(&mut self, is_in_text_entry_mode: bool);
}

#[allow(clippy::if_same_then_else)]
pub fn create_shared_app_instance(instance_id: InstanceId) -> SharedAppInstance {
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
            instance_id,
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
            instance_id,
            running_state: None,
        };
        share(instance)
    } else {
        share(DummyAppInstance)
    }
}

#[derive(Debug)]
struct DummyAppInstance;

impl AppInstance for DummyAppInstance {
    fn is_running(&self) -> bool {
        false
    }

    fn has_focus(&self) -> bool {
        false
    }

    fn is_visible(&self) -> bool {
        false
    }

    fn start_or_show(&mut self, _owning_window: Window, _location: Option<String>) -> Result<()> {
        bail!("Linux support for the Helgobox App (including the Playtime user interface) is currently at stage 1! That means the Helgobox App can't yet run embedded within REAPER, but it's possible to use it as a separate program that connects to REAPER (\"remote mode\").\n\nRead the instructions at https://bit.ly/3W51oEe to learn more about this temporary workaround. Subscribe to https://bit.ly/3BQvjcH to follow the development progress of Linux support stage 2.")
    }

    fn hide(&mut self) -> Result<()> {
        bail!("not implemented for Linux")
    }

    fn stop(&mut self) -> Result<()> {
        bail!("not implemented for Linux")
    }

    fn send(&self, _reply: &Reply) -> Result<()> {
        bail!("not implemented for Linux")
    }

    fn notify_app_is_ready(&mut self, _callback: AppCallback) {}

    fn window(&self) -> Option<Hwnd> {
        None
    }

    fn notify_app_is_in_text_entry_mode(&mut self, _is_in_text_entry_mode: bool) {}
}

/// App will run in its own window.
///
/// This is possible on all OS.
#[derive(Debug)]
struct StandaloneAppInstance {
    instance_id: InstanceId,
    running_state: Option<StandaloneAppRunningState>,
}

impl StandaloneAppInstance {
    fn register_app_window(&mut self, is_in_text_entry_mode: bool) {
        let mut map = REGISTERED_APP_WINDOWS.get().borrow_mut();
        let Some(hwnd) = self.window() else {
            return;
        };
        let window_state = AppWindowState {
            is_in_text_entry_mode,
        };
        map.insert(hwnd, window_state);
    }
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

    fn has_focus(&self) -> bool {
        match &self.running_state {
            None => false,
            Some(state) => state.common_state.has_focus(),
        }
    }

    fn is_visible(&self) -> bool {
        match &self.running_state {
            None => false,
            Some(state) => state.common_state.is_visible(),
        }
    }

    fn start_or_show(&mut self, _owning_window: Window, location: Option<String>) -> Result<()> {
        let app_library = BackboneShell::get_app_library()?;
        if let Some(running_state) = &self.running_state {
            app_library.show_app_instance(None, running_state.common_state.app_handle)?;
            if let Some(location) = location {
                // Hmmm, yeah ...
                let _ = self.send(&Reply {
                    value: Some(reply::Value::EventReply(EventReply {
                        value: Some(event_reply::Value::OccasionalGlobalUpdatesReply(
                            GetOccasionalGlobalUpdatesReply {
                                global_updates: vec![OccasionalGlobalUpdate {
                                    update: Some(occasional_global_update::Update::GoToLocation(
                                        location,
                                    )),
                                }],
                            },
                        )),
                    })),
                });
            }
            return Ok(());
        }
        let start_location = location.unwrap_or_else(|| "/".to_string());
        let app_handle = app_library.start_app_instance(None, self.instance_id, start_location)?;
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

    fn hide(&mut self) -> Result<()> {
        self.running_state
            .as_ref()
            .ok_or(anyhow!("app was already stopped"))?
            .common_state
            .hide()
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
        let instance_id = self.instance_id;
        // Handshake finished! The app has the host callback and we have the app callback.
        running_state.common_state.app_callback = Some(callback);
        // Now we can start passing events to the app callback
        let mut receivers = subscribe_to_events();
        let join_handle = BackboneShell::get().spawn_in_async_runtime(async move {
            receivers
                .keep_processing_updates(instance_id, &|event_reply| {
                    let reply = Reply {
                        value: Some(reply::Value::EventReply(event_reply)),
                    };
                    send_to_app(callback, &reply);
                })
                .await;
        });
        running_state.event_subscription_join_handle = Some(join_handle);
        // Register app window
        self.register_app_window(false);
    }

    fn window(&self) -> Option<Hwnd> {
        let running_state = self.running_state.as_ref()?;
        running_state.common_state.window()
    }

    fn notify_app_is_in_text_entry_mode(&mut self, is_in_text_entry_mode: bool) {
        self.register_app_window(is_in_text_entry_mode);
    }
}

static REGISTERED_APP_WINDOWS: Lazy<Fragile<RefCell<NonCryptoHashMap<Hwnd, AppWindowState>>>> =
    Lazy::new(Default::default);

#[derive(Default)]
struct AppWindowState {
    is_in_text_entry_mode: bool,
}

/// Relevant for all OS.
pub fn is_app_window(hwnd: Hwnd) -> bool {
    let yes = REGISTERED_APP_WINDOWS.get().borrow().contains_key(&hwnd);
    if yes {
        return true;
    }
    if let Some(parent) = Window::from_hwnd(hwnd).parent() {
        is_app_window(parent.raw_hwnd())
    } else {
        false
    }
}

/// Relevant on Windows only.
pub fn app_window_is_in_text_entry_mode(hwnd: Hwnd) -> Option<bool> {
    let map = REGISTERED_APP_WINDOWS.get().borrow();
    let state = map.get(&hwnd)?;
    Some(state.is_in_text_entry_mode)
}

#[derive(Debug)]
struct CommonAppRunningState {
    app_handle: AppHandle,
    app_callback: Option<AppCallback>,
}

impl CommonAppRunningState {
    pub fn send(&self, reply: &Reply) -> Result<()> {
        let app_callback = self.app_callback.context("app callback not known yet")?;
        send_to_app(app_callback, reply);
        Ok(())
    }

    pub fn window(&self) -> Option<Hwnd> {
        let app_library = BackboneShell::get_app_library().ok()?;
        app_library
            .app_instance_get_window(self.app_handle)
            .ok()
            .flatten()
    }

    pub fn is_visible(&self) -> bool {
        let Ok(app_library) = BackboneShell::get_app_library() else {
            return false;
        };
        app_library
            .app_instance_is_visible(self.app_handle)
            .unwrap_or(false)
    }

    pub fn has_focus(&self) -> bool {
        let Ok(app_library) = BackboneShell::get_app_library() else {
            return false;
        };
        app_library
            .app_instance_has_focus(self.app_handle)
            .unwrap_or(false)
    }

    pub fn hide(&self) -> Result<()> {
        BackboneShell::get_app_library()?.hide_app_instance(self.app_handle)
    }

    pub fn stop(&self, window: Option<Window>) -> Result<()> {
        BackboneShell::get_app_library()?.stop_app_instance(window, self.app_handle)
    }
}

fn send_to_app(app_callback: AppCallback, reply: &Reply) {
    let vec = reply.encode_to_vec();
    let length = vec.len();
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

fn subscribe_to_events() -> ProtoReceivers {
    BackboneShell::get()
        .proto_hub()
        .senders()
        .subscribe_to_all()
}
