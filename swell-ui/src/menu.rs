use crate::SwellStringArg;
use reaper_low::{raw, Swell};
use std::marker::PhantomData;
use std::ptr::null_mut;

/// Represents a top-level menu bar with resource management.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct MenuBar {
    raw: raw::HMENU,
}

impl MenuBar {
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

    pub fn get_menu(&self, index: u32) -> Option<Menu> {
        let menu = unsafe { Swell::get().GetSubMenu(self.raw, index as _) };
        if menu.is_null() {
            return None;
        }
        Some(Menu {
            raw: menu,
            p: PhantomData,
        })
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
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct Menu<'a> {
    raw: raw::HMENU,
    p: PhantomData<&'a ()>,
}

impl<'a> Menu<'a> {
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
                cbSize: 0,
                fMask: raw::MIIM_TYPE | raw::MIIM_DATA | raw::MIIM_ID,
                fType: 0,
                fState: 0,
                wID: item_id,
                hSubMenu: null_mut(),
                hbmpChecked: null_mut(),
                hbmpUnchecked: null_mut(),
                dwItemData: 0,
                dwTypeData: swell_string_arg.as_ptr() as _,
                cch: 0,
                hbmpItem: null_mut(),
            };
            swell.InsertMenuItem(self.raw, -1, 1, &mut mi as _);
        }
    }

    pub fn set_item_text<'b>(self, item_id: u32, text: impl Into<SwellStringArg<'b>>) {
        unsafe {
            let mut mi = raw::MENUITEMINFO {
                cbSize: 0,
                fMask: raw::MIIM_TYPE | raw::MIIM_DATA,
                fType: 0,
                fState: 0,
                wID: 0,
                hSubMenu: null_mut(),
                hbmpChecked: null_mut(),
                hbmpUnchecked: null_mut(),
                dwItemData: 0,
                dwTypeData: null_mut(),
                cch: 0,
                hbmpItem: null_mut(),
            };
            let swell = Swell::get();
            swell.GetMenuItemInfo(self.raw, item_id as _, 0, &mut mi as _);
            let swell_string_arg = text.into();
            mi.dwTypeData = swell_string_arg.as_ptr() as _;
            swell.SetMenuItemInfo(self.raw, item_id as _, 0, &mut mi as _);
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
