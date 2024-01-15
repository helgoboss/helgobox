use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::test::run_test;
use reaper_high::{ActionKind, Reaper};

pub struct ActionDef {
    pub section: TargetSection,
    pub command_name: &'static str,
    pub action_name: &'static str,
    pub op: fn(),
    pub developer: bool,
}

const DEFAULT_DEF: ActionDef = ActionDef {
    section: TargetSection::Helgobox,
    command_name: "",
    action_name: "",
    op: || {},
    developer: false,
};

impl ActionDef {
    pub fn register(&self) {
        let developer_suffix = if self.developer { "[developer] " } else { "" };
        Reaper::get().register_action(
            self.command_name,
            format!("{developer_suffix}{}: {}", self.section, self.action_name),
            self.op,
            ActionKind::NotToggleable,
        );
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, strum::Display, strum::EnumIter)]
pub enum TargetSection {
    Helgobox,
    ReaLearn,
    Playtime,
    Pot,
}

pub const ACTION_DEFS: &[ActionDef] = &[
    ActionDef {
        section: TargetSection::Helgobox,
        command_name: "REALEARN_RESOLVE_SYMBOLS",
        action_name: "Resolve symbols from clipboard",
        op: BackboneShell::resolve_symbols_from_clipboard,
        developer: true,
    },
    ActionDef {
        section: TargetSection::Helgobox,
        command_name: "REALEARN_INTEGRATION_TEST",
        action_name: "Run integration test",
        op: run_test,
        developer: true,
    },
    ActionDef {
        section: TargetSection::ReaLearn,
        command_name: "realearnLearnSourceForLastTouchedTarget",
        action_name: "Learn source for last touched target (reassigning target)",
        op: BackboneShell::learn_source_for_last_touched_target,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: TargetSection::ReaLearn,
        command_name: "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE",
        action_name: "Learn single mapping (reassigning source)",
        op: BackboneShell::learn_mapping_reassigning_source,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: TargetSection::ReaLearn,
        command_name: "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE_OPEN",
        action_name: "Learn single mapping (reassigning source) and open it",
        op: BackboneShell::learn_mapping_reassigning_source_open,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: TargetSection::ReaLearn,
        command_name: "REALEARN_FIND_FIRST_MAPPING_BY_SOURCE",
        action_name: "Find first mapping by source",
        op: BackboneShell::find_first_mapping_by_source,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: TargetSection::ReaLearn,
        command_name: "REALEARN_FIND_FIRST_MAPPING_BY_TARGET",
        action_name: "Find first mapping by target",
        op: BackboneShell::find_first_mapping_by_target,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: TargetSection::ReaLearn,
        command_name: "REALEARN_SEND_ALL_FEEDBACK",
        action_name: "Send feedback for all instances",
        op: BackboneShell::send_feedback_for_all_instances,
        ..DEFAULT_DEF
    },
    ActionDef {
        section: TargetSection::Pot,
        command_name: "REALEARN_OPEN_FIRST_POT_BROWSER",
        action_name: "Open first Pot Browser",
        op: BackboneShell::open_first_pot_browser,
        ..DEFAULT_DEF
    },
];
