use crate::application::session_manager;
use crate::infrastructure::data::{FileBasedControllerManager, SharedControllerManager};
use crate::infrastructure::server::{RealearnServer, ServerClients, SharedRealearnServer};
use once_cell::unsync::Lazy;
use reaper_high::{create_terminal_logger, Reaper};
use serde::{Deserialize, Serialize};
use slog::{debug, o};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

/// static mut maybe okay because we access this via `App::get()` function only and this one checks
/// the thread before returning the reference.
static mut APP: Lazy<App> = Lazy::new(App::load);

pub struct App {
    controller_manager: SharedControllerManager,
    server: SharedRealearnServer,
    server_is_enabled: bool,
}

impl App {
    /// Panics if not in main thread.
    pub fn get() -> &'static App {
        Reaper::get().require_main_thread();
        unsafe { &APP }
    }

    fn load() -> App {
        let config = App::load_config().unwrap_or_else(|e| {
            debug!(App::logger(), "{}", e);
            Default::default()
        });
        App::new(&config)
    }

    fn load_config() -> Result<AppConfig, String> {
        let ini_content = fs::read_to_string(&App::preference_file_path())
            .map_err(|_| "couldn't read preference file".to_string())?;
        let config = serde_ini::from_str(&ini_content).map_err(|e| format!("{:?}", e))?;
        Ok(config)
    }

    fn new(config: &AppConfig) -> App {
        let resource_dir = App::resource_dir_path();
        App {
            controller_manager: Rc::new(RefCell::new(FileBasedControllerManager::new(
                resource_dir.join("controllers"),
            ))),
            server: Rc::new(RefCell::new(RealearnServer::new(
                config.main.server_port,
                resource_dir.join("certs"),
            ))),
            server_is_enabled: config.main.server_enabled,
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

    pub fn server_is_enabled(&self) -> bool {
        self.server_is_enabled
    }

    /// Logging debug info is always initiated by a particular session.
    pub fn log_debug_info(&self, session_id: &str) {
        session_manager::log_debug_info();
        self.server.borrow().log_debug_info(session_id);
        self.controller_manager.borrow().log_debug_info();
    }

    fn resource_dir_path() -> PathBuf {
        let reaper_resource_path = Reaper::get().resource_path();
        reaper_resource_path.join("ReaLearn")
    }

    fn preference_file_path() -> PathBuf {
        App::resource_dir_path().join("preferences.ini")
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct AppConfig {
    main: MainConfig,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct MainConfig {
    server_enabled: bool,
    server_port: u16,
}

impl Default for MainConfig {
    fn default() -> Self {
        MainConfig {
            server_enabled: false,
            server_port: 49281,
        }
    }
}
