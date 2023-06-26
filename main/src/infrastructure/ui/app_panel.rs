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
use std::error::Error;
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

impl PlaytimeApp {
    pub fn load() -> Result<Self, libloading::Error> {
        let library = unsafe {
            Library::new("C:\\Users\\benja\\Documents\\projects\\dev\\playtime\\build\\windows\\runner\\Debug\\playtime.dll")
        }?;
        let playtime = Self { library };
        Ok(playtime)
    }

    pub fn run(&self, parent_window: Window) -> Result<(), &'static str> {
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
