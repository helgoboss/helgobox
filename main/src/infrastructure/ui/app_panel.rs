use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use libloading::{Library, Symbol};
use reaper_low::raw;
use reaper_low::raw::HWND;
use std::env;
use std::error::Error;
use std::ffi::{c_char, CString};
use std::path::Path;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct AppPanel {
    view: ViewContext,
    app: LoadedApp,
}

impl AppPanel {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let panel = Self {
            view: Default::default(),
            app: LoadedApp::load()?,
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
        self.app.run_in_parent(window).unwrap();
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
pub struct LoadedApp {
    main_library: Library,
    _dependencies: Vec<Library>,
}

// TODO-high-playtime Adjust

#[cfg(target_os = "macos")]
const APP_BASE_DIR: &str = "/Users/helgoboss/Documents/projects/dev/playtime/build/macos/Build/Products/Release/playtime.app";

#[cfg(target_os = "windows")]
const APP_BASE_DIR: &str =
    "C:\\Users\\benja\\Documents\\projects\\dev\\playtime\\build\\windows\\runner\\Release";

impl LoadedApp {
    pub fn load() -> Result<Self, libloading::Error> {
        let app_base_dir = Path::new(APP_BASE_DIR);
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
                        "Contents/Frameworks/url_launcher_macos.framework/url_launcher_macos",
                    ],
                )
            }
        };
        let app = unsafe {
            LoadedApp {
                _dependencies: dependencies
                    .into_iter()
                    .filter_map(|dep| Library::new(app_base_dir.join(dep)).ok())
                    .collect(),
                main_library: Library::new(app_base_dir.join(main_library))?,
            }
        };
        Ok(app)
    }

    pub fn run_in_parent(&self, parent_window: Window) -> Result<(), &'static str> {
        let app_base_dir_c_string =
            CString::new(APP_BASE_DIR).map_err(|_| "app base dir is not valid UTF-8")?;
        with_temporarily_changed_working_directory(APP_BASE_DIR, || {
            prepare_app_launch();
            let successful = unsafe {
                let symbol: Symbol<RunInParent> = self
                    .main_library
                    .get(b"run_app_in_parent\0")
                    .map_err(|_| "failed to load run_app_in_parent function")?;
                symbol(parent_window.raw(), app_base_dir_c_string.as_ptr())
            };
            if !successful {
                return Err("couldn't launch app");
            }
            Ok(())
        })
    }
}

type RunInParent =
    unsafe extern "stdcall" fn(parent_window: HWND, app_base_dir_utf8_c_str: *const c_char) -> bool;

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
