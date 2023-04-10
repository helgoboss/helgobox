use crate::base::SenderToNormalThread;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::egui_views;
use crate::infrastructure::ui::egui_views::pot_browser_panel::{run_ui, State};
use crossbeam_channel::Receiver;
use derivative::Derivative;
use raw_window_handle::HasRawWindowHandle;
use reaper_low::{raw, Swell};
use std::cell::{Cell, RefCell};
use std::time::Duration;
use swell_ui::{DialogUnits, Dimensions, Point, SharedView, View, ViewContext, Window};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct PotBrowserPanel {
    view: ViewContext,
    child_window: Cell<Option<Window>>,
}

impl PotBrowserPanel {
    pub fn new() -> Self {
        Self {
            view: Default::default(),
            child_window: Default::default(),
        }
    }
}

impl View for PotBrowserPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_EMPTY_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        let screen_size = Window::screen_size();
        window.move_to(Point::default());
        window.resize(screen_size);
        let child_window_handle = egui_views::open(window, "Pot browser", State::new(), run_ui);
        if let Ok(child_window) =
            Window::from_raw_window_handle(child_window_handle.raw_window_handle())
        {
            self.child_window.set(Some(child_window));
        }
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
        if let Some(child_window) = self.child_window.get() {
            let new_size = self.view.require_window().size();
            dbg!(child_window.size());
            child_window.resize(new_size);
        }
        true
    }
}
