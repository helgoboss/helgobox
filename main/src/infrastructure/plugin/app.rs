use crate::application::session_manager;
use crate::infrastructure::data::{FileBasedControllerManager, SharedControllerManager};
use crate::infrastructure::server::{RealearnServer, ServerClients, SharedRealearnServer};
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
        let resource_dir = App::resource_dir_path();
        App {
            controller_manager: Rc::new(RefCell::new(FileBasedControllerManager::new(
                resource_dir.join("controllers"),
            ))),
            server: Rc::new(RefCell::new(RealearnServer::new(
                config.main.server_http_port,
                config.main.server_https_port,
                resource_dir.join("certs"),
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
        session_manager::log_debug_info();
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

    fn resource_dir_path() -> PathBuf {
        let reaper_resource_path = Reaper::get().resource_path();
        reaper_resource_path.join("ReaLearn")
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
        fs::write(Self::config_file_path(), ini_content)
            .map_err(|_| "couldn't write config file")?;
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
        App::resource_dir_path().join("realearn.ini")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct MainConfig {
    server_enabled: u8,
    server_http_port: u16,
    server_https_port: u16,
}

impl Default for MainConfig {
    fn default() -> Self {
        MainConfig {
            server_enabled: 0,
            server_http_port: 39080,
            server_https_port: 39443,
        }
    }
}
