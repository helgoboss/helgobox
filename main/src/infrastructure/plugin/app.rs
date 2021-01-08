use crate::application::WeakSession;
use crate::core::default_util::is_default;
use crate::core::Global;
use crate::domain::{
    MainProcessor, RealTimeProcessor, RealearnAudioHook, RealearnAudioHookTask,
    RealearnControlSurfaceMainTask, RealearnControlSurfaceMiddleware,
    RealearnControlSurfaceServerTask, SharedRealTimeProcessor,
};
use crate::infrastructure::data::{
    FileBasedControllerPresetManager, FileBasedMainPresetManager, FileBasedPresetLinkManager,
    SharedControllerPresetManager, SharedMainPresetManager, SharedPresetLinkManager,
};
use crate::infrastructure::plugin::debug_util;
use crate::infrastructure::server;
use crate::infrastructure::server::{RealearnServer, SharedRealearnServer, COMPANION_WEB_APP_URL};
use reaper_high::{CrashInfo, Fx, MiddlewareControlSurface, Reaper};
use reaper_low::{PluginContext, Swell};
use reaper_medium::RegistrationHandle;
use reaper_rx::{ActionRxHookPostCommand, ActionRxHookPostCommand2};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use slog::{debug, o, Drain, Logger};
use std::cell::{Ref, RefCell};
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::rc::Rc;
use url::Url;

make_available_globally_in_main_thread!(App);

pub type RealearnControlSurface =
    MiddlewareControlSurface<RealearnControlSurfaceMiddleware<WeakSession>>;

pub type RealearnControlSurfaceMainTaskSender =
    crossbeam_channel::Sender<RealearnControlSurfaceMainTask<WeakSession>>;

pub type RealearnControlSurfaceServerTaskSender =
    crossbeam_channel::Sender<RealearnControlSurfaceServerTask>;

pub struct App {
    state: RefCell<AppState>,
    controller_manager: SharedControllerPresetManager,
    main_preset_manager: SharedMainPresetManager,
    preset_link_manager: SharedPresetLinkManager,
    server: SharedRealearnServer,
    config: RefCell<AppConfig>,
    changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    list_of_recently_focused_fx: Rc<RefCell<ListOfRecentlyFocusedFx>>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    control_surface_main_task_sender: RealearnControlSurfaceMainTaskSender,
    audio_hook_task_sender: crossbeam_channel::Sender<RealearnAudioHookTask>,
}

enum AppState {
    Uninitialized(UninitializedState),
    Initializing,
    Sleeping(SleepingState),
    WakingUp,
    Awake(AwakeState),
    GoingToSleep,
}

impl AppState {
    fn is_uninitialized(&self) -> bool {
        matches!(self, AppState::Uninitialized(_))
    }

    fn is_sleeping(&self) -> bool {
        matches!(self, AppState::Sleeping(_))
    }

    fn is_awake(&self) -> bool {
        matches!(self, AppState::Awake(_))
    }
}

struct UninitializedState {
    control_surface_main_task_receiver:
        crossbeam_channel::Receiver<RealearnControlSurfaceMainTask<WeakSession>>,
    control_surface_server_task_receiver:
        crossbeam_channel::Receiver<RealearnControlSurfaceServerTask>,
    audio_hook_task_receiver: crossbeam_channel::Receiver<RealearnAudioHookTask>,
}

struct SleepingState {
    control_surface: Box<RealearnControlSurface>,
    audio_hook: Box<RealearnAudioHook>,
}

struct AwakeState {
    control_surface_handle: RegistrationHandle<RealearnControlSurface>,
    audio_hook_handle: RegistrationHandle<RealearnAudioHook>,
}

impl Default for App {
    fn default() -> Self {
        // TODO-low Not so super cool to load from a file in the default function. However,
        //  that made it easier for our make_available_globally_in_main_thread!().
        let config = AppConfig::load().unwrap_or_else(|e| {
            debug!(crate::application::App::logger(), "{}", e);
            Default::default()
        });
        App::new(config)
    }
}

#[derive(Default)]
struct ListOfRecentlyFocusedFx {
    previous: Option<Fx>,
    current: Option<Fx>,
}

impl ListOfRecentlyFocusedFx {
    fn feed(&mut self, currently_focused_fx: Option<Fx>) {
        self.previous = self.current.take();
        self.current = currently_focused_fx;
    }
}

impl App {
    pub fn detailed_version_label() -> &'static str {
        static DETAILED_VERSION: once_cell::sync::Lazy<String> =
            once_cell::sync::Lazy::new(build_detailed_version);
        &DETAILED_VERSION
    }

    fn new(config: AppConfig) -> App {
        let (main_sender, main_receiver) = crossbeam_channel::unbounded();
        let (server_sender, server_receiver) = crossbeam_channel::unbounded();
        let (audio_sender, audio_receiver) = crossbeam_channel::unbounded();
        let uninitialized_state = UninitializedState {
            control_surface_main_task_receiver: main_receiver,
            control_surface_server_task_receiver: server_receiver,
            audio_hook_task_receiver: audio_receiver,
        };
        App {
            state: RefCell::new(AppState::Uninitialized(uninitialized_state)),
            controller_manager: Rc::new(RefCell::new(FileBasedControllerPresetManager::new(
                App::realearn_preset_dir_path().join("controller"),
            ))),
            main_preset_manager: Rc::new(RefCell::new(FileBasedMainPresetManager::new(
                App::realearn_preset_dir_path().join("main"),
            ))),
            preset_link_manager: Rc::new(RefCell::new(FileBasedPresetLinkManager::new(
                App::realearn_auto_load_configs_dir_path(),
            ))),
            server: Rc::new(RefCell::new(RealearnServer::new(
                config.main.server_http_port,
                config.main.server_https_port,
                App::server_resource_dir_path().join("certificates"),
                server_sender,
            ))),
            config: RefCell::new(config),
            changed_subject: Default::default(),
            list_of_recently_focused_fx: Default::default(),
            party_is_over_subject: Default::default(),
            control_surface_main_task_sender: main_sender,
            audio_hook_task_sender: audio_sender,
        }
    }

    /// Executed globally just once when module loaded.
    pub fn init_static(logger: Logger, context: PluginContext) {
        Swell::make_available_globally(Swell::load(context));
        Reaper::setup_with_defaults(
            context,
            logger,
            CrashInfo {
                plugin_name: "ReaLearn".to_string(),
                plugin_version: App::detailed_version_label().to_string(),
                support_email_address: "info@helgoboss.org".to_string(),
            },
        );
        App::get().init();
    }

    /// Executed globally just once as soon as we have access to global REAPER instance.
    pub fn init(&self) {
        let prev_state = self.state.replace(AppState::Initializing);
        let uninit_state = if let AppState::Uninitialized(s) = prev_state {
            s
        } else {
            panic!("App was not uninitialized anymore");
        };
        crate::application::App::get().register_global_learn_action();
        server::keep_informing_clients_about_sessions();
        debug_util::register_resolve_symbols_action();
        crate::infrastructure::test::register_test_action();
        let list_of_recently_focused_fx = self.list_of_recently_focused_fx.clone();
        Global::control_surface_rx()
            .fx_focused()
            .take_until(self.party_is_over())
            .subscribe(move |fx| {
                list_of_recently_focused_fx.borrow_mut().feed(fx);
            });
        let control_surface = MiddlewareControlSurface::new(RealearnControlSurfaceMiddleware::new(
            &crate::application::App::logger(),
            uninit_state.control_surface_main_task_receiver,
            uninit_state.control_surface_server_task_receiver,
            std::env::var("REALEARN_METER").is_ok(),
        ));
        let audio_hook = RealearnAudioHook::new(uninit_state.audio_hook_task_receiver);
        let sleeping_state = SleepingState {
            control_surface: Box::new(control_surface),
            audio_hook: Box::new(audio_hook),
        };
        self.state.replace(AppState::Sleeping(sleeping_state));
    }

    // Executed whenever the first ReaLearn instance is loaded.
    pub fn wake_up(&self) {
        let prev_state = self.state.replace(AppState::WakingUp);
        let sleeping_state = if let AppState::Sleeping(s) = prev_state {
            s
        } else {
            panic!("App was not sleeping");
        };
        if self.config.borrow().server_is_enabled() {
            self.server()
                .borrow_mut()
                .start()
                .unwrap_or_else(warn_about_failed_server_start);
        }
        let mut session = Reaper::get().medium_session();
        // Action hooks
        session
            .plugin_register_add_hook_post_command::<ActionRxHookPostCommand<Global>>()
            .unwrap();
        // This fails before REAPER 6.20 and therefore we don't have MIDI CC action feedback.
        let _ =
            session.plugin_register_add_hook_post_command_2::<ActionRxHookPostCommand2<Global>>();
        // Audio hook and control surface
        debug!(
            crate::application::App::logger(),
            "Registering ReaLearn audio hook and control surface..."
        );
        let audio_hook_handle = session
            .audio_reg_hardware_hook_add(sleeping_state.audio_hook)
            .expect("couldn't register ReaLearn audio hook");
        sleeping_state.control_surface.middleware().reset();
        let control_surface_handle = session
            .plugin_register_add_csurf_inst(sleeping_state.control_surface)
            .expect("couldn't register ReaLearn control surface");
        let awake_state = AwakeState {
            control_surface_handle,
            audio_hook_handle,
        };
        self.state.replace(AppState::Awake(awake_state));
    }

    pub fn register_processor_couple(
        &self,
        real_time_processor: SharedRealTimeProcessor,
        main_processor: MainProcessor<WeakSession>,
    ) {
        self.audio_hook_task_sender
            .send(RealearnAudioHookTask::AddRealTimeProcessor(
                real_time_processor,
            ))
            .unwrap();
        self.control_surface_main_task_sender
            .send(RealearnControlSurfaceMainTask::AddMainProcessor(
                main_processor,
            ))
            .unwrap();
    }

    pub fn unregister_processor_couple(&self, instance_id: String) {
        self.control_surface_main_task_sender
            .send(RealearnControlSurfaceMainTask::RemoveMainProcessor(
                instance_id.clone(),
            ))
            .unwrap();
        self.audio_hook_task_sender
            .send(RealearnAudioHookTask::RemoveRealTimeProcessor(instance_id))
            .unwrap();
    }

    // Executed whenever the last ReaLearn instance goes away.
    pub fn go_to_sleep(&self) {
        let prev_state = self.state.replace(AppState::GoingToSleep);
        let awake_state = if let AppState::Awake(s) = prev_state {
            s
        } else {
            panic!("App was not awake");
        };
        let mut session = Reaper::get().medium_session();
        debug!(
            crate::application::App::logger(),
            "Unregistering ReaLearn control surface and audio hook..."
        );
        let (control_surface, audio_hook) = unsafe {
            let control_surface = session
                .plugin_register_remove_csurf_inst(awake_state.control_surface_handle)
                .expect("control surface was not registered");
            let audio_hook = session
                .audio_reg_hardware_hook_remove(awake_state.audio_hook_handle)
                .expect("control surface was not registered");
            (control_surface, audio_hook)
        };
        // Actions
        session.plugin_register_remove_hook_post_command_2::<ActionRxHookPostCommand2<Global>>();
        session.plugin_register_remove_hook_post_command::<ActionRxHookPostCommand<Global>>();
        // Server
        self.server().borrow_mut().stop();
        let sleeping_state = SleepingState {
            control_surface,
            audio_hook,
        };
        self.state.replace(AppState::Sleeping(sleeping_state));
    }

    /// The special thing about this is that this doesn't return the currently focused FX but the
    /// last focused one. That's important because when queried from ReaLearn UI, the current one
    /// is mostly ReaLearn itself - which is in most cases not what we want.
    pub fn previously_focused_fx(&self) -> Option<Fx> {
        self.list_of_recently_focused_fx.borrow().previous.clone()
    }

    // TODO-medium Return a reference to a SharedControllerManager! Clients might just want to turn
    //  this into a weak one.
    pub fn controller_manager(&self) -> SharedControllerPresetManager {
        self.controller_manager.clone()
    }

    pub fn main_preset_manager(&self) -> SharedMainPresetManager {
        self.main_preset_manager.clone()
    }

    pub fn preset_link_manager(&self) -> SharedPresetLinkManager {
        self.preset_link_manager.clone()
    }

    pub fn server(&self) -> &SharedRealearnServer {
        &self.server
    }

    pub fn config(&self) -> Ref<AppConfig> {
        self.config.borrow()
    }

    pub fn start_server_persistently(&self) -> Result<(), String> {
        self.server.borrow_mut().start()?;
        self.change_config(AppConfig::enable_server);
        Ok(())
    }

    pub fn disable_server_persistently(&self) {
        self.change_config(AppConfig::disable_server);
    }

    pub fn enable_server_persistently(&self) {
        self.change_config(AppConfig::enable_server);
    }

    /// Logging debug info is always initiated by a particular session.
    pub fn log_debug_info(&self, session_id: &str) {
        self.server.borrow().log_debug_info(session_id);
        self.controller_manager.borrow().log_debug_info();
        self.control_surface_main_task_sender
            .send(RealearnControlSurfaceMainTask::LogDebugInfo)
            .unwrap();
        // Must be the last because it (intentionally) panics
        crate::application::App::get().log_debug_info();
    }

    pub fn changed(&self) -> impl UnitEvent {
        self.changed_subject.borrow().clone()
    }

    fn change_config(&self, op: impl FnOnce(&mut AppConfig)) {
        let mut config = self.config.borrow_mut();
        op(&mut config);
        config.save().unwrap();
        self.notify_changed();
    }

    fn helgoboss_resource_dir_path() -> PathBuf {
        Reaper::get().resource_path().join("Helgoboss")
    }

    fn realearn_resource_dir_path() -> PathBuf {
        App::helgoboss_resource_dir_path().join("ReaLearn")
    }

    pub fn realearn_data_dir_path() -> PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/realearn")
    }

    pub fn realearn_preset_dir_path() -> PathBuf {
        Self::realearn_data_dir_path().join("presets")
    }

    pub fn realearn_auto_load_configs_dir_path() -> PathBuf {
        Self::realearn_data_dir_path().join("auto-load-configs")
    }

    fn server_resource_dir_path() -> PathBuf {
        Self::helgoboss_resource_dir_path().join("Server")
    }

    fn notify_changed(&self) {
        self.changed_subject.borrow_mut().next(());
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.party_is_over_subject.next(());
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    main: MainConfig,
}

impl AppConfig {
    pub fn load() -> Result<AppConfig, String> {
        let ini_content = fs::read_to_string(&Self::config_file_path())
            .map_err(|_| "couldn't read config file".to_string())?;
        let config = serde_ini::from_str(&ini_content).map_err(|e| format!("{:?}", e))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let ini_content = serde_ini::to_string(self).map_err(|_| "couldn't serialize config")?;
        let config_file_path = Self::config_file_path();
        fs::create_dir_all(&config_file_path.parent().unwrap())
            .expect("couldn't create config directory");
        fs::write(config_file_path, ini_content).map_err(|_| "couldn't write config file")?;
        Ok(())
    }

    pub fn enable_server(&mut self) {
        self.main.server_enabled = 1;
    }

    pub fn disable_server(&mut self) {
        self.main.server_enabled = 0;
    }

    pub fn server_is_enabled(&self) -> bool {
        self.main.server_enabled > 0
    }

    pub fn companion_web_app_url(&self) -> url::Url {
        Url::parse(&self.main.companion_web_app_url).expect("invalid companion web app URL")
    }

    fn config_file_path() -> PathBuf {
        App::realearn_resource_dir_path().join("realearn.ini")
    }
}

#[derive(Serialize, Deserialize)]
struct MainConfig {
    #[serde(default, skip_serializing_if = "is_default")]
    server_enabled: u8,
    #[serde(
        default = "default_server_http_port",
        skip_serializing_if = "is_default_server_http_port"
    )]
    server_http_port: u16,
    #[serde(
        default = "default_server_https_port",
        skip_serializing_if = "is_default_server_https_port"
    )]
    server_https_port: u16,
    #[serde(
        default = "default_companion_web_app_url",
        skip_serializing_if = "is_default_companion_web_app_url"
    )]
    companion_web_app_url: String,
}

const DEFAULT_SERVER_HTTP_PORT: u16 = 39080;
const DEFAULT_SERVER_HTTPS_PORT: u16 = 39443;

fn default_server_http_port() -> u16 {
    DEFAULT_SERVER_HTTP_PORT
}

fn is_default_server_http_port(v: &u16) -> bool {
    *v == DEFAULT_SERVER_HTTP_PORT
}

fn default_server_https_port() -> u16 {
    DEFAULT_SERVER_HTTPS_PORT
}

fn is_default_server_https_port(v: &u16) -> bool {
    *v == DEFAULT_SERVER_HTTPS_PORT
}

fn default_companion_web_app_url() -> String {
    COMPANION_WEB_APP_URL.to_string()
}

fn is_default_companion_web_app_url(v: &str) -> bool {
    v == COMPANION_WEB_APP_URL
}

impl Default for MainConfig {
    fn default() -> Self {
        MainConfig {
            server_enabled: Default::default(),
            server_http_port: default_server_http_port(),
            server_https_port: default_server_https_port(),
            companion_web_app_url: default_companion_web_app_url(),
        }
    }
}

fn build_detailed_version() -> String {
    use crate::infrastructure::plugin::built_info::*;
    let dirty_mark = if GIT_DIRTY.contains(&true) {
        "-dirty"
    } else {
        ""
    };
    let date_info = if let Ok(d) = chrono::DateTime::parse_from_rfc2822(BUILT_TIME_UTC) {
        d.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        BUILT_TIME_UTC.to_string()
    };
    let debug_mark = if PROFILE == "debug" { "-debug" } else { "" };
    format!(
        "v{}/{}{} rev {}{} ({})",
        PKG_VERSION,
        CFG_TARGET_ARCH,
        debug_mark,
        GIT_COMMIT_HASH
            .map(|h| h[0..6].to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        dirty_mark,
        date_info
    )
}

pub fn warn_about_failed_server_start(info: String) {
    Reaper::get().show_console_msg(format!(
        "Couldn't start ReaLearn projection server because {}",
        info
    ))
}
