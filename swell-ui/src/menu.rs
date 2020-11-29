use reaper_low::{raw, Swell};
use std::marker::PhantomData;

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
}
