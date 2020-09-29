use crate::infrastructure::data::{FileBasedControllerManager, SharedControllerManager};
use crate::infrastructure::server::{RealearnServer, ServerClients, SharedRealearnServer};
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
    server: SharedRealearnServer,
}

impl App {
    /// Panics if not in main thread.
    pub fn get() -> &'static App {
        Reaper::get().require_main_thread();
        unsafe { &APP }
    }

    pub fn resource_dir_path() -> PathBuf {
        let reaper_resource_path = Reaper::get().resource_path();
        reaper_resource_path.join("ReaLearn")
    }

    pub fn controller_dir_path() -> PathBuf {
        App::resource_dir_path().join("controllers")
    }

    fn new() -> App {
        App {
            controller_manager: Rc::new(RefCell::new(FileBasedControllerManager::new())),
            server: Rc::new(RefCell::new(RealearnServer::new(3030))),
        }
    }

    // TODO-medium Return a reference to a SharedControllerManager! Clients might just want to turn
    //  this into a weak one.
    pub fn controller_manager(&self) -> SharedControllerManager {
        self.controller_manager.clone()
    }

    pub fn server(&self) -> &SharedRealearnServer {
        &self.server
    }
}
