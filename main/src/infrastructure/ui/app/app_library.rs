use crate::infrastructure::plugin::App;
use crate::infrastructure::server::services::playtime_service::AppMatrixProvider;
use crate::infrastructure::ui::AppCallback;
use anyhow::{anyhow, bail, Result};
use base::Global;
use egui::Key::V;
use libloading::{Library, Symbol};
use playtime_clip_engine::base::Matrix;
use playtime_clip_engine::proto;
use playtime_clip_engine::proto::{
    create_initial_matrix_updates, create_initial_slot_updates, create_initial_track_updates,
    event_reply, query_result, reply, ClipEngineRequestHandler, CommandRequest, EventReply,
    MatrixProvider, QueryReply, QueryResult, Reply, Request,
};
use prost::Message;
use reaper_low::raw::HWND;
use std::env;
use std::ffi::{c_char, CString};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::ptr::null;
use swell_ui::Window;
use tonic::Status;

#[derive(Debug)]
pub struct AppLibrary {
    app_base_dir: PathBuf,
    main_library: Library,
    _dependencies: Vec<Library>,
}

impl AppLibrary {
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
        let library = AppLibrary {
            main_library: load_library(&app_base_dir.join(main_library))?,
            app_base_dir,
            _dependencies: loaded_dependencies?,
        };
        Ok(library)
    }

    pub fn run_in_parent(&self, parent_window: Window, session_id: String) -> Result<()> {
        let app_base_dir_str = self
            .app_base_dir
            .to_str()
            .ok_or(anyhow!("app base dir is not an UTF-8 string"))?;
        let app_base_dir_c_string = CString::new(app_base_dir_str)
            .map_err(|_| anyhow!("app base dir contains a nul byte"))?;
        let session_id_c_string =
            CString::new(session_id).map_err(|_| anyhow!("session ID contains a nul byte"))?;
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
                    session_id_c_string.as_ptr(),
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
    let req = Request::decode(bytes).unwrap();
    let Some(req) = req.value else {
        return;
    };
    // We need to execute commands on the main thread!
    Global::task_support()
        .do_in_main_thread_asap(|| process_request(req).unwrap())
        .unwrap();
}

/// Signature of the function that we use to open a new App window.
type RunAppInParent = unsafe extern "C" fn(
    parent_window: HWND,
    app_base_dir_utf8_c_str: *const c_char,
    host_callback: HostCallback,
    session_id: *const c_char,
) -> bool;

/// Signature of the function that's used from the app in order to call the host.
type HostCallback = extern "C" fn(data: *const u8, length: i32);

fn load_library(path: &Path) -> Result<Library> {
    match path.try_exists() {
        Ok(false) => bail!("App library {path:?} not found."),
        Err(e) => bail!("App library {path:?} not accessible: {e}"),
        _ => {}
    }
    let lib = unsafe { Library::new(path) };
    lib.map_err(|_| anyhow!("Failed to load app library {path:?}."))
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

fn process_request(req: proto::request::Value) -> Result<(), tonic::Status> {
    use proto::request::Value;
    match req {
        Value::CommandRequest(req) => process_command(
            req.value
                .ok_or(Status::invalid_argument("command value missing"))?,
        ),
        Value::QueryRequest(req) => process_query(
            req.matrix_id,
            req.id,
            req.query.ok_or(Status::invalid_argument("query missing"))?,
        ),
    }
}

fn process_query(matrix_id: String, id: u32, query: proto::Query) -> Result<(), tonic::Status> {
    use proto::query::Value::*;
    let handler = ClipEngineRequestHandler::new(AppMatrixProvider);
    match query
        .value
        .ok_or(Status::invalid_argument("query value missing"))?
    {
        ProveAuthenticity(req) => {
            send_query_reply_to_app(matrix_id, id, async move {
                let value = handler.prove_authenticity(req).await?.into_inner();
                Ok(query_result::Value::ProveAuthenticityReply(value))
            });
        }
        GetClipDetail(req) => {
            send_query_reply_to_app(matrix_id, id, async move {
                let value = handler.get_clip_detail(req).await?.into_inner();
                Ok(query_result::Value::GetClipDetailReply(value))
            });
        }
    }
    Ok(())
}

fn process_command(req: proto::command_request::Value) -> Result<(), tonic::Status> {
    // TODO-low This should be a more generic command handler in future (not just clip engine)
    let handler = ClipEngineRequestHandler::new(AppMatrixProvider);
    use proto::command_request::Value::*;
    match req {
        // Embedding
        NotifyAppIsReady(req) => {
            // App instance is started. Put the app instance callback at the correct position.
            let ptr = req.app_callback_address as *const ();
            let app_callback: AppCallback = unsafe { std::mem::transmute(ptr) };
            let main_panel = App::get()
                .find_main_panel_by_session_id(&req.matrix_id)
                .ok_or(Status::not_found("instance not found"))?;
            main_panel.notify_app_is_ready(app_callback);
        }
        // Event subscription commands
        GetOccasionalMatrixUpdates(req) => {
            send_initial_events_to_app(&req.matrix_id, create_initial_matrix_updates)?;
        }
        GetOccasionalTrackUpdates(req) => {
            send_initial_events_to_app(&req.matrix_id, create_initial_track_updates)?;
        }
        GetOccasionalSlotUpdates(req) => {
            send_initial_events_to_app(&req.matrix_id, create_initial_slot_updates)?;
        }
        // Normal commands
        TriggerMatrix(req) => {
            handler.trigger_matrix(req)?;
        }
        SetMatrixSettings(req) => {
            handler.set_matrix_settings(req)?;
        }
        SetMatrixTempo(req) => {
            handler.set_matrix_tempo(req)?;
        }
        SetMatrixVolume(req) => {
            handler.set_matrix_volume(req)?;
        }
        SetMatrixPan(req) => {
            handler.set_matrix_pan(req)?;
        }
        TriggerColumn(req) => {
            handler.trigger_column(req)?;
        }
        SetColumnSettings(req) => {
            handler.set_column_settings(req)?;
        }
        SetColumnVolume(req) => {
            handler.set_column_volume(req)?;
        }
        SetColumnPan(req) => {
            handler.set_column_pan(req)?;
        }
        SetColumnTrack(req) => {
            Global::future_support().spawn_in_main_thread_from_main_thread(async move {
                handler.set_column_track(req).await?;
                Ok(())
            });
        }
        DragColumn(req) => {
            handler.drag_column(req)?;
        }
        SetTrackName(req) => {
            handler.set_track_name(req)?;
        }
        SetTrackInput(req) => {
            handler.set_track_input(req)?;
        }
        SetTrackInputMonitoring(req) => {
            handler.set_track_input_monitoring(req)?;
        }
        TriggerRow(req) => {
            handler.trigger_row(req)?;
        }
        SetRowData(req) => {
            handler.set_row_data(req)?;
        }
        DragRow(req) => {
            handler.drag_row(req)?;
        }
        TriggerSlot(req) => {
            handler.trigger_slot(req)?;
        }
        DragSlot(req) => {
            handler.drag_slot(req)?;
        }
        TriggerClip(req) => {
            handler.trigger_clip(req)?;
        }
        SetClipName(req) => {
            handler.set_clip_name(req)?;
        }
        SetClipData(req) => {
            handler.set_clip_data(req)?;
        }
    }
    Ok(())
}

fn send_initial_events_to_app<T: Into<event_reply::Value>>(
    matrix_id: &str,
    create_reply: impl FnOnce(&Matrix) -> T,
) -> Result<(), tonic::Status> {
    let event_reply_value = AppMatrixProvider
        .with_matrix(matrix_id, |matrix| create_reply(matrix).into())
        .map_err(Status::not_found)?;
    send_to_app(
        matrix_id,
        reply::Value::EventReply(EventReply {
            value: Some(event_reply_value),
        }),
    )
}

fn send_query_reply_to_app(
    matrix_id: String,
    id: u32,
    future: impl Future<Output = Result<query_result::Value, Status>> + 'static,
) {
    Global::future_support().spawn_in_main_thread_from_main_thread(async move {
        let value = reply::Value::QueryReply(QueryReply {
            id,
            result: Some(QueryResult {
                value: Some(future.await?),
            }),
        });
        send_to_app(&matrix_id, value)?;
        Ok(())
    });
}

fn send_to_app(session_id: &str, reply_value: reply::Value) -> Result<(), tonic::Status> {
    let main_panel = App::get()
        .find_main_panel_by_session_id(session_id)
        .ok_or(Status::not_found("instance not found"))?;
    let reply = Reply {
        value: Some(reply_value),
    };
    main_panel
        .send_to_app(&reply)
        .map_err(|e| Status::unknown(e.to_string()))?;
    Ok(())
}
