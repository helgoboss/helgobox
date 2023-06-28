use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views::advanced_script_editor;
use crate::infrastructure::ui::egui_views::advanced_script_editor::{
    SharedValue, State, Toolbox, Value,
};
use crate::infrastructure::ui::{egui_views, ScriptEditorInput};
use base::{blocking_lock, SenderToNormalThread};
use crossbeam_channel::Receiver;
use derivative::Derivative;
use libloading::{Library, Symbol};
use reaper_low::raw;
use reaper_low::raw::HWND;
use semver::Version;
use std::cell::RefCell;
use std::env;
use std::error::Error;
use std::path::Path;
use std::time::Duration;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct AppPanel {
    view: ViewContext,
    app: PlaytimeApp,
}

impl AppPanel {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let panel = Self {
            view: Default::default(),
            app: PlaytimeApp::load()?,
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
        self.app.run(window).unwrap();
        true
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            // Escape key
            raw::IDCANCEL => self.close(),
            _ => {}
        }
    }

    fn resized(self: SharedView<Self>) -> bool {
        egui_views::on_parent_window_resize(self.view.require_window())
    }
}

#[derive(Debug)]
pub struct PlaytimeApp {
    library: Library,
}

// TODO-high-playtime Adjust

#[cfg(target_os = "macos")]
const APP_BASE_DIR: &str = "/Users/helgoboss/Documents/projects/dev/playtime/build/macos/Build/Products/Release/playtime.app";

#[cfg(target_os = "windows")]
const APP_BASE_DIR: &str =
    "C:\\Users\\benja\\Documents\\projects\\dev\\playtime\\build\\windows\\runner\\Release";

impl PlaytimeApp {
    pub fn load() -> Result<Self, libloading::Error> {
        let app_base_dir = Path::new(APP_BASE_DIR);
        let library = unsafe {
            #[cfg(target_os = "windows")]
            {
                let lib1 = app_base_dir.join("flutter_windows.dll");
                let lib2 = app_base_dir.join("url_launcher_windows_plugin.dll");
                let lib3 = app_base_dir.join("playtime.dll");
                let libs = vec![Library::new(lib1).unwrap(), Library::new(lib2).unwrap()];
                let main_lib = Library::new(lib3);
                dbg!(&libs);
                main_lib
            }
            #[cfg(target_os = "macos")]
            {
                let lib1 =
                    app_base_dir.join("Contents/Frameworks/FlutterMacOS.framework/FlutterMacOS");
                let lib2 = app_base_dir
                    .join("Contents/Frameworks/url_launcher_macos.framework/url_launcher_macos");
                let lib3 = app_base_dir.join("Contents/MacOS/playtime");
                let libs = vec![Library::new(lib1).unwrap(), Library::new(lib2).unwrap()];
                let main_lib = Library::new(lib3);
                dbg!(&libs);
                main_lib
            }
        }?;
        dbg!(&library);
        let playtime = Self { library };
        Ok(playtime)
    }

    pub fn run(&self, parent_window: Window) -> Result<(), &'static str> {
        // TODO-high-playtime Safely revert current working directory after that!
        env::set_current_dir(APP_BASE_DIR).unwrap();
        unsafe {
            let symbol: Symbol<Run> = self
                .library
                .get(b"runPlaytime\0")
                .map_err(|_| "failed to load run function")?;
            symbol(parent_window.raw());
        };
        Ok(())
    }
}

type Run = unsafe extern "stdcall" fn(hwnd: HWND) -> std::ffi::c_int;
