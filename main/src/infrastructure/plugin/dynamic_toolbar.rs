use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use reaper_high::Reaper;
use reaper_medium::{CommandId, MenuOrToolbarItem, PositionDescriptor, UiRefreshBehavior};

pub fn custom_toolbar_api_is_available() -> bool {
    Reaper::get()
        .medium_reaper()
        .low()
        .pointers()
        .GetCustomMenuOrToolbarItem
        .is_some()
}

/// Dynamically adds or removes a toolbar button without persisting it, returning the command ID.
///
/// Requires REAPER version >= 711+dev0305.
///
/// # Errors
///
/// Returns and error if the command doesn't exist.
///
/// # Panics
///
/// Panics if the REAPER version is too low.
pub fn add_or_remove_toolbar_button(command_name: &str, add: bool) -> anyhow::Result<CommandId> {
    let action = Reaper::get().action_by_command_name(command_name);
    let command_id = action.command_id()?;
    let reaper = Reaper::get().medium_reaper();
    match scan_toolbar_for_command_id(command_id) {
        Some(pos) => {
            if !add {
                reaper.delete_custom_menu_or_toolbar_item(
                    "Main toolbar",
                    pos,
                    UiRefreshBehavior::Refresh,
                )?;
            }
        }
        None => {
            if add {
                reaper.add_custom_menu_or_toolbar_item_command(
                    "Main toolbar",
                    PositionDescriptor::Append,
                    command_id,
                    0,
                    action.name().unwrap_or_default(),
                    None,
                    UiRefreshBehavior::Refresh,
                )?;
            }
        }
    }
    Ok(command_id)
}

#[derive(Debug)]
pub struct ToolbarChangeDetector {
    /// Map from command ID to command name.
    observed_commands: NonCryptoHashMap<CommandId, String>,
    present_commands: NonCryptoHashSet<CommandId>,
}

impl ToolbarChangeDetector {
    pub fn new(observed_commands: NonCryptoHashMap<CommandId, String>) -> Self {
        Self {
            observed_commands,
            present_commands: Default::default(),
        }
    }

    pub fn detect_manually_removed_commands(&mut self) -> Vec<&str> {
        self.observed_commands
            .iter()
            .filter(|(command_id, _)| {
                if scan_toolbar_for_command_id(**command_id).is_some() {
                    self.present_commands.insert(**command_id);
                    false
                } else {
                    self.present_commands.remove(command_id)
                }
            })
            .map(|(_, command_name)| command_name.as_str())
            .collect()
    }
}

fn scan_toolbar_for_command_id(command_id: CommandId) -> Option<u32> {
    let reaper = Reaper::get().medium_reaper();
    let mut i = 0;
    loop {
        let pos =
            reaper.get_custom_menu_or_toolbar_item("Main toolbar", i, |result| match result? {
                MenuOrToolbarItem::Command(item) if item.command_id == command_id => Some(Some(i)),
                _ => Some(None),
            })?;
        match pos {
            None => i += 1,
            Some(pos) => {
                return Some(pos);
            }
        }
    }
}
