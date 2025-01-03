use crate::base::notification::notify_user_on_anyhow_error;
use crate::infrastructure::plugin::dynamic_toolbar::custom_toolbar_api_is_available;
use crate::infrastructure::plugin::{
    BackboneShell, ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME, ACTION_SHOW_WELCOME_SCREEN_LABEL,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{fonts, symbols};
use reaper_low::raw;
use std::fmt::Debug;
use swell_ui::{SharedView, View, ViewContext, Window};

#[derive(Debug)]
pub struct WelcomePanel {
    view: ViewContext,
}

impl WelcomePanel {
    pub fn new() -> Self {
        Self {
            view: Default::default(),
        }
    }

    fn toggle_toolbar_button(&self, command_name: &str) -> anyhow::Result<()> {
        if !custom_toolbar_api_is_available() {
            self.view.require_window().alert(
                "Helgobox",
                "To use this feature, please update REAPER to version 7.12 or later!",
            );
            return Ok(());
        }
        BackboneShell::get().toggle_toolbar_button_dynamically(command_name)?;
        self.invalidate_controls();
        Ok(())
    }

    fn toggle_send_errors_to_dev(&self) {
        let shell = BackboneShell::get();
        let value = shell.config().send_errors_to_dev();
        shell.set_send_errors_to_dev_persistently(!value);
        self.invalidate_controls();
    }

    fn toggle_show_errors_in_console(&self) {
        let shell = BackboneShell::get();
        let value = shell.config().show_errors_in_console();
        shell.set_show_errors_in_console_persistently(!value);
        self.invalidate_controls();
    }

    fn invalidate_controls(&self) {
        if custom_toolbar_api_is_available() {
            self.invalidate_toolbar_checkboxes();
        }
        let send_errors_to_dev = BackboneShell::get().config().send_errors_to_dev();
        let show_errors_in_console = BackboneShell::get().config().show_errors_in_console();
        let comment = match (send_errors_to_dev, show_errors_in_console) {
            (false, false) => {
                Some("Please consider checking at least one of the error checkboxes, as it helps to improve Helgobox!")
            }
            (false, true) => None,
            (true, _) => Some("Errors are sent anonymously. Please see our privacy statement for details."),
        };
        self.view
            .require_control(root::ID_SETUP_SEND_ERRORS_TO_DEV)
            .set_checked(send_errors_to_dev);
        self.view
            .require_control(root::ID_SETUP_SHOW_ERRORS_IN_CONSOLE)
            .set_checked(show_errors_in_console);
        self.view
            .require_control(root::ID_SETUP_COMMENT)
            .set_text_or_hide(comment);
        self.invalidate_button();
    }

    fn invalidate_toolbar_checkboxes(&self) {
        let bindings = [(
            root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON,
            ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME,
        )];
        for (control_id, action_name) in bindings {
            let checked = BackboneShell::get()
                .config()
                .toolbar_button_is_enabled(action_name);
            self.view.require_control(control_id).set_checked(checked);
        }
    }

    fn invalidate_button(&self) {
        self.view
            .require_control(root::ID_SETUP_PANEL_OK)
            .set_text("Close");
    }
}

impl View for WelcomePanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_SETUP_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.center_on_screen();
        let large_font = fonts::normal_font(window, 18);
        let medium_font = fonts::normal_font(window, 12);
        // Text 1
        let text_1 = window.require_control(root::ID_SETUP_INTRO_TEXT_1);
        text_1.set_cached_font(large_font);
        text_1.set_text("Helgobox has been\nsuccessfully installed!");
        // Text 2
        let text_2 = window.require_control(root::ID_SETUP_INTRO_TEXT_2);
        text_2.set_cached_font(medium_font);
        text_2.set_text("Consider the following options\nto optimize your user experience:");
        // Text 3
        let text_3 = window.require_control(root::ID_SETUP_TIP_TEXT);
        let arrow = symbols::arrow_right_symbol();
        text_3.set_text(format!("Tip: You can come back here at any time via\nExtensions {arrow} Helgobox {arrow} {ACTION_SHOW_WELCOME_SCREEN_LABEL}"));
        // Checkboxes
        self.invalidate_controls();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON => {
                notify_user_on_anyhow_error(
                    self.toggle_toolbar_button(ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME),
                );
            }
            root::ID_SETUP_SEND_ERRORS_TO_DEV => {
                self.toggle_send_errors_to_dev();
            }
            root::ID_SETUP_SHOW_ERRORS_IN_CONSOLE => {
                self.toggle_show_errors_in_console();
            }
            root::ID_SETUP_PANEL_OK => self.close(),
            // IDCANCEL is escape button
            raw::IDCANCEL => {
                self.close();
            }
            _ => {}
        }
    }
}
