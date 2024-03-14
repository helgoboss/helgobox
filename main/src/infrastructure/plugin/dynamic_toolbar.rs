use reaper_high::Reaper;
use reaper_medium::{CommandId, MenuOrToolbarItem, PositionDescriptor, UiRefreshBehavior};

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
    Ok(())
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
