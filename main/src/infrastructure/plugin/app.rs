use crate::core::default_util::is_default;
use crate::infrastructure::data::{FileBasedControllerManager, SharedControllerManager};
use crate::infrastructure::server::{RealearnServer, SharedRealearnServer, COMPANION_WEB_APP_URL};
use once_cell::unsync::Lazy;
use reaper_high::{create_terminal_logger, Reaper};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use slog::{debug, o, Drain};
use std::cell::{Ref, RefCell};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use url::Url;

/// static mut maybe okay because we access this via `App::get()` function only and this one checks
/// the thread before returning the reference.
static mut APP: Lazy<App> = Lazy::new(App::load);

pub struct App {
    controller_manager: SharedControllerManager,
    server: SharedRealearnServer,
    config: RefCell<AppConfig>,
    changed_subject: RefCell<LocalSubject<'static, (), ()>>,
}

impl App {
    /// Panics if not in main thread.
    pub fn get() -> &'static App {
        Reaper::get().require_main_thread();
        unsafe { &APP }
    }

    fn load() -> App {
        let config = AppConfig::load().unwrap_or_else(|e| {
            debug!(App::logger(), "{}", e);
            Default::default()
        });
        App::new(config)
    }

    fn new(config: AppConfig) -> App {
        App {
            controller_manager: Rc::new(RefCell::new(FileBasedControllerManager::new(
                App::realearn_resource_dir_path().join("controllers"),
            ))),
            server: Rc::new(RefCell::new(RealearnServer::new(
                config.main.server_http_port,
                config.main.server_https_port,
                App::server_resource_dir_path().join("certificates"),
            ))),
            config: RefCell::new(config),
            changed_subject: Default::default(),
        }
    }

    pub fn init(&self) {
        if self.config.borrow().server_is_enabled() {
            self.server().borrow_mut().start();
        }
    }

    // We need this to be static because we need it at plugin construction time, so we don't have
    // REAPER API access yet. App needs REAPER API to be constructed (e.g. in order to
    // know where's the resource directory that contains the app configuration).
    // TODO-low In future it might be wise to turn to a different logger as soon as REAPER API
    //  available. Then we can also do file logging to ReaLearn resource folder.
    pub fn logger() -> &'static slog::Logger {
        static APP_LOGGER: once_cell::sync::Lazy<slog::Logger> = once_cell::sync::Lazy::new(|| {
            env_logger::init_from_env("REALEARN_LOG");
            slog::Logger::root(slog_stdlog::StdLog.fuse(), o!("app" => "ReaLearn"))
        });
        &APP_LOGGER
    }

    // TODO-medium Return a reference to a SharedControllerManager! Clients might just want to turn
    //  this into a weak one.
    pub fn controller_manager(&self) -> SharedControllerManager {
        self.controller_manager.clone()
    }

    pub fn server(&self) -> &SharedRealearnServer {
        &self.server
    }

    pub fn config(&self) -> Ref<AppConfig> {
        self.config.borrow()
    }

    pub fn start_server_persistently(&self) {
        self.server.borrow_mut().start();
        self.change_config(AppConfig::enable_server);
    }

    pub fn disable_server_persistently(&self) {
        self.change_config(AppConfig::disable_server);
    }

    pub fn enable_server_persistently(&self) {
        self.change_config(AppConfig::enable_server);
    }

    /// Logging debug info is always initiated by a particular session.
    pub fn log_debug_info(&self, session_id: &str) {
        crate::application::App::get().log_debug_info();
        self.server.borrow().log_debug_info(session_id);
        self.controller_manager.borrow().log_debug_info();
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
        let reaper_resource_path = Reaper::get().resource_path();
        reaper_resource_path.join("Helgoboss")
    }

    fn realearn_resource_dir_path() -> PathBuf {
        Self::helgoboss_resource_dir_path().join("ReaLearn")
    }

    fn server_resource_dir_path() -> PathBuf {
        Self::helgoboss_resource_dir_path().join("Server")
    }

    fn notify_changed(&self) {
        self.changed_subject.borrow_mut().next(());
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
