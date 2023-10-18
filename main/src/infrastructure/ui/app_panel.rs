use crate::infrastructure::plugin::App;
use crate::infrastructure::server::services::playtime_service::AppMatrixProvider;
use crate::infrastructure::ui::bindings::root;
use anyhow::{anyhow, bail, Context, Result};
use base::Global;
use libloading::{Library, Symbol};
use playtime_clip_engine::proto::command_request::Value;
use playtime_clip_engine::proto::{command_request, ClipEngineCommandHandler, CommandRequest};
use prost::Message;
use reaper_high::{Reaper, TaskSupport};
use reaper_low::raw;
use reaper_low::raw::HWND;
use std::env;
use std::error::Error;
use std::ffi::{c_char, CString};
use std::path::{Path, PathBuf};
use std::ptr::null;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct AppPanel {
    view: ViewContext,
    app: &'static LoadedApp,
}

impl AppPanel {
    pub fn new(app: &'static LoadedApp) -> Result<Self> {
        let panel = Self {
            view: Default::default(),
            app,
        };
        Ok(panel)
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
        self.app.run_in_parent(window).unwrap();
        true
    }

    /// On macOS, the app window is a child *window* of this window, not a child *view*. We need
    /// to close it explicitly when this window is closed.
    #[cfg(target_os = "macos")]
    fn closed(self: SharedView<Self>, _window: Window) {
        if let Some(child_window) = self.view.window().and_then(|w| w.first_child_window()) {
            child_window.close();
        }
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

#[derive(Debug)]
pub struct LoadedApp {
    app_base_dir: PathBuf,
    main_library: Library,
    _dependencies: Vec<Library>,
}

// #[cfg(target_os = "macos")]
// const APP_BASE_DIR: &str = "/Users/helgoboss/Documents/projects/dev/playtime/build/macos/Build/Products/Release/playtime.app";
//
// #[cfg(target_os = "windows")]
// const APP_BASE_DIR: &str =
//     "C:\\Users\\benja\\Documents\\projects\\dev\\playtime\\build\\windows\\runner\\Release";

// #[cfg(target_os = "linux")]
// const APP_BASE_DIR: &str = "TODO";

impl LoadedApp {
    pub fn load(app_base_dir: PathBuf) -> Result<Self> {
        let (main_library, dependencies) = {
            #[cfg(target_os = "windows")]
            {
                (
                    "playtime.dll",
                    ["flutter_windows.dll", "url_launcher_windows_plugin.dll"],
                )
            }
            #[cfg(target_os = "macos")]
            {
                (
                    "Contents/MacOS/playtime",
                    [
                        "Contents/Frameworks/FlutterMacOS.framework/FlutterMacOS",
                        "Contents/Frameworks/cryptography_flutter.framework/cryptography_flutter",
                        "Contents/Frameworks/native_context_menu.framework/native_context_menu",
                        "Contents/Frameworks/path_provider_foundation.framework/path_provider_foundation",
                        "Contents/Frameworks/url_launcher_macos.framework/url_launcher_macos",
                        "Contents/Frameworks/screen_retriever.framework/screen_retriever",
                        "Contents/Frameworks/window_manager.framework/window_manager",
                    ],
                )
            }
            #[cfg(target_os = "linux")]
            {
                (
                    "playtime.so",
                    ["flutter_linux.so", "url_launcher_linux_plugin.so"],
                )
            }
        };
        let loaded_dependencies: Result<Vec<Library>> = dependencies
            .into_iter()
            .map(|dep| load_library(&app_base_dir.join(dep)))
            .collect();
        let app = LoadedApp {
            main_library: load_library(&app_base_dir.join(main_library))?,
            app_base_dir,
            _dependencies: loaded_dependencies?,
        };
        Ok(app)
    }

    pub fn run_in_parent(&self, parent_window: Window) -> Result<()> {
        let app_base_dir_str = self
            .app_base_dir
            .to_str()
            .ok_or(anyhow!("app base dir is not an UTF-8 string"))?;
        let app_base_dir_c_string = CString::new(app_base_dir_str)
            .map_err(|_| anyhow!("app base dir contains a nul byte"))?;
        with_temporarily_changed_working_directory(&self.app_base_dir, || {
            prepare_app_launch();
            let successful = unsafe {
                let symbol: Symbol<RunAppInParent> = self
                    .main_library
                    .get(b"run_app_in_parent\0")
                    .map_err(|_| anyhow!("failed to load run_app_in_parent function"))?;
                symbol(
                    parent_window.raw(),
                    app_base_dir_c_string.as_ptr(),
                    invoke_host,
                )
            };
            if !successful {
                return bail!("couldn't launch app");
            }
            Ok(())
        })
    }
}

/// Function that's used from Dart in order to call the host.
///
/// Attention: This is *not* called from the main thread but from some special Flutter UI thread.
#[no_mangle]
extern "C" fn invoke_host(data: *const u8, length: i32) {
    let bytes = unsafe { std::slice::from_raw_parts(data, length as usize) };
    let req = CommandRequest::decode(bytes).unwrap();
    let Some(req) = req.value else {
        return;
    };
    // We need to execute the commands on the main thread!
    Global::task_support()
        .do_in_main_thread_asap(|| process_command(req).unwrap())
        .unwrap();
}

/// Signature of the function that's used from the app in order to call the host.
type HostCallback = extern "C" fn(data: *const u8, length: i32);

/// Signature of the function that's used from the host in order to call the app.
type AppCallback = extern "C" fn(data: *const u8, length: i32);

/// Signature of the function that we use to open a new App window.
type RunAppInParent = unsafe extern "C" fn(
    parent_window: HWND,
    app_base_dir_utf8_c_str: *const c_char,
    host_callback: HostCallback,
) -> bool;

fn prepare_app_launch() {
    #[cfg(target_os = "macos")]
    {
        // This is only necessary and only considered by Flutter Engine when Flutter is compiled in
        // debug mode. In release mode, Flutter will work with AOT data embedded in the binary.
        let env_vars = [
            ("FLUTTER_ENGINE_SWITCHES", "3"),
            ("FLUTTER_ENGINE_SWITCH_1", "snapshot-asset-path=Contents/Frameworks/App.framework/Versions/A/Resources/flutter_assets"),
            ("FLUTTER_ENGINE_SWITCH_2", "vm-snapshot-data=vm_snapshot_data"),
            ("FLUTTER_ENGINE_SWITCH_3", "isolate-snapshot-data=isolate_snapshot_data"),
        ];
        for (key, value) in env_vars {
            env::set_var(key, value);
        }
    }
}

fn with_temporarily_changed_working_directory<R>(
    new_dir: impl AsRef<Path>,
    f: impl FnOnce() -> R,
) -> R {
    let previous_dir = env::current_dir();
    let dir_change_was_successful = env::set_current_dir(new_dir).is_ok();
    let r = f();
    if dir_change_was_successful {
        if let Ok(d) = previous_dir {
            let _ = env::set_current_dir(d);
        }
    }
    r
}

fn load_library(path: &Path) -> Result<Library> {
    match path.try_exists() {
        Ok(false) => bail!("App library {path:?} not found."),
        Err(e) => bail!("App library {path:?} not accessible: {e}"),
        _ => {}
    }
    let lib = unsafe { Library::new(path) };
    lib.map_err(|_| anyhow!("Failed to load app library {path:?}."))
}

fn process_command(req: command_request::Value) -> Result<(), tonic::Status> {
    // TODO-low This should be a more generic command handler in future (not just clip engine)
    let command_handler = ClipEngineCommandHandler::new(AppMatrixProvider);
    use command_request::Value::*;
    match req {
        NotifyAppIsReady(req) => {
            let ptr = req.app_callback_address as *const ();
            let app_callback: AppCallback = unsafe { std::mem::transmute(ptr) };
            // app_callback(null(), 0);
            // TODO-high Save the callback somewhere
        }
        ProveAuthenticity(req) => {}
        // TODO-high CONTINUE Let Dart detect if embedded and in this case use the in-process command calls.
        TriggerMatrix(req) => {
            command_handler.trigger_matrix(req)?;
        }
        SetMatrixSettings(req) => {
            command_handler.set_matrix_settings(req)?;
        }
        SetMatrixTempo(req) => {
            command_handler.set_matrix_tempo(req)?;
        }
        SetMatrixVolume(req) => {
            command_handler.set_matrix_volume(req)?;
        }
        SetMatrixPan(req) => {
            command_handler.set_matrix_pan(req)?;
        }
        TriggerColumn(req) => {
            command_handler.trigger_column(req)?;
        }
        SetColumnSettings(req) => {
            command_handler.set_column_settings(req)?;
        }
        SetColumnVolume(req) => {
            command_handler.set_column_volume(req)?;
        }
        SetColumnPan(req) => {
            command_handler.set_column_pan(req)?;
        }
        SetColumnTrack(req) => {
            command_handler.set_column_track(req)?;
        }
        DragColumn(req) => {
            command_handler.drag_column(req)?;
        }
        SetTrackName(req) => {
            command_handler.set_track_name(req)?;
        }
        SetTrackInput(req) => {
            command_handler.set_track_input(req)?;
        }
        SetTrackInputMonitoring(req) => {
            command_handler.set_track_input_monitoring(req)?;
        }
        TriggerRow(req) => {
            command_handler.trigger_row(req)?;
        }
        SetRowData(req) => {
            command_handler.set_row_data(req)?;
        }
        DragRow(req) => {
            command_handler.drag_row(req)?;
        }
        TriggerSlot(req) => {
            command_handler.trigger_slot(req)?;
        }
        DragSlot(req) => {
            command_handler.drag_slot(req)?;
        }
        TriggerClip(req) => {
            command_handler.trigger_clip(req)?;
        }
        SetClipName(req) => {
            command_handler.set_clip_name(req)?;
        }
        SetClipData(req) => {
            command_handler.set_clip_data(req)?;
        }
    }
    Ok(())
}
