use crate::SwellStringArg;
use reaper_low::{raw, Swell};
use std::marker::PhantomData;

/// Represents a top-level menu bar with resource management.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct MenuBar {
    raw: raw::HMENU,
}

impl MenuBar {
    pub fn new_popup_menu() -> MenuBar {
        Self {
            raw: Swell::get().CreatePopupMenu(),
        }
    }

    pub fn load(resource_id: u32) -> Result<MenuBar, &'static str> {
        let swell = Swell::get();
        let raw = unsafe {
            swell.LoadMenu(
                swell.plugin_context().h_instance(),
                resource_id as u16 as raw::ULONG_PTR as raw::LPSTR,
            )
        };
        if raw.is_null() {
            return Err("couldn't load menu");
        }
        Ok(MenuBar { raw })
    }

    pub fn menu(&self) -> Menu {
        Menu::new(self.raw)
    }

    pub fn get_sub_menu(&self, index: u32) -> Option<Menu> {
        get_sub_menu_at(self.raw, index)
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        unsafe {
            Swell::get().DestroyMenu(self.raw);
        }
    }
}

/// Represents a menu or submenu.
///
/// Doesn't need to implement Drop because Windows will destroy all sub menus automatically
/// when the root menu is destroyed.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Menu<'a> {
    raw: raw::HMENU,
    p: PhantomData<&'a ()>,
}

impl<'a> Menu<'a> {
    fn new(raw: raw::HMENU) -> Self {
        Self {
            raw,
            p: Default::default(),
        }
    }

    pub fn raw(self) -> raw::HMENU {
        self.raw
    }

    pub fn set_item_checked(self, item_id: u32, checked: bool) {
        unsafe {
            Swell::get().CheckMenuItem(
                self.raw,
                item_id as _,
                if checked {
                    raw::MF_CHECKED
                } else {
                    raw::MF_UNCHECKED
                } as _,
            );
        }
    }

    pub fn add_item<'b>(self, item_id: u32, text: impl Into<SwellStringArg<'b>>) {
        unsafe {
            let swell = Swell::get();
            let swell_string_arg = text.into();
            let mut mi = raw::MENUITEMINFO {
                fMask: raw::MIIM_TYPE | raw::MIIM_DATA | raw::MIIM_ID,
                wID: item_id,
                dwTypeData: swell_string_arg.as_ptr() as _,
                ..Default::default()
            };
            swell.InsertMenuItem(self.raw, -1, 1, &mut mi as _);
        }
    }

    pub fn add_separator(self) {
        unsafe {
            let swell = Swell::get();
            let mut mi = raw::MENUITEMINFO {
                fMask: raw::MIIM_TYPE,
                fType: raw::MF_SEPARATOR,
                ..Default::default()
            };
            swell.InsertMenuItem(self.raw, -1, 1, &mut mi as _);
        }
    }

    pub fn get_sub_menu_at(&self, index: u32) -> Option<Menu> {
        get_sub_menu_at(self.raw, index)
    }

    pub fn turn_into_submenu(&self, item_id: u32) -> Menu {
        let sub_menu = Swell::get().CreatePopupMenu();
        let mut mi = raw::MENUITEMINFO {
            fMask: raw::MIIM_SUBMENU,
            hSubMenu: sub_menu,
            ..Default::default()
        };
        unsafe {
            Swell::get().SetMenuItemInfo(self.raw, item_id as _, 0, &mut mi as _);
        }
        Menu::new(sub_menu)
    }

    pub fn set_item_text<'b>(self, item_id: u32, text: impl Into<SwellStringArg<'b>>) {
        unsafe {
            let swell_string_arg = text.into();
            let mut mi = raw::MENUITEMINFO {
                fMask: raw::MIIM_TYPE | raw::MIIM_DATA,
                dwTypeData: swell_string_arg.as_ptr() as _,
                ..Default::default()
            };
            Swell::get().SetMenuItemInfo(self.raw, item_id as _, 0, &mut mi as _);
        }
    }

    pub fn set_item_enabled(self, item_id: u32, enabled: bool) {
        unsafe {
            Swell::get().EnableMenuItem(
                self.raw,
                item_id as _,
                if enabled {
                    raw::MF_ENABLED
                } else {
                    raw::MF_GRAYED
                } as _,
            );
        }
    }
}

fn get_sub_menu_at<'a>(raw: raw::HMENU, index: u32) -> Option<Menu<'a>> {
    let menu = unsafe { Swell::get().GetSubMenu(raw, index as _) };
    if menu.is_null() {
        return None;
    }
    Some(Menu::new(menu))
}
