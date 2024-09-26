use crate::menu_tree::{Entry, Menu};
use crate::Menu as SwellMenu;

/// Adds all entries within the given pure menu to the given SWELL menu, ignoring the label of the pure menu.
///
/// Also adds a separator first if the SWELL menu already contains entries.
pub fn add_all_entries_of_menu<R>(swell_menu: SwellMenu, root_menu: &Menu<R>) {
    // Add separator if there are entries already
    if swell_menu.entry_count().is_ok_and(|count| count > 0) {
        swell_menu.add_separator();
    }
    // Add entries
    for e in &root_menu.entries {
        fill_menu_recursively(swell_menu, e);
    }
}

/// Adds the given menu entry and potential sub entries to the given SWELL menu.
fn fill_menu_recursively<R>(swell_menu: SwellMenu, entry: &Entry<R>) {
    match entry {
        Entry::Menu(m) => {
            let sub_menu = swell_menu.add_menu(m.text.as_str());
            for e in &m.entries {
                fill_menu_recursively(sub_menu, e);
            }
        }
        Entry::Item(i) => {
            swell_menu.add_item(i.id, i.text.as_str());
            if i.opts.checked {
                swell_menu.set_item_checked(i.id, true);
            }
            if !i.opts.enabled {
                swell_menu.set_item_enabled(i.id, false);
            }
        }
        Entry::Separator(s) => {
            swell_menu.add_separator();
            if let Some(text) = &s.text {
                swell_menu.add_item(s.id, format!("{text}"));
                swell_menu.set_item_enabled(s.id, false);
                swell_menu.add_separator();
            }
        }
        Entry::Nothing => {}
    }
}
