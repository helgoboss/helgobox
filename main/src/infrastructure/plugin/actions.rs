use crate::base::notification::alert;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::test::run_test;
use enumflags2::make_bitflags;
use reaper_high::{ActionKind, KeyBinding, KeyBindingKind, Reaper};
use reaper_medium::{AcceleratorBehavior, AcceleratorKeyCode};
use std::ffi::CStr;
use swell_ui::menu_tree::{item, menu, Entry};

pub const ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME: &str = "HB_SHOW_HIDE_PLAYTIME";
pub const ACTION_SHOW_HIDE_CUSTOM_PLAYTIME_COMMAND_NAME: &str = "HB_SHOW_HIDE_CUSTOM_PLAYTIME";
pub const ACTION_TOGGLE_APP_FOCUS_COMMAND_NAME: &str = "HB_TOGGLE_APP_FOCUS";
pub const ACTION_SHOW_WELCOME_SCREEN_LABEL: &str = "Show welcome screen";

pub const ACTION_DEFS: &[ActionDef] = &[
    ActionDef {
        section: ActionSection::General,
        command_name: "HB_SHOW_WELCOME_SCREEN",
        action_name: ACTION_SHOW_WELCOME_SCREEN_LABEL,
        op: BackboneShell::show_welcome_screen,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::General,
        command_name: "REALEARN_RESOLVE_SYMBOLS",
        action_name: "Resolve symbols from clipboard",
        op: BackboneShell::resolve_symbols_from_clipboard,
        developer: true,
        requires_instance: false,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::General,
        command_name: "REALEARN_INTEGRATION_TEST",
        action_name: "Run integration test",
        op: run_test,
        developer: true,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "realearnLearnSourceForLastTouchedTarget",
        action_name: "Learn source for last touched target (reassigning target)",
        op: BackboneShell::learn_source_for_last_touched_target,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE",
        action_name: "Learn single mapping (reassigning source)",
        op: BackboneShell::learn_mapping_reassigning_learnable_source,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE_OPEN",
        action_name: "Learn single mapping (reassigning source) and open it",
        op: BackboneShell::learn_mapping_reassigning_learnable_source_open,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_FIND_FIRST_MAPPING_BY_SOURCE",
        action_name: "Find first mapping by source",
        op: BackboneShell::find_first_mapping_by_learnable_source,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_FIND_FIRST_MAPPING_BY_TARGET",
        action_name: "Find first mapping by target",
        op: BackboneShell::find_first_mapping_by_target,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_SEND_ALL_FEEDBACK",
        action_name: "Send feedback for all instances",
        op: BackboneShell::send_feedback_for_all_instances,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_DETECT_CONTROLLERS_WITH_LOGGING",
        action_name: "Auto-detect controllers (with logging)",
        op: BackboneShell::detect_controllers_with_logging,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::Playtime,
        command_name: ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME,
        action_name: "Show/hide Playtime",
        op: BackboneShell::show_hide_playtime,
        add_toolbar_button: true,
        icon: Some(c"toolbar_playtime"),
        default_key_binding: Some(KeyBinding {
            behavior: make_bitflags!(AcceleratorBehavior::{Shift | Control | VirtKey}),
            key_code: AcceleratorKeyCode::new(b'P' as _),
            kind: KeyBindingKind::GlobalText,
        }),
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::Playtime,
        command_name: ACTION_SHOW_HIDE_CUSTOM_PLAYTIME_COMMAND_NAME,
        action_name: "Show/hide custom Playtime",
        op: BackboneShell::show_hide_custom_playtime,
        icon: Some(c"toolbar_playtime_custom"),
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::General,
        command_name: ACTION_TOGGLE_APP_FOCUS_COMMAND_NAME,
        action_name: "Toggle app focus",
        op: BackboneShell::toggle_app_focus,
        default_key_binding: Some(KeyBinding {
            behavior: make_bitflags!(AcceleratorBehavior::{Shift | Control | VirtKey}),
            key_code: AcceleratorKeyCode::new(b'H' as _),
            kind: KeyBindingKind::GlobalText,
        }),
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::SoundPot,
        command_name: "REALEARN_OPEN_FIRST_POT_BROWSER",
        action_name: "Open first Pot Browser",
        op: BackboneShell::open_first_pot_browser,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    #[cfg(debug_assertions)]
    ActionDef {
        section: ActionSection::General,
        command_name: "HB_PANIC",
        action_name: "Simulate error",
        developer: true,
        op: crate::infrastructure::plugin::sandbox::simulate_error,
        ..DEFAULT_DEF
    },
];

pub struct ActionDef {
    pub section: ActionSection,
    pub command_name: &'static str,
    pub action_name: &'static str,
    pub op: fn(),
    pub developer: bool,
    pub requires_instance: bool,
    pub default_key_binding: Option<KeyBinding>,
    pub icon: Option<&'static CStr>,
    pub add_toolbar_button: bool,
}

const DEFAULT_DEF: ActionDef = ActionDef {
    section: ActionSection::General,
    command_name: "",
    action_name: "",
    op: || {},
    developer: false,
    requires_instance: false,
    default_key_binding: None,
    icon: None,
    add_toolbar_button: false,
};

impl ActionDef {
    pub fn register(&self) {
        let requires_instance = self.requires_instance;
        let op = self.op;
        Reaper::get().register_action(
            self.command_name,
            self.build_full_action_name(),
            self.default_key_binding,
            move || {
                if requires_instance && BackboneShell::get().instance_count() == 0 {
                    alert("Please add a Helgobox plug-in instance first!");
                    return;
                }
                op();
            },
            ActionKind::NotToggleable,
        );
    }

    pub fn should_appear_in_menu(&self) -> bool {
        !self.developer || cfg!(debug_assertions)
    }

    pub fn build_full_action_name(&self) -> String {
        format!(
            "{}Helgobox/{}: {}{}",
            self.developer_prefix(),
            self.section,
            self.action_name,
            self.instance_suffix(),
        )
    }

    pub fn build_menu_item(&self) -> Entry<&'static str> {
        item(
            format!(
                "{}{}{}",
                self.developer_prefix(),
                self.action_name,
                self.instance_suffix()
            ),
            self.command_name,
        )
    }

    pub fn developer_prefix(&self) -> &'static str {
        if self.developer {
            "[developer] "
        } else {
            ""
        }
    }

    pub fn instance_suffix(&self) -> &'static str {
        ""
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, strum::Display, strum::EnumIter)]
pub enum ActionSection {
    General,
    ReaLearn,
    Playtime,
    #[strum(serialize = "SoundPot (experimental)")]
    SoundPot,
}

impl ActionSection {
    pub fn build_menu(&self) -> Option<Entry<&'static str>> {
        let items: Vec<_> = ACTION_DEFS
            .iter()
            .filter(|def| def.section == *self && def.should_appear_in_menu())
            .map(|def| def.build_menu_item())
            .collect();
        if items.is_empty() {
            return None;
        }
        Some(menu(self.to_string(), items))
    }
}
