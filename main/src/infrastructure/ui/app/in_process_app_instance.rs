use crate::domain::InstanceId;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::proto::{
    EventReply, GetOccasionalGlobalUpdatesReply, OccasionalGlobalUpdate, ProtoReceivers, Reply,
    event_reply, occasional_global_update, reply,
};
use crate::infrastructure::ui::{AppHandle, AppInstance, AppPage, InstanceRef, called_from_dart};
use anyhow::{Context, anyhow};
use base::hash_util::NonCryptoHashMap;
use fragile::Fragile;
use once_cell::sync::Lazy;
use prost::Message;
use reaper_medium::Hwnd;
use std::cell::RefCell;
use swell_ui::Window;
use tokio::task::JoinHandle;

/// App will run in its own window.
///
/// This is possible on all OS.
#[derive(Debug)]
pub struct InProcessStandaloneAppInstance {
    instance_id: InstanceId,
    running_state: Option<InProcessStandaloneAppRunningState>,
}

impl InProcessStandaloneAppInstance {
    pub fn new(instance_id: InstanceId) -> Self {
        Self {
            instance_id,
            running_state: None,
        }
    }

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
struct InProcessStandaloneAppRunningState {
    common_state: InProcessAppRunningState,
    event_subscription_join_handle: Option<JoinHandle<()>>,
}

impl Drop for InProcessStandaloneAppRunningState {
    fn drop(&mut self) {
        if let Some(join_handle) = self.event_subscription_join_handle.take() {
            join_handle.abort();
        }
    }
}

impl AppInstance for InProcessStandaloneAppInstance {
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

    fn start_or_show(
        &mut self,
        _owning_window: Window,
        page: Option<AppPage>,
    ) -> anyhow::Result<()> {
        let app_library = BackboneShell::get_app_library()?;
        if let Some(running_state) = &self.running_state {
            app_library.show_app_instance(None, running_state.common_state.app_handle)?;
            if let Some(page) = page {
                // Hmmm, yeah ...
                let _ = self.send(&Reply {
                    value: Some(reply::Value::EventReply(EventReply {
                        value: Some(event_reply::Value::OccasionalGlobalUpdatesReply(
                            GetOccasionalGlobalUpdatesReply {
                                global_updates: vec![OccasionalGlobalUpdate {
                                    update: Some(occasional_global_update::Update::GoToLocation(
                                        page.location(InstanceRef::Host),
                                    )),
                                }],
                            },
                        )),
                    })),
                });
            }
            return Ok(());
        }
        let start_location = page
            .map(|p| p.location(InstanceRef::Host))
            .unwrap_or_else(|| "/".to_string());
        let app_handle = app_library.start_app_instance(None, self.instance_id, start_location)?;
        let running_state = InProcessStandaloneAppRunningState {
            common_state: InProcessAppRunningState {
                app_handle,
                app_callback: None,
            },
            event_subscription_join_handle: None,
        };
        self.running_state = Some(running_state);
        Ok(())
    }

    fn hide(&mut self) -> anyhow::Result<()> {
        self.running_state
            .as_ref()
            .ok_or(anyhow!("app was already stopped"))?
            .common_state
            .hide()
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        // On macOS, with the new Flutter version containing the "Great thread merge" (3.47.3 for example), stopping
        // the application can easily lead to a crash when built with the official Flutter engine.
        // This is because the (new) FlutterRunLoop continues to run even after the view and engine has stopped.
        // In one case, it asynchronously performs a task (ResizeSynchronizer task) that has been scheduled to be executed async, and it performs this
        // task when the engine is already gone. This task tries to access the engine and things crash.
        // Relevant log messages:
        //     Deiniting StandaloneFlutterWindow
        //     Deiniting StandaloneFlutterViewController
        //     Communicating on a dead channel.
        //     embedder.cc (3235): 'FlutterEngineOnVsync' returned 'kInvalidArguments'. Invalid engine handle.
        //  See https://github.com/flutter/flutter/blob/da72d5936d697169c8ee2535ad6f615b0352dabd/engine/src/flutter/shell/platform/darwin/macos/framework/Source/FlutterRunLoop.swift#L70
        // I forked the engine to fix this in a few places, mostly by preventing illegal engine accesses.
        self.running_state
            .take()
            .ok_or(anyhow!("app was already stopped"))?
            .common_state
            .stop(None)?;
        Ok(())
    }

    fn send(&self, reply: &Reply) -> anyhow::Result<()> {
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
struct InProcessAppRunningState {
    app_handle: AppHandle,
    app_callback: Option<AppCallback>,
}

impl InProcessAppRunningState {
    pub fn send(&self, reply: &Reply) -> anyhow::Result<()> {
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

    pub fn hide(&self) -> anyhow::Result<()> {
        BackboneShell::get_app_library()?.hide_app_instance(self.app_handle)
    }

    #[allow(dead_code)]
    pub fn stop(&self, window: Option<Window>) -> anyhow::Result<()> {
        BackboneShell::get_app_library()?.stop_app_instance(window, self.app_handle)
    }
}

fn send_to_app(app_callback: AppCallback, reply: &Reply) {
    let bytes = reply.encode_to_vec();
    if called_from_dart() {
        // We must never call back into Dart if we are being called by Dart!
        BackboneShell::get().spawn_in_async_runtime(async move {
            send_to_app_internal(app_callback, bytes);
        });
    } else {
        send_to_app_internal(app_callback, bytes);
    }
}

fn send_to_app_internal(app_callback: AppCallback, bytes: Vec<u8>) {
    let length = bytes.len();
    let boxed_slice = bytes.into_boxed_slice();
    // The app side is responsible for freeing the memory!
    // We really need to pass owned data to the app because it's written in Dart and Dart code
    // doesn't execute on the same thread. It will execute the code asynchronously in another
    // thread and at that point the data still needs to be valid.
    let raw_ptr = Box::into_raw(boxed_slice);
    // This can lead to a User-mode data execution prevention (DEP) violation if the DLL
    // is unloaded at this point. This was possible on Windows on REAPER exit. However, it shouldn't happen anymore
    // now that we drop everything in the HiddenHelperPanel instead of waiting for the DLL detach.
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
