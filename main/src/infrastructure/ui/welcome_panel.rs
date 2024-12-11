use reaper_low::raw;
use std::fmt::Debug;

use crate::infrastructure::plugin::dynamic_toolbar::custom_toolbar_api_is_available;
use crate::infrastructure::plugin::{
    BackboneShell, ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME,
    ACTION_SHOW_HIDE_PLAYTIME_FROM_TEMPLATE_COMMAND_NAME, ACTION_SHOW_WELCOME_SCREEN_LABEL,
};
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{fonts, symbols};
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
        }
        BackboneShell::get().toggle_toolbar_button_dynamically(command_name)?;
        self.invalidate_controls();
        Ok(())
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
        text_1.set_text("Helgobox has been successfully installed!");
        // Text 2
        let text_2 = window.require_control(root::ID_SETUP_INTRO_TEXT_2);
        text_2.set_cached_font(medium_font);
        text_2.set_text("Consider the following options to optimize your user experience:");
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
                self.toggle_toolbar_button(ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME)
                    .expect("couldn't toggle toolbar button");
            }
            root::ID_SETUP_ADD_PLAYTIME_FROM_TEMPLATE_TOOLBAR_BUTTON => {
                self.toggle_toolbar_button(ACTION_SHOW_HIDE_PLAYTIME_FROM_TEMPLATE_COMMAND_NAME)
                    .expect("couldn't toggle toolbar button");
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

impl WelcomePanel {
    fn invalidate_controls(&self) {
        if custom_toolbar_api_is_available() {
            self.invalidate_toolbar_checkboxes();
        }
        self.invalidate_button();
    }

    fn invalidate_toolbar_checkboxes(&self) {
        let bindings = [
            (
                root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON,
                ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME,
            ),
            (
                root::ID_SETUP_ADD_PLAYTIME_FROM_TEMPLATE_TOOLBAR_BUTTON,
                ACTION_SHOW_HIDE_PLAYTIME_FROM_TEMPLATE_COMMAND_NAME,
            ),
        ];
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
