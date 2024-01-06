use std::fmt::Debug;

#[derive(Debug)]
pub enum Entry<R> {
    Menu(Menu<R>),
    Item(Item<R>),
    Separator,
    Nothing,
}

impl<R> Entry<R> {
    fn index_recursive(&mut self, counter: &mut Counter) {
        match self {
            Entry::Menu(m) => {
                m.id = counter.next_value();
                for e in &mut m.entries {
                    e.index_recursive(counter);
                }
            }
            Entry::Item(i) => {
                i.id = counter.next_value();
            }
            _ => {}
        }
    }

    fn find_item_by_id_recursive(self, id: u32) -> Option<Item<R>> {
        match self {
            Entry::Menu(m) => m
                .entries
                .into_iter()
                .find_map(|e| e.find_item_by_id_recursive(id)),
            Entry::Item(i) => {
                if i.id == id {
                    Some(i)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Menu<R> {
    pub id: u32,
    pub text: String,
    pub entries: Vec<Entry<R>>,
}

impl<R> Menu<R> {
    /// Returns next possible value.
    pub fn index(&mut self, first_id: u32) -> u32 {
        let mut counter = Counter::starting_from(first_id);
        for e in &mut self.entries {
            e.index_recursive(&mut counter);
        }
        counter.next_value()
    }

    pub fn find_item_by_id(self, id: u32) -> Option<Item<R>> {
        self.entries
            .into_iter()
            .find_map(|e| e.find_item_by_id_recursive(id))
    }
}

#[derive(Debug)]
pub struct Item<R> {
    pub id: u32,
    pub text: String,
    pub result: R,
    pub opts: ItemOpts,
}

pub fn root_menu<R>(entries: Vec<Entry<R>>) -> Menu<R> {
    Menu {
        id: 0,
        text: "".to_owned(),
        entries,
    }
}

pub fn menu<R>(text: impl Into<String>, entries: Vec<Entry<R>>) -> Entry<R> {
    Entry::Menu(Menu {
        id: 0,
        text: text.into(),
        entries,
    })
}

pub fn item<R>(text: impl Into<String>, result: R) -> Entry<R> {
    Entry::Item(Item {
        id: 0,
        text: text.into(),
        result,
        opts: Default::default(),
    })
}

pub fn separator<R>() -> Entry<R> {
    Entry::Separator
}

pub fn item_with_opts<R>(text: impl Into<String>, opts: ItemOpts, result: R) -> Entry<R> {
    Entry::Item(Item {
        id: 0,
        text: text.into(),
        result,
        opts,
    })
}

#[allow(clippy::redundant_closure)]
pub fn disabled_item<R: Default>(text: impl Into<String>) -> Entry<R> {
    item_with_opts(
        text,
        ItemOpts {
            enabled: false,
            checked: false,
        },
        R::default(),
    )
}

#[derive(Debug)]
pub struct ItemOpts {
    pub enabled: bool,
    pub checked: bool,
}

impl Default for ItemOpts {
    fn default() -> Self {
        Self {
            enabled: true,
            checked: false,
        }
    }
}

struct Counter {
    value: u32,
}

impl Counter {
    pub fn starting_from(value: u32) -> Self {
        Self { value }
    }

    pub fn next_value(&mut self) -> u32 {
        let val = self.value;
        self.value += 1;
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn basic() {
        // Given
        let a = Rc::new(RefCell::new(""));
        let a1 = a.clone();
        let a2 = a.clone();
        let a3 = a.clone();
        let a4 = a.clone();
        let a5 = a.clone();
        let mut devs_menu = root_menu(vec![
            item("<New>", move || *a1.borrow_mut() = "new"),
            menu(
                "Device 1",
                vec![
                    item("Edit...", move || *a2.borrow_mut() = "dev-1-edit"),
                    item_with_opts(
                        "Enabled",
                        ItemOpts {
                            enabled: false,
                            checked: true,
                        },
                        move || *a3.borrow_mut() = "dev-1-enabled",
                    ),
                ],
            ),
            menu(
                "Device 2",
                vec![
                    item("Edit...", move || *a4.borrow_mut() = "dev-2-edit"),
                    item_with_opts(
                        "Enabled",
                        ItemOpts {
                            enabled: false,
                            checked: true,
                        },
                        move || *a5.borrow_mut() = "dev-2-enabled",
                    ),
                ],
            ),
        ]);
        // When
        devs_menu.index(50);
        // Then
        let edit_item = devs_menu.find_item_by_id(52).unwrap();
        assert_eq!(edit_item.text.as_str(), "Edit...");
        edit_item.invoke_handler();
        assert_eq!(*a.borrow(), "dev-1-edit");
    }
}
