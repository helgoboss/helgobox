use crate::infrastructure::ui::util;
use anyhow::Context;
use base::tracing_debug;
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
    displayed_unit_panel_index: Cell<Option<usize>>,
}

impl Drop for InstancePanel {
    fn drop(&mut self) {
        tracing_debug!("Dropping InstancePanel");
    }
}

impl InstancePanel {
    pub fn new() -> InstancePanel {
        InstancePanel {
            view: Default::default(),
            shell: OnceCell::new(),
            dimensions: None.into(),
            displayed_unit_panel_index: Default::default(),
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
        self.displayed_unit_panel_index.set(None);
        self.shell
            .set(sync::Arc::downgrade(&shell))
            .expect("instance shell already set");
        if let Some(window) = self.view.window() {
            // Window is currently open. Add main unit panel as subview.
            let main_unit_panel = shell.main_unit_shell().panel().clone();
            main_unit_panel.clone().open(window);
        }
    }

    pub fn displayed_unit_panel_index(&self) -> Option<usize> {
        self.displayed_unit_panel_index.get()
    }

    pub fn open_unit_popup_menu(&self) {
        enum MenuAction {
            RemoveCurrentUnit,
            ShowUnit(Option<usize>),
            AddUnit,
        }
        let menu = {
            use swell_ui::menu_tree::*;
            let additional_unit_count = self.shell().unwrap().additional_unit_panel_count();
            let displayed_unit_index = self.displayed_unit_panel_index.get();
            root_menu(
                [
                    item_with_opts(
                        "Remove current unit",
                        ItemOpts {
                            enabled: displayed_unit_index.is_some(),
                            checked: false,
                        },
                        || MenuAction::RemoveCurrentUnit,
                    ),
                    separator(),
                    item_with_opts(
                        "Unit 1 (main unit)",
                        ItemOpts {
                            enabled: true,
                            checked: displayed_unit_index == None,
                        },
                        || MenuAction::ShowUnit(None),
                    ),
                ]
                .into_iter()
                .chain((0..additional_unit_count).map(|i| {
                    item_with_opts(
                        format!("Unit {}", i + 2),
                        ItemOpts {
                            enabled: true,
                            checked: displayed_unit_index == Some(i),
                        },
                        move || MenuAction::ShowUnit(Some(i)),
                    )
                }))
                .chain([separator(), item("Add unit", || MenuAction::AddUnit)])
                .collect(),
            )
        };
        let action = self
            .view
            .require_window()
            .open_simple_popup_menu(menu, Window::cursor_pos());
        if let Some(action) = action {
            match action {
                MenuAction::RemoveCurrentUnit => self.remove_current_unit(),
                MenuAction::ShowUnit(i) => self.show_unit(i),
                MenuAction::AddUnit => {
                    self.add_unit();
                }
            }
        }
    }

    fn add_unit(&self) {
        let shell = self.shell().unwrap();
        shell.add_unit();
        let index_of_new_unit = shell.additional_unit_panel_count() - 1;
        self.show_unit(Some(index_of_new_unit));
    }

    fn remove_current_unit(&self) {
        let Some(index) = self.displayed_unit_panel_index.get() else {
            return;
        };
        let new_index = index.checked_sub(1);
        self.show_unit(new_index);
        let _ = self.shell().unwrap().remove_unit(index);
    }

    fn show_unit(&self, unit_index: Option<usize>) {
        self.displayed_unit_panel_index.set(unit_index);
        if let Some(window) = self.view.window() {
            if let Some(child_window) = window.first_child() {
                child_window.close();
            }
            let panel = self
                .shell()
                .unwrap()
                .find_unit_panel_by_index(self.displayed_unit_panel_index.get())
                .expect("couldn't find unit panel");
            panel.clone().open(window);
        }
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
            shell
                .find_unit_panel_by_index(self.displayed_unit_panel_index.get())
                .expect("couldn't find unit panel")
                .open(window);
        }
        true
    }
}
