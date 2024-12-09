use enumset::EnumSet;
use reaper_low::raw;
use std::fmt::Debug;

use crate::base::notification::alert;
use crate::infrastructure::plugin::dynamic_toolbar::custom_toolbar_api_is_available;
use crate::infrastructure::plugin::{
    BackboneShell, ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME, ACTION_SHOW_WELCOME_SCREEN_LABEL,
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
        if custom_toolbar_api_is_available() {
            BackboneShell::get().toggle_toolbar_button_dynamically(command_name)?;
        }
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
        let playtime_checkbox = window.require_control(root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON);
        playtime_checkbox.check();
        self.invalidate_controls();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON => {
                self.toggle_toolbar_button(ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME)
                    .expect("couldn't toggle toolbar button");
            }
            root::ID_SETUP_PANEL_OK => {
                if custom_toolbar_api_is_available() {
                    self.close()
                } else {
                    self.apply_and_close();
                }
            }
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
            self.invalidate_playtime_checkbox();
        }
        self.invalidate_button();
    }

    fn invalidate_playtime_checkbox(&self) {
        let checked = BackboneShell::get()
            .config()
            .toolbar_button_is_enabled(ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME);
        self.view
            .require_control(root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON)
            .set_checked(checked);
    }

    fn invalidate_button(&self) {
        let button_text =
            if self.build_instructions().is_empty() || custom_toolbar_api_is_available() {
                "Close"
            } else {
                "Continue"
            };
        self.view
            .require_control(root::ID_SETUP_PANEL_OK)
            .set_text(button_text);
    }

    fn apply_and_close(&self) {
        let instructions = self.build_instructions();
        if !instructions.is_empty() {
            for instruction in instructions {
                instruction.execute();
            }
            let addition = if custom_toolbar_api_is_available() {
                ""
            } else {
                "\n\nIf you enabled the toolbar button, please restart REAPER now (otherwise you will not see the button)!"
            };
            alert(format!("Additional setup finished!{addition}"));
        }
        self.close();
    }

    fn build_instructions(&self) -> EnumSet<SetupInstruction> {
        let mut set = EnumSet::empty();
        if self
            .view
            .require_control(root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON)
            .is_checked()
        {
            set.insert(SetupInstruction::PlaytimeToolbarButton);
        }
        set
    }
}

#[derive(enumset::EnumSetType)]
enum SetupInstruction {
    PlaytimeToolbarButton,
}

impl SetupInstruction {
    pub fn execute(&self) {
        match self {
            SetupInstruction::PlaytimeToolbarButton => {
                BackboneShell::add_toolbar_buttons_persistently();
            }
        }
    }
}
