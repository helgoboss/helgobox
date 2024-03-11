use crate::infrastructure::plugin::ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME;
use anyhow::{bail, Context};
use reaper_high::Reaper;
use reaper_medium::{
    CommandId, CommandItem, MenuOrToolbarItem, PositionDescriptor, UiRefreshBehavior,
};

/// Dynamically adds or removes a toolbar button without persisting it.
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
pub fn add_or_remove_toolbar_button(command_name: &str, add: bool) -> anyhow::Result<()> {
    let action = Reaper::get().action_by_command_name(command_name);
    let command_id = action.command_id()?;
    let reaper = Reaper::get().medium_reaper();
    match scan_toolbar_for_command_id(command_id) {
        ToolbarScanOutcome::Exists { pos } => {
            if !add {
                reaper.delete_custom_menu_or_toolbar_item(
                    "Main toolbar",
                    pos,
                    UiRefreshBehavior::Refresh,
                )?;
            }
        }
        ToolbarScanOutcome::DoesntExist { .. } => {
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
    Ok(())
}

fn scan_toolbar_for_command_id(command_id: CommandId) -> ToolbarScanOutcome {
    let reaper = Reaper::get().medium_reaper();
    let mut i = 0;
    loop {
        let pos =
            reaper.get_custom_menu_or_toolbar_item("Main toolbar", i, |result| match result? {
                MenuOrToolbarItem::Command(item) if item.command_id == command_id => Some(Some(i)),
                _ => Some(None),
            });
        match pos {
            None => {
                return ToolbarScanOutcome::DoesntExist {
                    toolbar_size: i + 1,
                }
            }
            Some(None) => i += 1,
            Some(Some(pos)) => {
                return ToolbarScanOutcome::Exists { pos };
            }
        }
    }
}

enum ToolbarScanOutcome {
    Exists { pos: u32 },
    DoesntExist { toolbar_size: u32 },
}
