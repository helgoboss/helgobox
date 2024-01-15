use anyhow::Context;
use reaper_high::Reaper;

#[cfg(feature = "playtime")]
struct ToolbarItem<'a> {
    index: u32,
    command: &'a str,
    _desc: &'a str,
}

#[cfg(feature = "playtime")]
impl<'a> ToolbarItem<'a> {
    fn parse_from_ini_prop(key: &'a str, value: &'a str) -> Option<Self> {
        let Some(("item", i)) = key.split_once('_') else {
            return None;
        };
        let Some((command, desc)) = value.split_once(' ') else {
            return None;
        };
        let item = ToolbarItem {
            index: i.parse().ok()?,
            command,
            _desc: desc,
        };
        Some(item)
    }
}

#[cfg(feature = "playtime")]
fn add_toolbar_button(command_name: &str, action_label: &str) -> anyhow::Result<()> {
    // Load toolbar button INI file
    let reaper = Reaper::get();
    let reaper_menu_ini = reaper
        .medium_reaper()
        .get_resource_path(|p| p.join("reaper-menu.ini"));
    let mut ini = ini::Ini::load_from_file_opt(
        &reaper_menu_ini,
        ini::ParseOption {
            enabled_quote: false,
            enabled_escape: false,
        },
    )?;
    // Look through existing toolbar buttons
    let toolbar_section = ini
        .section_mut(Some("Main toolbar"))
        .context("couldn't find main toolbar section")?;
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
    // Add new toolbar button
    let next_item_index = max_item_index + 1;
    toolbar_section.insert(
        format!("item_{next_item_index}"),
        format!("_{command_name} {action_label}"),
    );
    ini.write_to_file(&reaper_menu_ini)?;
    reaper.medium_reaper().low().UpdateArrange();
    reaper.medium_reaper().low().UpdateTimeline();
    Ok(())
}
