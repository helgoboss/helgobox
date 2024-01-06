use crate::application::{UnitModel, WeakUnitModel};
use crate::domain::{
    compartment_param_index_iter, Compartment, CompartmentParamIndex, CompartmentParams, MappingId,
};
use crate::infrastructure::data::PresetInfo;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::Item;
use reaper_high::FxChainContext;
use std::iter;
use swell_ui::menu_tree::{item_with_opts, menu, root_menu, Entry, ItemOpts};

pub fn menu_containing_realearn_params(
    session: &WeakUnitModel,
    compartment: Compartment,
    current_value: CompartmentParamIndex,
) -> swell_ui::menu_tree::Menu<CompartmentParamIndex> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    root_menu(
        compartment_param_index_iter()
            .map(|i| {
                let param_name = get_optional_param_name(&session, compartment, Some(i));
                item_with_opts(
                    param_name,
                    ItemOpts {
                        enabled: true,
                        checked: i == current_value,
                    },
                    i,
                )
            })
            .collect(),
    )
}

pub fn menu_containing_realearn_params_optional(
    session: &WeakUnitModel,
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
            None,
        ))
        .chain(compartment_param_index_iter().map(|i| {
            let value = Some(i);
            let param_name = get_optional_param_name(&session, compartment, value);
            item_with_opts(
                param_name,
                ItemOpts {
                    enabled: true,
                    checked: value == current_value,
                },
                value,
            )
        }))
        .collect(),
    )
}

pub fn menu_containing_mappings(
    session: &WeakUnitModel,
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
        None,
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
                        Some(mapping_id),
                    );
                    Some(menu_item)
                })
                .collect(),
        )
    });
    root_menu(iter::once(none_item).chain(group_items).collect())
}

pub fn menu_containing_sessions(
    this_session: &UnitModel,
    current_other_session_id: Option<&str>,
) -> swell_ui::menu_tree::Menu<Option<String>> {
    let this_item = item_with_opts(
        THIS,
        ItemOpts {
            enabled: true,
            checked: current_other_session_id.is_none(),
        },
        None,
    );
    let items: Vec<_> = BackboneShell::get().with_units(|sessions| {
        let instance_items = sessions.iter().filter_map(|session| {
            let other_session = session.unit_model.upgrade()?;
            let other_session = other_session.try_borrow().ok()?;
            // Exclude certain sessions
            if other_session.unit_id() == this_session.unit_id() {
                // Don't include our own session.
                return None;
            }
            let this_chain_context = this_session
                .processor_context()
                .containing_fx()
                .chain()
                .context();
            let other_chain_context = other_session
                .processor_context()
                .containing_fx()
                .chain()
                .context();
            match (this_chain_context, other_chain_context) {
                // It's okay if this instance is on the monitoring FX chain and the other one
                // as well (global => global).
                (FxChainContext::Monitoring, FxChainContext::Monitoring) => {}
                // It's okay if this instance is in a specific project and the other one on the
                // monitoring FX chain (local => global).
                (FxChainContext::Track { .. }, FxChainContext::Monitoring) => {}
                // It's okay if this instance is in a specific project and the other one in the same
                // project.
                (
                    FxChainContext::Track {
                        track: this_track, ..
                    },
                    FxChainContext::Track {
                        track: other_track, ..
                    },
                ) if other_track.project() == this_track.project() => {}
                // All other combinations are not allowed!
                _ => return None,
            }
            // Build item
            let other_session_id = other_session.id().to_string();
            let item = item_with_opts(
                other_session.to_string(),
                ItemOpts {
                    enabled: true,
                    checked: Some(other_session_id.as_str()) == current_other_session_id,
                },
                Some(other_session_id),
            );
            Some(item)
        });
        iter::once(this_item).chain(instance_items).collect()
    });
    root_menu(items)
}

pub fn menu_containing_banks(
    session: &WeakUnitModel,
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

pub fn get_optional_param_name(
    session: &UnitModel,
    compartment: Compartment,
    index: Option<CompartmentParamIndex>,
) -> String {
    match index {
        None => "<None>".to_owned(),
        Some(i) => {
            let params = session.params().compartment_params(compartment);
            get_param_name(params, i)
        }
    }
}

pub fn get_param_name(params: &CompartmentParams, index: CompartmentParamIndex) -> String {
    let param_name = params.get_parameter_name(index);
    format!("{}. {}", index.get() + 1, param_name)
}

pub fn get_bank_name(
    session: &UnitModel,
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
        bank_index as u32,
    )
}

pub fn menu_containing_compartment_presets(
    compartment: Compartment,
    current_value: Option<&str>,
) -> swell_ui::menu_tree::Menu<Option<String>> {
    let preset_manager = BackboneShell::get().preset_manager(compartment);
    let preset_manager = preset_manager.borrow();
    root_menu(
        iter::once(item_with_opts(
            NONE,
            ItemOpts {
                enabled: true,
                checked: current_value.is_none(),
            },
            None,
        ))
        .chain(build_compartment_preset_menu_entries(
            preset_manager.preset_infos(),
            |info| Some(info.id.clone()),
            |info| current_value.is_some_and(|id| id == &info.id),
        ))
        .collect(),
    )
}

pub fn build_compartment_preset_menu_entries<'a, T: 'static>(
    preset_infos: &'a [PresetInfo],
    build_id: impl Fn(&PresetInfo) -> T + 'a,
    is_current_value: impl Fn(&PresetInfo) -> bool + 'a,
) -> impl Iterator<Item = Entry<T>> + 'a {
    let (user_preset_infos, factory_preset_infos): (Vec<_>, Vec<_>) =
        preset_infos.iter().partition(|info| info.origin.is_user());
    [
        ("User presets", user_preset_infos),
        ("Factory presets", factory_preset_infos),
    ]
    .into_iter()
    .map(move |(label, mut infos)| {
        infos.sort_by_key(|info| &info.name);
        menu(
            label,
            build_compartment_preset_menu_entries_internal(
                infos.into_iter(),
                &build_id,
                &is_current_value,
            )
            .collect(),
        )
    })
}

fn build_compartment_preset_menu_entries_internal<'a, T: 'static>(
    preset_infos: impl Iterator<Item = &'a PresetInfo> + 'a,
    build_id: &'a (impl Fn(&PresetInfo) -> T + 'a),
    is_current_value: &'a (impl Fn(&PresetInfo) -> bool + 'a),
) -> impl Iterator<Item = Entry<T>> + 'a {
    preset_infos.map(move |info| {
        let id = build_id(info);
        let label = if info.name == info.id {
            info.name.clone()
        } else {
            format!("{} ({})", info.name, info.id)
        };
        item_with_opts(
            label,
            ItemOpts {
                enabled: true,
                checked: is_current_value(info),
            },
            id,
        )
    })
}

pub const NONE: &str = "<None>";
pub const THIS: &str = "<This>";
