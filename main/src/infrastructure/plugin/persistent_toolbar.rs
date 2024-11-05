use anyhow::{bail, Context};
use camino::Utf8PathBuf;
use ini::{Ini, Properties};
use reaper_high::Reaper;

/// This attempts to add a toolbar button persistently by modifying the "reaper-menu.ini" file.
///
/// This should only be used for REAPER versions < 711+dev0305. Later versions have API functions for dynamically
/// adding and removing toolbar buttons, which is more flexible and doesn't require a restart of REAPER.
pub fn add_toolbar_button_persistently(
    command_name: &str,
    action_label: &str,
    icon_file_name: Option<&str>,
) -> anyhow::Result<()> {
    // Load toolbar button INI file
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
    let mut ini = MenuIni::load().context(MISSING_CUSTOMIZATION)?;
    // Look through existing toolbar buttons
    let mut toolbar_section = ini
        .get_toolbar("Main toolbar")
        .context(MISSING_CUSTOMIZATION)?;
    let mut max_item_index = -1i32;
    for toolbar_item in toolbar_section.items() {
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
    let new_item = ToolbarItem {
        index: next_item_index as u32,
        command: command_name,
        desc: action_label,
        icon: icon_file_name,
    };
    toolbar_section.add_item(new_item);
    ini.save()?;
    let reaper = Reaper::get();
    reaper.medium_reaper().update_arrange();
    reaper.medium_reaper().update_timeline();
    Ok(())
}

pub struct MenuIni {
    ini: Ini,
    path: Utf8PathBuf,
}

impl MenuIni {
    pub fn load() -> anyhow::Result<Self> {
        let path = Reaper::get()
            .medium_reaper()
            .get_resource_path(|p| p.join("reaper-menu.ini"));
        let ini = Ini::load_from_file_opt(
            &path,
            ini::ParseOption {
                enabled_quote: false,
                enabled_escape: false,
            },
        )?;
        let menu_ini = Self { ini, path };
        Ok(menu_ini)
    }

    pub fn get_toolbar(&mut self, toolbar_name: &str) -> anyhow::Result<Toolbar> {
        let properties = self
            .ini
            .section_mut(Some(toolbar_name))
            .context("finding section")?;
        Ok(Toolbar(properties))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.ini.write_to_file(&self.path)?;
        Ok(())
    }
}

pub struct Toolbar<'a>(&'a mut Properties);

impl<'a> Toolbar<'a> {
    pub fn items(&'a self) -> impl Iterator<Item = ToolbarItem<'a>> + 'a {
        self.0
            .iter()
            .filter_map(move |(key, value)| ToolbarItem::parse_from_ini_prop(self, key, value))
    }

    pub fn find_icon(&self, index: u32) -> Option<&str> {
        self.0.get(format!("icon_{index}"))
    }

    pub fn add_item(&mut self, item: ToolbarItem) {
        self.0.insert(
            format!("item_{}", item.index),
            format!("_{} {}", item.command, item.desc),
        );
        if let Some(icon) = item.icon {
            self.0.insert(format!("icon_{}", item.index), icon);
        }
    }
}

pub struct ToolbarItem<'a> {
    pub index: u32,
    pub command: &'a str,
    pub desc: &'a str,
    pub icon: Option<&'a str>,
}

impl<'a> ToolbarItem<'a> {
    fn parse_from_ini_prop(toolbar: &'a Toolbar, key: &'a str, value: &'a str) -> Option<Self> {
        let Some(("item", i)) = key.split_once('_') else {
            return None;
        };
        let (command, desc) = value.split_once(' ')?;
        let index = i.parse().ok()?;
        let icon = toolbar.find_icon(index);
        let item = ToolbarItem {
            index: index,
            command,
            desc,
            icon,
        };
        Some(item)
    }
}
