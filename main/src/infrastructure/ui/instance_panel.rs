use crate::infrastructure::ui::{build_unit_label, util, AppPage, UnitPanel};
use anyhow::Context;
use reaper_medium::Hbrush;
use std::cell::{Cell, OnceCell, RefCell};
use std::fmt::Debug;
use std::sync;
use std::sync::Arc;

use crate::domain::{InstanceId, SharedInstance, UnitId};
use crate::infrastructure::plugin::{
    reaper_main_window, BackboneShell, InstanceShell, SharedInstanceShell,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::colors;
use swell_ui::{DeviceContext, Dimensions, Pixels, SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct InstancePanel {
    view: ViewContext,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
    shell: OnceCell<sync::Weak<InstanceShell>>,
    /// This is necessary *only* to keep the currently displayed view in memory shortly after its
    /// unit has been removed. Without that, the view manager would not be able to upgrade the
    /// weak view for the last few dispatched calls.
    displayed_unit_panel: RefCell<Option<SharedView<UnitPanel>>>,
    displayed_unit_id: Cell<Option<UnitId>>,
    /// We have at most one app instance open per ReaLearn instance.
    app_instance: crate::infrastructure::ui::SharedAppInstance,
}

impl Drop for InstancePanel {
    fn drop(&mut self) {
        tracing::debug!("Dropping InstancePanel...");
        let _ = self.app_instance.borrow_mut().stop();
    }
}

impl InstancePanel {
    pub fn new(instance_id: InstanceId) -> InstancePanel {
        InstancePanel {
            view: Default::default(),
            shell: OnceCell::new(),
            dimensions: None.into(),
            displayed_unit_id: Default::default(),
            app_instance: crate::infrastructure::ui::create_shared_app_instance(instance_id),
            displayed_unit_panel: RefCell::new(None),
        }
    }

    pub fn app_instance(&self) -> &crate::infrastructure::ui::SharedAppInstance {
        &self.app_instance
    }

    pub fn toggle_app_instance_focus(&self) {
        let mut app_instance = self.app_instance.borrow_mut();
        if app_instance.has_focus() {
            reaper_main_window().focus();
        } else {
            let _ = app_instance.start_or_show(reaper_main_window(), None);
        }
    }

    pub fn start_or_show_app_instance(&self, page: Option<AppPage>) {
        let result = self
            .app_instance
            .borrow_mut()
            .start_or_show(reaper_main_window(), page);
        crate::base::notification::notify_user_on_anyhow_error(result);
    }

    pub fn start_show_or_hide_app_instance(&self, page: Option<AppPage>) {
        if self.app_instance.borrow_mut().is_visible() {
            self.hide_app_instance();
        } else {
            self.start_or_show_app_instance(page);
        }
    }

    pub fn hide_app_instance(&self) {
        let _ = self.app_instance.borrow_mut().hide();
    }

    pub fn stop_app_instance(&self) {
        let _ = self.app_instance.borrow_mut().stop();
    }

    pub fn app_instance_is_running(&self) -> bool {
        self.app_instance.borrow().is_running()
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| util::main_panel_dimensions().in_pixels())
    }

    pub fn open_with_resize(self: SharedView<Self>, parent_window: Window) -> Option<Window> {
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
        self.displayed_unit_id.set(None);
        self.shell
            .set(sync::Arc::downgrade(&shell))
            .expect("instance shell already set");
        if let Some(window) = self.view.window() {
            // Window is currently open. Add main unit panel as subview.
            let main_unit_panel = shell.main_unit_shell().panel().clone();
            main_unit_panel.clone().open(window);
        }
    }

    pub fn displayed_unit_id(&self) -> Option<UnitId> {
        self.displayed_unit_id.get()
    }

    pub fn open_unit_popup_menu(&self) {
        #[allow(clippy::enum_variant_names)]
        enum MenuAction {
            RemoveCurrentUnit,
            ShowUnit(Option<UnitId>),
            AddUnit,
        }
        let menu = {
            use swell_ui::menu_tree::*;
            let shell = self.shell().unwrap();
            let additional_unit_models = shell.additional_unit_models();
            let displayed_unit_id = self.displayed_unit_id.get();
            let main_unit_model = shell.main_unit_shell().model().borrow();
            anonymous_menu(
                [
                    item_with_opts(
                        "Remove current unit",
                        ItemOpts {
                            enabled: displayed_unit_id.is_some(),
                            checked: false,
                        },
                        MenuAction::RemoveCurrentUnit,
                    ),
                    separator(),
                    item_with_opts(
                        build_unit_label(&main_unit_model, None, None),
                        ItemOpts {
                            enabled: true,
                            checked: displayed_unit_id.is_none(),
                        },
                        MenuAction::ShowUnit(None),
                    ),
                ]
                .into_iter()
                .chain(
                    additional_unit_models
                        .iter()
                        .enumerate()
                        .map(|(i, unit_model)| {
                            let unit_model = unit_model.borrow();
                            let unit_id = unit_model.unit_id();
                            item_with_opts(
                                build_unit_label(&unit_model, Some(i), None),
                                ItemOpts {
                                    enabled: true,
                                    checked: displayed_unit_id == Some(unit_id),
                                },
                                MenuAction::ShowUnit(Some(unit_id)),
                            )
                        }),
                )
                .chain([separator(), item("Add unit", MenuAction::AddUnit)])
                .collect(),
            )
        };
        let action = self
            .view
            .require_window()
            .open_popup_menu(menu, Window::cursor_pos());
        if let Some(action) = action {
            match action {
                MenuAction::RemoveCurrentUnit => self.remove_current_unit(),
                MenuAction::ShowUnit(id) => self.show_unit(id),
                MenuAction::AddUnit => {
                    self.add_unit();
                }
            }
        }
    }

    fn add_unit(&self) {
        let shell = self.shell().unwrap();
        let id = shell.add_unit();
        self.show_unit(Some(id));
    }

    pub fn notify_units_changed(&self) {
        // The currently displayed unit might have been removed.
        if self.view.window().is_none() {
            // Window not open, nothing to worry
            return;
        }
        let Some(id) = self.displayed_unit_id.get() else {
            // The main unit can't be removed, we are safe
            return;
        };
        if !self.shell().unwrap().unit_exists(id) {
            // The currently displayed unit doesn't exist anymore. Show main unit.
            self.show_unit(None);
        }
    }

    fn remove_current_unit(&self) {
        let Some(id) = self.displayed_unit_id.get() else {
            // The main unit can't be removed
            return;
        };
        let _ = self.shell().unwrap().remove_unit(id);
    }

    pub fn show_unit(&self, unit_id: Option<UnitId>) {
        self.displayed_unit_id.set(unit_id);
        if let Some(window) = self.view.window() {
            if let Some(child_window) = window.first_child() {
                tracing::debug!("Closing previous unit window");
                child_window.close();
            }
            let shell = self.shell().expect("shell gone");
            self.open_unit_panel_as_child(&shell, window);
        }
    }

    fn open_unit_panel_as_child(&self, shell: &SharedInstanceShell, parent_window: Window) {
        let panel = shell
            .find_unit_panel_by_id(self.displayed_unit_id.get())
            .unwrap_or_else(|| {
                self.displayed_unit_id.set(None);
                shell.main_unit_shell().panel().clone()
            });
        self.displayed_unit_panel
            .borrow_mut()
            .replace(panel.clone());
        panel.clone().open(parent_window);
    }

    /// # Errors
    ///
    /// Returns an error if the instance shell is not set yet or gone.
    pub fn instance(&self) -> anyhow::Result<SharedInstance> {
        Ok(self.shell()?.instance().clone())
    }

    /// # Errors
    ///
    /// Returns an error if the instance shell is not set yet or gone.
    pub fn shell(&self) -> anyhow::Result<Arc<InstanceShell>> {
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
        // Optimal dimensions have been calculated and window has been reopened. Now add sub panels!
        if let Ok(shell) = self.shell() {
            self.open_unit_panel_as_child(&shell, window);
        }
        true
    }

    fn on_destroy(self: SharedView<Self>, _window: Window) {
        // No need to keep currently displayed unit panel in memory when window closed
        self.displayed_unit_panel.borrow_mut().take();
    }

    fn control_color_dialog(
        self: SharedView<Self>,
        _device_context: DeviceContext,
        _window: Window,
    ) -> Option<Hbrush> {
        if !BackboneShell::get().config().background_colors_enabled() {
            return None;
        }
        util::view::get_brush_for_color_pair(colors::instance_panel_background())
    }
}
