use c_str_macro::c_str;
use enumset::EnumSet;
use reaper_low::{raw, Swell};
use std::fmt::Debug;

use crate::base::notification::alert;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::util::{fonts, symbols};
use swell_ui::{FontDescriptor, SharedView, View, ViewContext, ViewManager, Window};

#[derive(Debug)]
pub struct SetupPanel {
    view: ViewContext,
}

impl SetupPanel {
    pub fn new() -> Self {
        Self {
            view: Default::default(),
        }
    }
}

impl View for SetupPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_SETUP_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.center_on_screen();
        let large_font = fonts::welcome_screen_font(20);
        let medium_font = fonts::welcome_screen_font(14);
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
        text_3.set_text(format!("Tip: You can come back here at any time via\nExtensions {arrow} Helgobox {arrow} Show welcome screen"));
        // Checkboxes
        let playtime_checkbox = window.require_control(root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON);
        playtime_checkbox.set_cached_font(medium_font);
        playtime_checkbox.check();
        self.invalidate_controls();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_SETUP_ADD_PLAYTIME_TOOLBAR_BUTTON => {
                self.invalidate_controls();
            }
            root::ID_SETUP_PANEL_OK => {
                self.apply_and_close();
            }
            // IDCANCEL is escape button
            raw::IDCANCEL => {
                self.close();
            }
            _ => {}
        }
    }
}

impl SetupPanel {
    fn invalidate_controls(&self) {
        let button_text = if self.build_instructions().is_empty() {
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
            alert("Additional setup finished!")
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
                BackboneShell::add_toolbar_buttons();
            }
        }
    }
}
