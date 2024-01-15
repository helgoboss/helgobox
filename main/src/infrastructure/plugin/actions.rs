use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::test::run_test;
use reaper_high::{ActionKind, Reaper};

pub struct ActionDef {
    pub section: ActionSection,
    pub command_name: &'static str,
    pub action_name: &'static str,
    pub op: fn(),
    pub developer: bool,
    pub requires_instance: bool,
}

const DEFAULT_DEF: ActionDef = ActionDef {
    section: ActionSection::General,
    command_name: "",
    action_name: "",
    op: || {},
    developer: false,
    requires_instance: false,
};

impl ActionDef {
    pub fn register(&self) {
        Reaper::get().register_action(
            self.command_name,
            format!(
                "{}Helgobox/{}: {}{}",
                self.developer_prefix(),
                self.section,
                self.action_name,
                self.instance_suffix(),
            ),
            self.op,
            ActionKind::NotToggleable,
        );
    }

    pub fn developer_prefix(&self) -> &'static str {
        if self.developer {
            "[developer] "
        } else {
            ""
        }
    }

    pub fn instance_suffix(&self) -> &'static str {
        if self.requires_instance {
            " (requires instance)"
        } else {
            ""
        }
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

pub const ACTION_DEFS: &[ActionDef] = &[
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
        op: BackboneShell::learn_mapping_reassigning_source,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE_OPEN",
        action_name: "Learn single mapping (reassigning source) and open it",
        op: BackboneShell::learn_mapping_reassigning_source_open,
        requires_instance: true,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: ActionSection::ReaLearn,
        command_name: "REALEARN_FIND_FIRST_MAPPING_BY_SOURCE",
        action_name: "Find first mapping by source",
        op: BackboneShell::find_first_mapping_by_source,
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
        section: ActionSection::SoundPot,
        command_name: "REALEARN_OPEN_FIRST_POT_BROWSER",
        action_name: "Open first Pot Browser",
        op: BackboneShell::open_first_pot_browser,
        requires_instance: true,
        ..DEFAULT_DEF
    },
];
