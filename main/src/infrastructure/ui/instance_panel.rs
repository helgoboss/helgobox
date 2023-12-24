use crate::infrastructure::ui::util;
use anyhow::Context;
use std::cell::{Cell, OnceCell};
use std::fmt::Debug;
use std::sync;
use std::sync::Arc;

use crate::infrastructure::plugin::InstanceShell;
use crate::infrastructure::ui::bindings::root;
use swell_ui::{Dimensions, Pixels, SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct InstancePanel {
    view: ViewContext,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
    shell: OnceCell<sync::Weak<InstanceShell>>,
    displayed_additional_unit_panel_index: Cell<Option<usize>>,
}

impl InstancePanel {
    pub fn new() -> InstancePanel {
        InstancePanel {
            view: Default::default(),
            shell: OnceCell::new(),
            dimensions: None.into(),
            displayed_additional_unit_panel_index: Default::default(),
        }
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| util::main_panel_dimensions().in_pixels())
    }

    pub fn open_with_resize(self: SharedView<Self>, parent_window: Window) {
        #[cfg(target_family = "windows")]
        {
            // On Windows, the first time opening the dialog window is just to determine the best
            // dimensions based on HiDPI settings.
            // TODO-low If we skip this, the dimensions would be saved. Wouldn't that be better?
            //  I guess if there are multiple screens, keeping this line is better. Then it's a
            //  matter of reopening the GUI to improve scaling. Test it!
            self.dimensions.replace(None);
        }
        self.open(parent_window)
    }

    pub fn notify_shell_available(&self, shell: Arc<InstanceShell>) {
        self.displayed_additional_unit_panel_index.set(None);
        self.shell
            .set(sync::Arc::downgrade(&shell))
            .expect("instance shell already set");
        if let Some(window) = self.view.window() {
            // Window is currently open. Add main unit panel as subview.
            let main_unit_panel = shell.main_unit_shell().panel().clone();
            main_unit_panel.clone().open(window);
        }
    }

    pub fn add_unit(&self) {
        let shell = self.shell().unwrap();
        shell.add_unit();
        let index_of_new_unit = shell.additional_unit_panel_count() - 1;
        self.displayed_additional_unit_panel_index
            .set(Some(index_of_new_unit));
        // TODO-high CONTINUE Display immediately
    }

    fn shell(&self) -> anyhow::Result<Arc<InstanceShell>> {
        self.shell
            .get()
            .context("instance shell not yet set")?
            .upgrade()
            .context("instance shell gone")
    }
}

impl View for InstancePanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_INSTANCE_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        #[cfg(target_family = "windows")]
        if self.dimensions.get().is_none() {
            // The dialog has been opened by user request but the optimal dimensions have not yet
            // been figured out. Figure them out now.
            self.dimensions.replace(Some(
                window.convert_to_pixels(util::main_panel_dimensions()),
            ));
            // Close and reopen window, this time with `dimensions()` returning the optimal size to
            // the host.
            let parent_window = window.parent().expect("must have parent");
            window.destroy();
            self.open(parent_window);
            return false;
        }
        if let Ok(shell) = self.shell() {
            let panel = match self.displayed_additional_unit_panel_index.get() {
                None => shell.main_unit_shell().panel().clone(),
                Some(i) => shell
                    .find_additional_unit_panel_by_index(i)
                    .expect("couldn't find additional unit panel"),
            };
            panel.open(window);
        }
        true
    }
}
