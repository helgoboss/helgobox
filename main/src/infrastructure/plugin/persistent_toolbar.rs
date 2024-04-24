use anyhow::{bail, Context};
use reaper_high::Reaper;

/// This attempts to add a toolbar button persistently by modifying the "reaper-menu.ini" file.
///
/// This should only be used for REAPER versions < 711+dev0305. Later versions have API functions for dynamically
/// adding and removing toolbar buttons, which is more flexible and doesn't require a restart of REAPER.
pub fn add_toolbar_button_persistently(
    command_name: &str,
    action_label: &str,
    icon_file_name: &str,
) -> anyhow::Result<()> {
    // Load toolbar button INI file
    let reaper = Reaper::get();
    let reaper_menu_ini = reaper
        .medium_reaper()
        .get_resource_path(|p| p.join("reaper-menu.ini"));
    const MISSING_CUSTOMIZATION: &str = "Because of limitations of the REAPER extension API, Helgobox can't automatically add toolbar buttons if you haven't already customized the toolbar at least once!\n\
        \n\
        Please add an arbitrary toolbar customization first and try again:\n\
        \n\
        1. Right-click the main toolbar\n\
        2. Click \"Customize toolbar...\"\n\
        3. Click \"Add...\"\n\
        4. Choose an arbitrary action, e.g. \"Reset all MIDI devices\"\n\
        5. Click \"Select/close\"\n\
        6. Click \"OK\"\n\
        7. Try again\n\
    ";
    let mut ini = ini::Ini::load_from_file_opt(
        &reaper_menu_ini,
        ini::ParseOption {
            enabled_quote: false,
            enabled_escape: false,
        },
    )
    .context(MISSING_CUSTOMIZATION)?;
    // Look through existing toolbar buttons
    let toolbar_section = ini
        .section_mut(Some("Main toolbar"))
        .context(MISSING_CUSTOMIZATION)?;
    let mut max_item_index = -1i32;
    for (key, value) in toolbar_section.iter() {
        let Some(toolbar_item) = ToolbarItem::parse_from_ini_prop(key, value) else {
            continue;
        };
        if &toolbar_item.command[1..] == command_name {
            // Toolbar button exists already
            return Ok(());
        }
        max_item_index = max_item_index.max(toolbar_item.index as _);
    }
    if max_item_index < 0 {
        bail!(MISSING_CUSTOMIZATION);
    }
    // Add new toolbar button
    let next_item_index = max_item_index + 1;
    toolbar_section.insert(
        format!("item_{next_item_index}"),
        format!("_{command_name} {action_label}"),
    );
    if !icon_file_name.is_empty() {
        toolbar_section.insert(format!("icon_{next_item_index}"), icon_file_name);
    }
    ini.write_to_file(&reaper_menu_ini)?;
    reaper.medium_reaper().update_arrange();
    reaper.medium_reaper().update_timeline();
    Ok(())
}

struct ToolbarItem<'a> {
    index: u32,
    command: &'a str,
    _desc: &'a str,
}

impl<'a> ToolbarItem<'a> {
    fn parse_from_ini_prop(key: &'a str, value: &'a str) -> Option<Self> {
        let Some(("item", i)) = key.split_once('_') else {
            return None;
        };
        let (command, desc) = value.split_once(' ')?;
        let item = ToolbarItem {
            index: i.parse().ok()?,
            command,
            _desc: desc,
        };
        Some(item)
    }
}
