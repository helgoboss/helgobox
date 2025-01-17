use crate::infrastructure::plugin::{reaper_main_window, BackboneShell};
use crate::infrastructure::proto;
use crate::infrastructure::proto::{
    create_initial_global_updates, create_initial_instance_updates, create_initial_unit_updates,
    event_reply, query_result, reply, request, EventReply, ProtoRequestHandler, QueryReply,
    QueryResult, Reply, Request,
};
use crate::infrastructure::ui::{AppCallback, SharedAppInstance};
use anyhow::{anyhow, bail, Context, Result};
use base::Global;
use libloading::{Library, Symbol};

use crate::domain::InstanceId;
#[cfg(feature = "playtime")]
use playtime_clip_engine::base::Matrix;
use prost::Message;
use reaper_high::Reaper;
use reaper_low::raw::HWND;
use reaper_medium::Hwnd;
use semver::Version;
use std::env;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::ptr::{null_mut, NonNull};
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
        let (main_library, dependencies) = if cfg!(target_os = "windows") {
            (
                "helgobox.dll",
                [
                    // Important: This must be the first. Because below plug-in libraries
                    // depend on it.
                    "flutter_windows.dll",
                    // The rest can have an arbitrary order.
                    "desktop_drop_plugin.dll",
                    "native_context_menu_plugin.dll",
                    "screen_retriever_plugin.dll",
                    "url_launcher_windows_plugin.dll",
                    "window_manager_plugin.dll",
                    "pointer_lock_plugin.dll",
                ]
                .as_slice(),
            )
        } else if cfg!(target_os = "macos") {
            (
                "Contents/MacOS/helgobox",
                [
                    // Important: This must be the first. Because below plug-in libraries
                    // depend on it.
                    "Contents/Frameworks/FlutterMacOS.framework/FlutterMacOS",
                    // The rest can have an arbitrary order.
                    "Contents/Frameworks/cryptography_flutter.framework/cryptography_flutter",
                    "Contents/Frameworks/device_info_plus.framework/device_info_plus",
                    "Contents/Frameworks/desktop_drop.framework/desktop_drop",
                    "Contents/Frameworks/native_context_menu.framework/native_context_menu",
                    "Contents/Frameworks/path_provider_foundation.framework/path_provider_foundation",
                    "Contents/Frameworks/screen_retriever.framework/screen_retriever",
                    "Contents/Frameworks/url_launcher_macos.framework/url_launcher_macos",
                    "Contents/Frameworks/window_manager.framework/window_manager",
                    "Contents/Frameworks/pointer_lock.framework/pointer_lock",
                ].as_slice(),
            )
        } else if cfg!(target_os = "linux") {
            (
                "helgobox.so",
                ["flutter_linux.so", "url_launcher_linux_plugin.so"].as_slice(),
            )
        } else {
            bail!("OS not supported");
        };
        let loaded_dependencies: Result<Vec<Library>> = dependencies
            .iter()
            .map(|dep| load_library(&app_base_dir.join(dep)))
            .collect();
        let library = AppLibrary {
            main_library: load_library(&app_base_dir.join(main_library))?,
            app_base_dir,
            _dependencies: loaded_dependencies?,
        };
        library.verify_version_compatibility()?;
        Ok(library)
    }

    fn verify_version_compatibility(&self) -> Result<()> {
        let version = self.get_app_api_version()?;
        if version < MIN_APP_API_VERSION || version.major > MIN_APP_API_VERSION.major {
            bail!("App API version doesn't match. Expected version: {MIN_APP_API_VERSION}. Actual version: {version}.");
        }
        Ok(())
    }

    fn get_app_api_version(&self) -> Result<Version> {
        let mut buf = [0; 32];
        let version_str = unsafe {
            let get_app_version: Symbol<GetAppApiVersion> = self
                .main_library
                .get(b"get_app_api_version\0")
                .map_err(|_| anyhow!("Failed to load get_api_version function"))?;
            get_app_version(buf.as_mut_ptr() as *mut c_char, buf.len());
            CStr::from_bytes_until_nul(&buf)?.to_str()?
        };
        let version = Version::parse(version_str)?;
        Ok(version)
    }

    pub fn start_app_instance(
        &self,
        parent_window: Option<Window>,
        instance_id: InstanceId,
        location: String,
    ) -> Result<AppHandle> {
        let app_base_dir_str = self
            .app_base_dir
            .to_str()
            .ok_or(anyhow!("app base dir is not an UTF-8 string"))?;
        let app_base_dir_c_string = CString::new(app_base_dir_str)
            .map_err(|_| anyhow!("app base dir contains a nul byte"))?;
        let location_c_string =
            CString::new(location).map_err(|_| anyhow!("location contains a nul byte"))?;
        with_temporarily_changed_working_directory(&self.app_base_dir, || {
            prepare_app_start();
            let app_handle = unsafe {
                let start_app_instance: Symbol<StartAppInstance> = self
                    .main_library
                    .get(b"start_app_instance\0")
                    .map_err(|_| anyhow!("failed to load start_app_instance function"))?;
                start_app_instance(
                    parent_window.map(|w| w.raw()).unwrap_or(null_mut()),
                    app_base_dir_c_string.as_ptr(),
                    invoke_host,
                    instance_id.into(),
                    location_c_string.as_ptr(),
                    Reaper::get().main_window().as_ptr(),
                )
            };
            let Some(app_handle) = app_handle else {
                bail!("couldn't start app");
            };
            Ok(app_handle)
        })
    }

    pub fn show_app_instance(
        &self,
        parent_window: Option<Window>,
        app_handle: AppHandle,
    ) -> Result<()> {
        unsafe {
            let show_app_instance: Symbol<ShowAppInstance> = self
                .main_library
                .get(b"show_app_instance\0")
                .map_err(|_| anyhow!("failed to load show_app_instance function"))?;
            show_app_instance(
                parent_window.map(|w| w.raw()).unwrap_or(null_mut()),
                app_handle,
            );
        };
        Ok(())
    }

    pub fn hide_app_instance(&self, app_handle: AppHandle) -> Result<()> {
        unsafe {
            let hide_app_instance: Symbol<HideAppInstance> = self
                .main_library
                .get(b"hide_app_instance\0")
                .map_err(|_| anyhow!("failed to load hide_app_instance function"))?;
            hide_app_instance(app_handle, reaper_main_window().raw());
        };
        Ok(())
    }

    pub fn app_instance_is_visible(&self, app_handle: AppHandle) -> Result<bool> {
        let visible = unsafe {
            let app_instance_is_visible: Symbol<AppInstanceIsVisible> = self
                .main_library
                .get(b"app_instance_is_visible\0")
                .map_err(|_| anyhow!("failed to load app_instance_is_visible function"))?;
            app_instance_is_visible(app_handle)
        };
        Ok(visible)
    }

    pub fn app_instance_get_window(&self, app_handle: AppHandle) -> Result<Option<Hwnd>> {
        let hwnd = unsafe {
            let get_app_instance_window: Symbol<GetAppInstanceWindow> = self
                .main_library
                .get(b"get_app_instance_window\0")
                .map_err(|_| anyhow!("failed to load get_app_instance_window function"))?;
            get_app_instance_window(app_handle)
        };
        Ok(Hwnd::new(hwnd))
    }

    pub fn app_instance_has_focus(&self, app_handle: AppHandle) -> Result<bool> {
        let visible = unsafe {
            let app_instance_has_focus: Symbol<AppInstanceHasFocus> = self
                .main_library
                .get(b"app_instance_has_focus\0")
                .map_err(|_| anyhow!("failed to load app_instance_has_focus function"))?;
            app_instance_has_focus(app_handle)
        };
        Ok(visible)
    }

    pub fn stop_app_instance(
        &self,
        parent_window: Option<Window>,
        app_handle: AppHandle,
    ) -> Result<()> {
        unsafe {
            let stop_app_instance: Symbol<StopAppInstance> = self
                .main_library
                .get(b"stop_app_instance\0")
                .map_err(|_| anyhow!("failed to load stop_app_instance function"))?;
            stop_app_instance(
                parent_window.map(|w| w.raw()).unwrap_or(null_mut()),
                app_handle,
            );
        };
        Ok(())
    }
}

/// Function that's used from Dart in order to call the host.
///
/// Attention: This is *not* called from the main thread but from some special Flutter UI thread.
#[no_mangle]
extern "C" fn invoke_host(data: *const u8, length: i32) {
    // Decode payload
    let bytes = unsafe { std::slice::from_raw_parts(data, length as usize) };
    let request = Request::decode(bytes).unwrap();
    // Extract values
    let Some(request_value) = request.value else {
        tracing::error!(msg = "incoming app request didn't have value");
        return;
    };
    // Process request
    if let Err(error) = process_request(request.instance_id.into(), request_value) {
        tracing::error!(msg = "error in synchronous phase of request processing", %error);
    }
}

/// Processes the given request.
///
/// Essentially this only extracts some values and then schedules the actual work on the main
/// thread.
///
/// # Errors
///
/// Returns an error if something in the synchronous part of the request processing went wrong.
fn process_request(instance_id: InstanceId, request_value: request::Value) -> Result<()> {
    use proto::request::Value;
    match request_value {
        // It's a command (fire-and-forget)
        Value::CommandRequest(command_request) => {
            let command_request_value = command_request
                .value
                .context("incoming app command request didn't have value")?;
            process_command_request(instance_id, command_request_value)
                .context("processing command request")?;
            Ok(())
        }
        // It's a query (with async response)
        Value::QueryRequest(query_request) => {
            let query_request_value = query_request
                .query
                .context("incoming app query request didn't have query")?
                .value
                .context("incoming app query didn't have value")?;
            process_query_request(instance_id, query_request.id, query_request_value)
                .context("processing query request")?;
            Ok(())
        }
    }
}

pub type AppHandle = NonNull<c_void>;

/// Signature of the function that we use to query the version of the app API.
///
/// This is not the official version off the app, just the semantic version that affects how
/// the host can talk to the app. The general idea is:
///
/// 1. Host (ReaLearn) queries this app API version number (before doing anything else).
/// 2. Host checks if that version number matches his expectations (semantic versioning semantics).
/// 3. Host refuses to load the app if the expectations are not matched.
type GetAppApiVersion = unsafe extern "C" fn(version: *mut c_char, buf_size: usize);

/// Signature of the function that we use to start an app instance and show it for the first time.
///
/// # Arguments
///
/// * `parent_window` - Optional parent window handle. If you pass this, the app (if supported for
///   the OS) will render itself *within* that parent window. On macOS, this is should be an NSView.
/// * `app_base_dir_utf8_c_str`- Directory where the app is located
/// * `host_callback` - Pointer to host callback function
/// * `instance_id` - Instance ID of the ReaLearn instance associated with this new app instance.
/// * `location` - Initial location (route) within the app.
/// * `main_window` - Handle to REAPER's main window
type StartAppInstance = unsafe extern "C" fn(
    parent_window: HWND,
    app_base_dir_utf8_c_str: *const c_char,
    host_callback: HostCallback,
    instance_id: c_uint,
    location: *const c_char,
    main_window: HWND,
) -> Option<AppHandle>;

/// Signature of the function that we use to show an app instance.
type ShowAppInstance = unsafe extern "C" fn(parent_window: HWND, app_handle: AppHandle);

/// Signature of the function that we use to hide an app instance.
type HideAppInstance = unsafe extern "C" fn(app_handle: AppHandle, host_window: HWND);

/// Signature of the function that we use to check whether an app instance has focus.
type AppInstanceHasFocus = unsafe extern "C" fn(app_handle: AppHandle) -> bool;

/// Signature of the function that we use to check whether an app instance is visible.
type AppInstanceIsVisible = unsafe extern "C" fn(app_handle: AppHandle) -> bool;

/// Signature of the function that we use to acquire the app window.
type GetAppInstanceWindow = unsafe extern "C" fn(app_handle: AppHandle) -> HWND;

/// Signature of the function that we use to stop an app instance.
type StopAppInstance = unsafe extern "C" fn(parent_window: HWND, app_handle: AppHandle);

/// Signature of the function that's used from the app in order to call the host.
type HostCallback = extern "C" fn(data: *const u8, length: i32);

fn load_library(path: &Path) -> Result<Library> {
    match path.try_exists() {
        Ok(false) => bail!("App library {path:?} not found."),
        Err(e) => bail!("App library {path:?} not accessible: {e}"),
        _ => {}
    }
    let lib = unsafe { Library::new(path) };
    lib.with_context(|| format!("Failed to load app library {path:?}."))
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

fn prepare_app_start() {
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

/// Executes the given command asynchronously (in the main thread).
///
/// # Errors
///
/// Returns an error if the main thread task queue is full.
fn process_command_request(
    instance_id: InstanceId,
    value: proto::command_request::Value,
) -> Result<()> {
    // We need to execute commands on the main thread!
    Global::task_support().do_in_main_thread_asap(move || {
        // Execute command
        let result = process_command(instance_id, value);
        // Handle possible error
        if let Err(status) = result {
            // Log error
            tracing::error!(msg = "error in asynchronous phase of command request processing", %status);
            // Send it to the app as notification
            let _ = send_to_app(
                instance_id,
                reply::Value::EventReply(EventReply {
                    value: Some(event_reply::Value::ErrorMessage(status.message().to_string())),
                }),
            );
        }
    }).map_err(|e| anyhow!(e))?;
    Ok(())
}

fn process_query_request(
    instance_id: InstanceId,
    req_id: u32,
    query: proto::query::Value,
) -> Result<()> {
    use proto::query::Value::*;
    let handler = ProtoRequestHandler;
    match query {
        ProveAuthenticity(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.prove_authenticity(req).await?.into_inner();
                Ok(query_result::Value::ProveAuthenticityReply(value))
            });
        }
        GetClipDetail(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_clip_detail(req).await?.into_inner();
                Ok(query_result::Value::GetClipDetailReply(value))
            });
        }
        GetProjectDir(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_project_dir(req).await?.into_inner();
                Ok(query_result::Value::GetProjectDirReply(value))
            });
        }
        GetHostInfo(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_host_info(req).await?.into_inner();
                Ok(query_result::Value::GetHostInfoReply(value))
            });
        }
        GetArrangementInfo(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_arrangement_info(req).await?.into_inner();
                Ok(query_result::Value::GetArrangementInfoReply(value))
            });
        }
        GetAppSettings(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_app_settings(req).await?.into_inner();
                Ok(query_result::Value::GetAppSettingsReply(value))
            });
        }
        GetCompartmentData(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_compartment_data(req)?.into_inner();
                Ok(query_result::Value::GetCompartmentDataReply(value))
            });
        }
        GetCustomInstanceData(req) => {
            send_query_reply_to_app(instance_id, req_id, async move {
                let value = handler.get_custom_instance_data(req)?.into_inner();
                Ok(query_result::Value::GetCustomInstanceDataReply(value))
            });
        }
    }
    Ok(())
}

fn process_command(
    instance_id: InstanceId,
    req: proto::command_request::Value,
) -> std::result::Result<(), Status> {
    let handler = ProtoRequestHandler;
    use proto::command_request::Value::*;
    match req {
        // Embedding
        NotifyAppIsReady(req) => {
            // App instance is started. Put the app instance callback at the correct position.
            let ptr = req.app_callback_address as *const ();
            let app_callback: AppCallback = unsafe { std::mem::transmute(ptr) };
            find_app_instance(req.matrix_id.into())
                .map_err(to_status)?
                .borrow_mut()
                .notify_app_is_ready(app_callback);
        }
        SetAppIsInTextEntryMode(req) => {
            find_app_instance(req.matrix_id.into())
                .map_err(to_status)?
                .borrow_mut()
                .notify_app_is_in_text_entry_mode(req.is_in_text_entry_mode);
        }
        // Event subscription commands
        GetOccasionalGlobalUpdates(_) => {
            send_initial_events_to_app(instance_id, create_initial_global_updates)
                .map_err(to_status)?;
        }
        GetOccasionalInstanceUpdates(req) => {
            send_initial_events_to_app(instance_id, || {
                let instance_shell = BackboneShell::get()
                    .find_instance_shell_by_instance_id(req.instance_id.into())
                    .unwrap();
                create_initial_instance_updates(&instance_shell)
            })
            .map_err(to_status)?;
        }
        GetOccasionalUnitUpdates(req) => {
            send_initial_events_to_app(instance_id, || {
                let instance_shell = BackboneShell::get()
                    .find_instance_shell_by_instance_id(req.instance_id.into())
                    .unwrap();
                create_initial_unit_updates(&instance_shell)
            })
            .map_err(to_status)?;
        }
        GetOccasionalPlaytimeEngineUpdates(_) => {
            #[cfg(not(feature = "playtime"))]
            {
                return playtime_not_available();
            }
            #[cfg(feature = "playtime")]
            {
                send_initial_events_to_app(
                    instance_id,
                    crate::infrastructure::proto::create_initial_engine_updates,
                )
                .map_err(to_status)?;
            }
        }
        GetOccasionalMatrixUpdates(req) => {
            #[cfg(not(feature = "playtime"))]
            {
                let _ = req;
                return playtime_not_available();
            }
            #[cfg(feature = "playtime")]
            {
                send_initial_matrix_events_to_app(
                    instance_id,
                    req.matrix_id.into(),
                    proto::create_initial_matrix_updates,
                )
                .map_err(to_status)?;
            }
        }
        GetOccasionalTrackUpdates(req) => {
            #[cfg(not(feature = "playtime"))]
            {
                let _ = req;
                return playtime_not_available();
            }
            #[cfg(feature = "playtime")]
            {
                send_initial_matrix_events_to_app(
                    instance_id,
                    req.matrix_id.into(),
                    proto::create_initial_track_updates,
                )
                .map_err(to_status)?;
            }
        }
        GetOccasionalSlotUpdates(req) => {
            #[cfg(not(feature = "playtime"))]
            {
                let _ = req;
                return playtime_not_available();
            }
            #[cfg(feature = "playtime")]
            {
                send_initial_matrix_events_to_app(
                    instance_id,
                    req.matrix_id.into(),
                    proto::create_initial_slot_updates,
                )
                .map_err(to_status)?;
            }
        }
        GetOccasionalClipUpdates(req) => {
            #[cfg(not(feature = "playtime"))]
            {
                let _ = req;
                return playtime_not_available();
            }
            #[cfg(feature = "playtime")]
            {
                send_initial_matrix_events_to_app(
                    instance_id,
                    req.matrix_id.into(),
                    proto::create_initial_clip_updates,
                )
                .map_err(to_status)?;
            }
        }
        // Normal commands
        AddLicense(req) => {
            handler.add_license(req)?;
        }
        SaveController(req) => {
            handler.save_controller(req)?;
        }
        DeleteController(req) => {
            handler.delete_controller(req)?;
        }
        TriggerMatrix(req) => {
            handler.trigger_matrix(req)?;
        }
        TriggerInstance(req) => {
            handler.trigger_instance(req)?;
        }
        TriggerGlobal(req) => {
            handler.trigger_global(req)?;
        }
        SetPlaytimeEngineSettings(req) => {
            handler.set_playtime_engine_settings(req)?;
        }
        SetMatrixSettings(req) => {
            handler.set_matrix_settings(req)?;
        }
        SetMatrixTempo(req) => {
            handler.set_matrix_tempo(req)?;
        }
        SetMatrixPlayRate(req) => {
            handler.set_matrix_play_rate(req)?;
        }
        SetMatrixTimeSignature(req) => {
            handler.set_matrix_time_signature(req)?;
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
        TriggerTrack(req) => {
            handler.trigger_track(req)?;
        }
        SetColumnSettings(req) => {
            handler.set_column_settings(req)?;
        }
        SetTrackVolume(req) => {
            handler.set_track_volume(req)?;
        }
        SetTrackPan(req) => {
            handler.set_track_pan(req)?;
        }
        OpenTrackFx(req) => {
            handler.open_track_fx(req)?;
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
        SetTrackColor(req) => {
            handler.set_track_color(req)?;
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
        ImportFiles(req) => {
            handler.import_files(req)?;
        }
        DragSlot(req) => {
            handler.drag_slot(req)?;
        }
        DragClip(req) => {
            handler.drag_clip(req)?;
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
        TriggerSequence(req) => {
            handler.trigger_sequence(req)?;
        }
        SetSequenceInfo(req) => {
            handler.set_sequence_info(req)?;
        }
        SetInstanceSettings(req) => {
            handler.set_instance_settings(req)?;
        }
        SetAppSettings(req) => {
            handler.set_app_settings(req)?;
        }
        SaveCustomCompartmentData(req) => {
            handler.save_custom_compartment_data(req)?;
        }
        InsertColumns(req) => {
            handler.insert_columns(req)?;
        }
        SetCustomInstanceData(req) => {
            handler.set_custom_instance_data(req)?;
        }
    }
    Ok(())
}

fn send_initial_events_to_app<T: Into<event_reply::Value>>(
    instance_id: InstanceId,
    create_reply: impl FnOnce() -> T + Copy,
) -> Result<()> {
    let reply = create_reply().into();
    send_event_reply_to_app(instance_id, reply)
}

/// The matrix ID should actually always be the same as the instance ID. We use different
/// parameters because one is for identifying the matrix and the other one the destination app
/// instance. In practice, there's a one-to-one relationship between
/// Helgobox instance <=> Matrix instance <=> App instance.
#[cfg(feature = "playtime")]
fn send_initial_matrix_events_to_app<T: Into<event_reply::Value>>(
    instance_id: InstanceId,
    matrix_id: InstanceId,
    create_reply: impl FnOnce(Option<&Matrix>) -> T + Copy,
) -> Result<()> {
    let reply = BackboneShell::get()
        .with_clip_matrix(matrix_id, |matrix| create_reply(Some(matrix)).into())
        .unwrap_or_else(|_| create_reply(None).into());
    send_event_reply_to_app(instance_id, reply)
}

fn send_event_reply_to_app(instance_id: InstanceId, value: event_reply::Value) -> Result<()> {
    send_to_app(
        instance_id,
        reply::Value::EventReply(EventReply { value: Some(value) }),
    )
}

fn send_query_reply_to_app(
    instance_id: InstanceId,
    req_id: u32,
    future: impl Future<Output = Result<query_result::Value, Status>> + Send + 'static,
) {
    Global::future_support().spawn_in_main_thread(async move {
        let query_result_value = match future.await {
            Ok(outcome) => outcome,
            Err(error) => query_result::Value::Error(error.to_string()),
        };
        let reply_value = reply::Value::QueryReply(QueryReply {
            id: req_id,
            result: Some(QueryResult {
                value: Some(query_result_value),
            }),
        });
        send_to_app(instance_id, reply_value)?;
        Ok(())
    });
}

fn send_to_app(instance_id: InstanceId, reply_value: reply::Value) -> Result<()> {
    let app_instance = find_app_instance(instance_id)?;
    let reply = Reply {
        value: Some(reply_value),
    };
    app_instance.borrow().send(&reply)?;
    Ok(())
}

fn find_app_instance(instance_id: InstanceId) -> Result<SharedAppInstance> {
    let instance_panel = BackboneShell::get()
        .find_instance_panel_by_instance_id(instance_id)
        .ok_or_else(|| anyhow!("Helgobox instance {instance_id} not found"))?;
    Ok(instance_panel.app_instance().clone())
}

fn to_status(err: anyhow::Error) -> Status {
    Status::unknown(err.to_string())
}

/// The minimum version of the app API that the host (Helgobox) requires to properly
/// communicate with it. Keep this up to date!
///
/// This doesn't necessarily need to match the `HOST_API_VERSION`, it's too different things.
/// In practice, it might be equal or similar because host (plug-in) and app are developed tightly together.
pub const MIN_APP_API_VERSION: Version = Version::new(15, 0, 0);

#[cfg(not(feature = "playtime"))]
fn playtime_not_available() -> Result<(), Status> {
    Err(Status::not_found("Playtime feature not available"))
}
