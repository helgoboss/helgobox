use crate::application::{Session, WeakSession};
use crate::domain::{compartment_param_index_iter, Compartment, CompartmentParamIndex, MappingId};
use crate::infrastructure::ui::Item;
use std::iter;
use swell_ui::menu_tree::{item_with_opts, menu, root_menu, Entry, ItemOpts};

pub fn menu_containing_realearn_params(
    session: &WeakSession,
    compartment: Compartment,
    current_value: CompartmentParamIndex,
) -> swell_ui::menu_tree::Menu<CompartmentParamIndex> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    root_menu(
        compartment_param_index_iter()
            .map(|i| {
                let param_name = get_param_name(&session, compartment, Some(i));
                item_with_opts(
                    param_name,
                    ItemOpts {
                        enabled: true,
                        checked: i == current_value,
                    },
                    move || i,
                )
            })
            .collect(),
    )
}

pub fn menu_containing_realearn_params_optional(
    session: &WeakSession,
    compartment: Compartment,
    current_value: Option<CompartmentParamIndex>,
) -> swell_ui::menu_tree::Menu<Option<CompartmentParamIndex>> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    root_menu(
        iter::once(item_with_opts(
            NONE,
            ItemOpts {
                enabled: true,
                checked: current_value.is_none(),
            },
            || None,
        ))
        .chain(compartment_param_index_iter().map(|i| {
            let value = Some(i);
            let param_name = get_param_name(&session, compartment, value);
            item_with_opts(
                param_name,
                ItemOpts {
                    enabled: true,
                    checked: value == current_value,
                },
                move || value,
            )
        }))
        .collect(),
    )
}

pub fn menu_containing_mappings(
    session: &WeakSession,
    compartment: Compartment,
    current_value: Option<MappingId>,
) -> swell_ui::menu_tree::Menu<Option<MappingId>> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    let none_item = item_with_opts(
        NONE,
        ItemOpts {
            enabled: true,
            checked: current_value.is_none(),
        },
        || None,
    );
    let group_items = session.groups_sorted(compartment).map(|group| {
        let group = group.borrow();
        let group_id = group.id();
        menu(
            group.effective_name(),
            session
                .mappings(compartment)
                .filter_map(|mapping| {
                    // If borrowing fails, we know it's our own mapping
                    let mapping = mapping.try_borrow().ok()?;
                    if mapping.group_id() != group_id {
                        return None;
                    }
                    let mapping_id = mapping.id();
                    let menu_item = item_with_opts(
                        mapping.effective_name(),
                        ItemOpts {
                            enabled: true,
                            checked: Some(mapping_id) == current_value,
                        },
                        move || Some(mapping_id),
                    );
                    Some(menu_item)
                })
                .collect(),
        )
    });
    root_menu(iter::once(none_item).chain(group_items).collect())
}

pub fn menu_containing_banks(
    session: &WeakSession,
    compartment: Compartment,
    param_index: CompartmentParamIndex,
    current_value: u32,
) -> swell_ui::menu_tree::Menu<u32> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    let bank_param = session
        .params()
        .compartment_params(compartment)
        .at(param_index);
    let menu_items = if let Some(discrete_values) = bank_param.setting().discrete_values() {
        discrete_values
            .enumerate()
            // Don't block GUI if we come across a parameter that has insanely many
            // discrete values (and is probably not intended to be used with banks).
            .take(500)
            .map(|(i, s)| bank_item(s.to_string(), i, current_value))
            .collect()
    } else {
        // For continuous parameters we just choose a default of 100 values.
        let bank_count = 100;
        (0..bank_count)
            .map(|i| bank_item(i.to_string(), i, current_value))
            .collect()
    };
    root_menu(menu_items)
}

pub fn get_param_name(
    session: &Session,
    compartment: Compartment,
    index: Option<CompartmentParamIndex>,
) -> String {
    match index {
        None => "<None>".to_owned(),
        Some(i) => {
            let param_name = session
                .params()
                .compartment_params(compartment)
                .get_parameter_name(i);
            format!("{}. {}", i.get() + 1, param_name)
        }
    }
}

pub fn get_bank_name(
    session: &Session,
    item: &dyn Item,
    bank_param_index: CompartmentParamIndex,
    bank_index: u32,
) -> String {
    let bank_param = session
        .params()
        .compartment_params(item.compartment())
        .at(bank_param_index);
    if let Some(label) = bank_param.setting().find_label_for_value(bank_index) {
        label.to_owned()
    } else {
        bank_index.to_string()
    }
}

fn bank_item(text: String, bank_index: usize, current_bank_index: u32) -> Entry<u32> {
    item_with_opts(
        text,
        ItemOpts {
            enabled: true,
            checked: bank_index == current_bank_index as usize,
        },
        move || bank_index as u32,
    )
}

pub const NONE: &str = "<None>";
