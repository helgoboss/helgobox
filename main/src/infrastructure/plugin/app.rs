use crate::core::default_util::is_default;
use crate::infrastructure::data::{FileBasedControllerManager, SharedControllerManager};
use crate::infrastructure::server::{
    RealearnServer, ServerClients, SharedRealearnServer, COMPANION_WEB_APP_URL,
};
use once_cell::unsync::Lazy;
use reaper_high::{create_terminal_logger, Reaper};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use slog::{debug, o};
use std::cell::{Cell, Ref, RefCell};
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
    realearn_config: RefCell<RealearnConfig>,
    server_config: RefCell<ServerConfig>,
    changed_subject: RefCell<LocalSubject<'static, (), ()>>,
}

impl App {
    /// Panics if not in main thread.
    pub fn get() -> &'static App {
        Reaper::get().require_main_thread();
        unsafe { &APP }
    }

    fn load() -> App {
        let realearn_config = RealearnConfig::load().unwrap_or_else(|e| {
            debug!(App::logger(), "{}", e);
            Default::default()
        });
        let server_config = ServerConfig::load().unwrap_or_else(|e| {
            debug!(App::logger(), "{}", e);
            Default::default()
        });
        App::new(realearn_config, server_config)
    }

    fn new(realearn_config: RealearnConfig, server_config: ServerConfig) -> App {
        App {
            controller_manager: Rc::new(RefCell::new(FileBasedControllerManager::new(
                App::realearn_resource_dir_path().join("controllers"),
            ))),
            server: Rc::new(RefCell::new(RealearnServer::new(
                server_config.main.server_http_port,
                server_config.main.server_https_port,
                App::server_resource_dir_path().join("certs"),
            ))),
            realearn_config: RefCell::new(realearn_config),
            server_config: RefCell::new(server_config),
            changed_subject: Default::default(),
        }
    }

    pub fn init(&self) {
        if self.server_config.borrow().server_is_enabled() {
            self.server().borrow_mut().start();
        }
    }

    pub fn logger() -> &'static slog::Logger {
        static APP_LOGGER: once_cell::sync::Lazy<slog::Logger> =
            once_cell::sync::Lazy::new(|| create_terminal_logger().new(o!("app" => "ReaLearn")));
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

    pub fn realearn_config(&self) -> Ref<RealearnConfig> {
        self.realearn_config.borrow()
    }

    pub fn server_config(&self) -> Ref<ServerConfig> {
        self.server_config.borrow()
    }

    pub fn start_server_persistently(&self) {
        self.server.borrow_mut().start();
        self.change_server_config(ServerConfig::enable_server);
    }

    pub fn disable_server_persistently(&self) {
        self.change_server_config(ServerConfig::disable_server);
    }

    pub fn enable_server_persistently(&self) {
        self.change_server_config(ServerConfig::enable_server);
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

    fn change_server_config(&self, op: impl FnOnce(&mut ServerConfig)) {
        let mut config = self.server_config.borrow_mut();
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
pub struct RealearnConfig {
    main: RealearnMainConfig,
}

impl RealearnConfig {
    pub fn load() -> Result<RealearnConfig, String> {
        let ini_content = fs::read_to_string(&Self::config_file_path())
            .map_err(|_| "couldn't read ReaLearn config file".to_string())?;
        let config = serde_ini::from_str(&ini_content).map_err(|e| format!("{:?}", e))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let ini_content =
            serde_ini::to_string(self).map_err(|_| "couldn't serialize ReaLearn config")?;
        fs::write(Self::config_file_path(), ini_content)
            .map_err(|_| "couldn't write ReaLearn config file")?;
        Ok(())
    }

    pub fn companion_web_app_url(&self) -> url::Url {
        Url::parse(&self.main.companion_web_app_url).expect("invalid companion web app URL")
    }

    fn config_file_path() -> PathBuf {
        App::realearn_resource_dir_path().join("realearn.ini")
    }
}

#[derive(Serialize, Deserialize)]
struct RealearnMainConfig {
    #[serde(
        default = "default_companion_web_app_url",
        skip_serializing_if = "is_default_companion_web_app_url"
    )]
    companion_web_app_url: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    main: ServerMainConfig,
}

impl ServerConfig {
    pub fn load() -> Result<ServerConfig, String> {
        let ini_content = fs::read_to_string(&Self::config_file_path())
            .map_err(|_| "couldn't read server config file".to_string())?;
        let config = serde_ini::from_str(&ini_content).map_err(|e| format!("{:?}", e))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let ini_content =
            serde_ini::to_string(self).map_err(|_| "couldn't serialize server config")?;
        fs::write(Self::config_file_path(), ini_content)
            .map_err(|_| "couldn't write server config file")?;
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

    fn config_file_path() -> PathBuf {
        App::server_resource_dir_path().join("server.ini")
    }
}

#[derive(Serialize, Deserialize)]
struct ServerMainConfig {
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
fn is_default_companion_web_app_url(v: &String) -> bool {
    v == COMPANION_WEB_APP_URL
}

impl Default for RealearnMainConfig {
    fn default() -> Self {
        RealearnMainConfig {
            companion_web_app_url: default_companion_web_app_url(),
        }
    }
}

impl Default for ServerMainConfig {
    fn default() -> Self {
        ServerMainConfig {
            server_enabled: Default::default(),
            server_http_port: default_server_http_port(),
            server_https_port: default_server_https_port(),
        }
    }
}
