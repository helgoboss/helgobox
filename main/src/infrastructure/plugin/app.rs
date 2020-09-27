use crate::infrastructure::data::{FileBasedControllerManager, SharedControllerManager};
use crate::infrastructure::projection::ProjectionClients;
use once_cell::unsync::Lazy;
use reaper_high::Reaper;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// static mut maybe okay because we access this via `App::get()` function only and this one checks
/// the thread before returning the reference.
static mut APP: Lazy<App> = Lazy::new(App::new);

pub struct App {
    controller_manager: SharedControllerManager,
    projection_clients: ProjectionClients,
}

impl App {
    pub fn resource_dir_path() -> PathBuf {
        let reaper_resource_path = Reaper::get().resource_path();
        reaper_resource_path.join("ReaLearn")
    }

    pub fn controller_dir_path() -> PathBuf {
        App::resource_dir_path().join("controllers")
    }

    /// Panics if not in main thread.
    pub fn get() -> &'static App {
        Reaper::get().require_main_thread();
        unsafe { &APP }
    }

    fn new() -> App {
        App {
            controller_manager: Rc::new(RefCell::new(FileBasedControllerManager::new())),
            projection_clients: Default::default(),
        }
    }

    pub fn controller_manager(&self) -> SharedControllerManager {
        self.controller_manager.clone()
    }

    pub fn projection_clients(&self) -> ProjectionClients {
        self.projection_clients.clone()
    }
}
