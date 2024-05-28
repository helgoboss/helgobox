use crate::application::{UnitModel, WeakUnitModel};
use crate::domain::{
    compartment_param_index_iter, CompartmentKind, CompartmentParamIndex, CompartmentParams,
    ControlInput, FeedbackOutput, MappingId, MidiControlInput, MidiDestination, OscDeviceId,
    ReaperTargetType, TargetSection,
};
use crate::infrastructure::data::{CommonPresetInfo, OscDevice};
use crate::infrastructure::plugin::{ActionSection, BackboneShell, ACTION_DEFS};
use crate::infrastructure::ui::Item;
use camino::Utf8Path;
use indexmap::IndexMap;
use reaper_high::{FxChainContext, MidiInputDevice, MidiOutputDevice, Reaper};

use base::hash_util::NonCryptoIndexMap;
use derive_more::Display;
use realearn_api::persistence::VirtualControlElementCharacter;
use reaper_medium::ReaperString;
use std::iter;
use strum::IntoEnumIterator;
use swell_ui::menu_tree::{
    anonymous_menu, item, item_with_opts, menu, separator, Entry, ItemOpts, Menu,
};

pub enum ControlInputMenuAction {
    SelectControlInput(ControlInput),
    ManageOsc(OscDeviceManagementAction),
}

pub fn control_input_menu(current_value: ControlInput) -> Menu<ControlInputMenuAction> {
    let fx_input = ControlInput::Midi(MidiControlInput::FxInput);
    let (open_midi_devs, closed_midi_devs): (Vec<_>, Vec<_>) = Reaper::get()
        .midi_input_devices()
        .filter(|d| d.is_available())
        .partition(|dev| dev.is_open());
    let osc_device_manager = BackboneShell::get().osc_device_manager();
    let osc_device_manager = osc_device_manager.borrow();
    let (open_osc_devs, closed_osc_devs): (Vec<_>, Vec<_>) = {
        osc_device_manager
            .devices()
            .partition(|dev| dev.input_status().is_connected())
    };
    let entries = iter::once(item_with_opts(
        CONTROL_INPUT_MIDI_FX_INPUT_LABEL,
        ItemOpts {
            enabled: true,
            checked: current_value == fx_input,
        },
        ControlInputMenuAction::SelectControlInput(fx_input),
    ))
    .chain(
        open_midi_devs
            .into_iter()
            .map(|dev| build_midi_input_dev_menu_item(dev, current_value)),
    )
    .chain(iter::once(menu(
        "Unavailable MIDI input devices",
        closed_midi_devs
            .into_iter()
            .map(|dev| build_midi_input_dev_menu_item(dev, current_value))
            .collect(),
    )))
    .chain(iter::once(separator()))
    .chain(
        open_osc_devs
            .into_iter()
            .map(|dev| build_osc_input_dev_menu_item(dev, current_value)),
    )
    .chain(iter::once(menu(
        "Unavailable OSC devices",
        closed_osc_devs
            .into_iter()
            .map(|dev| build_osc_input_dev_menu_item(dev, current_value))
            .collect(),
    )))
    .chain(iter::once(menu(
        "Manage OSC devices",
        osc_device_management_menu_entries(ControlInputMenuAction::ManageOsc),
    )))
    .chain(iter::once(separator()))
    .chain(iter::once(item_with_opts(
        CONTROL_INPUT_KEYBOARD_LABEL,
        ItemOpts {
            enabled: true,
            checked: current_value == ControlInput::Keyboard,
        },
        ControlInputMenuAction::SelectControlInput(ControlInput::Keyboard),
    )));
    anonymous_menu(entries.collect())
}

pub enum FeedbackOutputMenuAction {
    SelectFeedbackOutput(Option<FeedbackOutput>),
    ManageOsc(OscDeviceManagementAction),
}

pub fn feedback_output_menu(
    current_value: Option<FeedbackOutput>,
) -> Menu<FeedbackOutputMenuAction> {
    let fx_output = Some(FeedbackOutput::Midi(MidiDestination::FxOutput));
    let (open_midi_devs, closed_midi_devs): (Vec<_>, Vec<_>) = Reaper::get()
        .midi_output_devices()
        .filter(|d| d.is_available())
        .partition(|dev| dev.is_open());
    let osc_device_manager = BackboneShell::get().osc_device_manager();
    let osc_device_manager = osc_device_manager.borrow();
    let (open_osc_devs, closed_osc_devs): (Vec<_>, Vec<_>) = {
        osc_device_manager
            .devices()
            .partition(|dev| dev.input_status().is_connected())
    };
    let entries = iter::once(item_with_opts(
        FEEDBACK_OUTPUT_NONE_LABEL,
        ItemOpts {
            enabled: true,
            checked: current_value.is_none(),
        },
        FeedbackOutputMenuAction::SelectFeedbackOutput(None),
    ))
    .chain(iter::once(separator()))
    .chain(iter::once(item_with_opts(
        FEEDBACK_OUTPUT_MIDI_FX_OUTPUT,
        ItemOpts {
            enabled: true,
            checked: current_value == fx_output,
        },
        FeedbackOutputMenuAction::SelectFeedbackOutput(fx_output),
    )))
    .chain(
        open_midi_devs
            .into_iter()
            .map(|dev| build_midi_output_dev_menu_item(dev, current_value)),
    )
    .chain(iter::once(menu(
        "Unavailable MIDI output devices",
        closed_midi_devs
            .into_iter()
            .map(|dev| build_midi_output_dev_menu_item(dev, current_value))
            .collect(),
    )))
    .chain(iter::once(separator()))
    .chain(
        open_osc_devs
            .into_iter()
            .map(|dev| build_osc_output_dev_menu_item(dev, current_value)),
    )
    .chain(iter::once(menu(
        "Unavailable OSC devices",
        closed_osc_devs
            .into_iter()
            .map(|dev| build_osc_output_dev_menu_item(dev, current_value))
            .collect(),
    )))
    .chain(iter::once(menu(
        "Manage OSC devices",
        osc_device_management_menu_entries(FeedbackOutputMenuAction::ManageOsc),
    )))
    .chain(iter::once(separator()));
    anonymous_menu(entries.collect())
}

pub const CONTROL_INPUT_MIDI_FX_INPUT_LABEL: &str = "MIDI: <FX input>";
pub const CONTROL_INPUT_KEYBOARD_LABEL: &str = "Computer keyboard";
pub const FEEDBACK_OUTPUT_MIDI_FX_OUTPUT: &str = "MIDI: <FX output>";
pub const FEEDBACK_OUTPUT_NONE_LABEL: &str = "<None>";

fn build_midi_input_dev_menu_item(
    dev: MidiInputDevice,
    current_value: ControlInput,
) -> Entry<ControlInputMenuAction> {
    let control_input = ControlInput::Midi(MidiControlInput::Device(dev.id()));
    item_with_opts(
        get_midi_input_device_list_label(dev),
        ItemOpts {
            enabled: true,
            checked: current_value == control_input,
        },
        ControlInputMenuAction::SelectControlInput(control_input),
    )
}

fn build_midi_output_dev_menu_item(
    dev: MidiOutputDevice,
    current_value: Option<FeedbackOutput>,
) -> Entry<FeedbackOutputMenuAction> {
    let feedback_output = Some(FeedbackOutput::Midi(MidiDestination::Device(dev.id())));
    item_with_opts(
        get_midi_output_device_list_label(dev),
        ItemOpts {
            enabled: true,
            checked: current_value == feedback_output,
        },
        FeedbackOutputMenuAction::SelectFeedbackOutput(feedback_output),
    )
}

fn build_osc_input_dev_menu_item(
    dev: &OscDevice,
    current_value: ControlInput,
) -> Entry<ControlInputMenuAction> {
    let control_input = ControlInput::Osc(*dev.id());
    item_with_opts(
        get_osc_device_list_label(dev, false),
        ItemOpts {
            enabled: true,
            checked: current_value == control_input,
        },
        ControlInputMenuAction::SelectControlInput(control_input),
    )
}

fn build_osc_output_dev_menu_item(
    dev: &OscDevice,
    current_value: Option<FeedbackOutput>,
) -> Entry<FeedbackOutputMenuAction> {
    let feedback_output = Some(FeedbackOutput::Osc(*dev.id()));
    item_with_opts(
        get_osc_device_list_label(dev, true),
        ItemOpts {
            enabled: true,
            checked: current_value == feedback_output,
        },
        FeedbackOutputMenuAction::SelectFeedbackOutput(feedback_output),
    )
}

pub fn get_osc_device_list_label(dev: &OscDevice, is_output: bool) -> String {
    format!("OSC: {}", dev.get_list_label(is_output))
}

pub fn get_midi_input_device_list_label(dev: MidiInputDevice) -> String {
    get_midi_device_list_label(
        dev.name().unwrap_or_default(),
        dev.id().get(),
        MidiDeviceStatus::from_flags(dev.is_open(), dev.is_connected()),
    )
}

pub fn get_midi_output_device_list_label(dev: MidiOutputDevice) -> String {
    get_midi_device_list_label(
        dev.name().unwrap_or_default(),
        dev.id().get(),
        MidiDeviceStatus::from_flags(dev.is_open(), dev.is_connected()),
    )
}

fn get_midi_device_list_label(name: ReaperString, raw_id: u8, status: MidiDeviceStatus) -> String {
    format!(
        "MIDI: [{}] {}{}",
        raw_id,
        // Here we don't rely on the string to be UTF-8 because REAPER doesn't have influence on
        // how MIDI devices encode their name. Indeed a user reported an error related to that:
        // https://github.com/helgoboss/realearn/issues/78
        name.into_inner().to_string_lossy(),
        status
    )
}

pub fn extension_menu() -> Menu<&'static str> {
    anonymous_menu(vec![menu("Helgobox", helgbox_menu_entries().collect())])
}

fn helgbox_menu_entries() -> impl Iterator<Item = Entry<&'static str>> {
    ActionSection::iter()
        .filter(|section| *section != ActionSection::General)
        .filter_map(|section| section.build_menu())
        .chain(iter::once(separator()))
        .chain(
            ACTION_DEFS
                .iter()
                .filter(|def| def.section == ActionSection::General && def.should_appear_in_menu())
                .map(|def| def.build_menu_item()),
        )
        .map(|mut entry| {
            assign_command_ids_recursively(&mut entry);
            entry
        })
}

pub fn reaper_target_type_menu(current_value: ReaperTargetType) -> Menu<ReaperTargetType> {
    let entries = TargetSection::iter().map(|section| {
        let sub_entries = ReaperTargetType::iter()
            .filter(|t| t.definition().section == section)
            .map(|t| {
                item_with_opts(
                    t.definition().name,
                    ItemOpts {
                        enabled: true,
                        checked: t == current_value,
                    },
                    t,
                )
            });
        menu(section.to_string(), sub_entries.collect())
    });
    anonymous_menu(entries.collect())
}

pub fn virtual_control_element_character_menu(
    current_value: VirtualControlElementCharacter,
) -> Menu<VirtualControlElementCharacter> {
    let entries = VirtualControlElementCharacter::iter().map(|t| {
        item_with_opts(
            t.to_string(),
            ItemOpts {
                enabled: true,
                checked: t == current_value,
            },
            t,
        )
    });
    anonymous_menu(entries.collect())
}

pub fn menu_containing_realearn_params(
    session: &WeakUnitModel,
    compartment: CompartmentKind,
    current_value: CompartmentParamIndex,
) -> Menu<CompartmentParamIndex> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    anonymous_menu(
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
    compartment: CompartmentKind,
    current_value: Option<CompartmentParamIndex>,
) -> Menu<Option<CompartmentParamIndex>> {
    let session = session.upgrade().expect("session gone");
    let session = session.borrow();
    anonymous_menu(
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
    compartment: CompartmentKind,
    current_value: Option<MappingId>,
) -> Menu<Option<MappingId>> {
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
    anonymous_menu(iter::once(none_item).chain(group_items).collect())
}

pub fn menu_containing_sessions(
    this_session: &UnitModel,
    current_other_session_id: Option<&str>,
) -> Menu<Option<String>> {
    let this_item = item_with_opts(
        THIS,
        ItemOpts {
            enabled: true,
            checked: current_other_session_id.is_none(),
        },
        None,
    );
    let items: Vec<_> = BackboneShell::get().with_unit_infos(|sessions| {
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
            let other_session_id = other_session.unit_key().to_string();
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
    anonymous_menu(items)
}

pub fn menu_containing_banks(
    session: &WeakUnitModel,
    compartment: CompartmentKind,
    param_index: CompartmentParamIndex,
    current_value: u32,
) -> Menu<u32> {
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
    anonymous_menu(menu_items)
}

pub fn get_optional_param_name(
    session: &UnitModel,
    compartment: CompartmentKind,
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
    compartment: CompartmentKind,
    current_value: Option<&str>,
) -> Menu<Option<String>> {
    let preset_manager = BackboneShell::get().compartment_preset_manager(compartment);
    let preset_manager = preset_manager.borrow();
    anonymous_menu(
        iter::once(item_with_opts(
            NONE,
            ItemOpts {
                enabled: true,
                checked: current_value.is_none(),
            },
            None,
        ))
        .chain(build_compartment_preset_menu_entries(
            preset_manager.common_preset_infos(),
            |info| Some(info.id.clone()),
            current_value,
        ))
        .collect(),
    )
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, derive_more::Display)]
enum PresetCategory<'a> {
    #[display(fmt = "Factory")]
    Factory,
    #[display(fmt = "User ({_0})")]
    UserSorted(&'a str),
    #[display(fmt = "User (Unsorted)")]
    UserUnsorted,
}

pub fn build_compartment_preset_menu_entries<'a, T: 'static>(
    preset_infos: impl Iterator<Item = &'a CommonPresetInfo> + 'a,
    build_id: impl Fn(&CommonPresetInfo) -> T + 'a,
    current_id: Option<&'a str>,
) -> impl Iterator<Item = Entry<T>> + 'a {
    let preset_infos: Vec<_> = preset_infos.collect();
    let mut categorized_infos: NonCryptoIndexMap<PresetCategory, Vec<&CommonPresetInfo>> =
        IndexMap::default();
    for info in preset_infos {
        let path = Utf8Path::new(&info.id);
        let category = if path.components().count() == 1 {
            // Preset directly in user preset root = "Unsorted"
            PresetCategory::UserUnsorted
        } else {
            // Preset in subdirectory (good)
            let component = path
                .components()
                .next()
                .expect("preset ID should have at least one component")
                .as_str();
            if component == "factory" {
                PresetCategory::Factory
            } else {
                PresetCategory::UserSorted(component)
            }
        };
        categorized_infos.entry(category).or_default().push(info);
    }
    categorized_infos.sort_keys();
    categorized_infos
        .into_iter()
        .map(move |(category, mut infos)| {
            infos.sort_by_key(|info| &info.meta_data.name);
            let entries = build_compartment_preset_menu_entries_internal(
                infos.into_iter(),
                &build_id,
                current_id,
            );
            let mut contains_current_entry = false;
            let entries = entries
                .inspect(|e| {
                    if let Entry::Item(item) = e {
                        if item.opts.checked {
                            contains_current_entry = true;
                        }
                    }
                })
                .collect();
            let category_suffix = if contains_current_entry { " *" } else { "" };
            let category_label = format!("{category}{category_suffix}");
            menu(category_label, entries)
        })
}

fn build_compartment_preset_menu_entries_internal<'a, T: 'static>(
    preset_infos: impl Iterator<Item = &'a CommonPresetInfo> + 'a,
    build_id: &'a (impl Fn(&CommonPresetInfo) -> T + 'a),
    current_id: Option<&'a str>,
) -> impl Iterator<Item = Entry<T>> + 'a {
    preset_infos.map(move |info| {
        let id = build_id(info);
        item_with_opts(
            info.meta_data.name.clone(),
            ItemOpts {
                enabled: true,
                checked: current_id.is_some_and(|id| id == info.id),
            },
            id,
        )
    })
}

pub enum OscDeviceManagementAction {
    EditNewOscDevice,
    EditExistingOscDevice(OscDeviceId),
    RemoveOscDevice(OscDeviceId),
    ToggleOscDeviceControl(OscDeviceId),
    ToggleOscDeviceFeedback(OscDeviceId),
    ToggleOscDeviceBundles(OscDeviceId),
}

fn osc_device_management_menu_entries<P>(
    build_payload: impl Fn(OscDeviceManagementAction) -> P,
) -> Vec<Entry<P>> {
    let dev_manager = BackboneShell::get().osc_device_manager();
    let dev_manager = dev_manager.borrow();
    iter::once(item(
        "<New>",
        build_payload(OscDeviceManagementAction::EditNewOscDevice),
    ))
    .chain(dev_manager.devices().map(|dev| {
        let dev_id = *dev.id();
        menu(
            dev.name(),
            vec![
                item(
                    "Edit...",
                    build_payload(OscDeviceManagementAction::EditExistingOscDevice(dev_id)),
                ),
                item(
                    "Remove",
                    build_payload(OscDeviceManagementAction::RemoveOscDevice(dev_id)),
                ),
                item_with_opts(
                    "Enabled for control",
                    ItemOpts {
                        enabled: true,
                        checked: dev.is_enabled_for_control(),
                    },
                    build_payload(OscDeviceManagementAction::ToggleOscDeviceControl(dev_id)),
                ),
                item_with_opts(
                    "Enabled for feedback",
                    ItemOpts {
                        enabled: true,
                        checked: dev.is_enabled_for_feedback(),
                    },
                    build_payload(OscDeviceManagementAction::ToggleOscDeviceFeedback(dev_id)),
                ),
                item_with_opts(
                    "Can deal with OSC bundles",
                    ItemOpts {
                        enabled: true,
                        checked: dev.can_deal_with_bundles(),
                    },
                    build_payload(OscDeviceManagementAction::ToggleOscDeviceBundles(dev_id)),
                ),
            ],
        )
    }))
    .collect()
}

pub const NONE: &str = "<None>";
pub const THIS: &str = "<This>";

/// Interprets the item payloads as REAPER command names and uses them to lookup the corresponding REAPER command
/// IDs.
///
/// This is useful for top-level menus.
pub fn assign_command_ids_recursively(entry: &mut Entry<&'static str>) {
    match entry {
        Entry::Menu(m) => {
            m.id = 0;
            for e in &mut m.entries {
                assign_command_ids_recursively(e);
            }
        }
        Entry::Item(i) => {
            let command_id = Reaper::get()
                .medium_reaper()
                .named_command_lookup(format!("_{}", i.result));
            i.id = command_id.map(|id| id.get()).unwrap_or(0);
        }
        _ => {}
    }
}

#[derive(Display)]
enum MidiDeviceStatus {
    #[display(fmt = " <disconnected>")]
    Disconnected,
    #[display(fmt = " <connected but disabled>")]
    ConnectedButDisabled,
    #[display(fmt = "")]
    Connected,
}

impl MidiDeviceStatus {
    fn from_flags(open: bool, connected: bool) -> MidiDeviceStatus {
        use MidiDeviceStatus::*;
        match (open, connected) {
            (false, false) => Disconnected,
            (false, true) => ConnectedButDisabled,
            // Shouldn't happen but cope with it.
            (true, false) => Disconnected,
            (true, true) => Connected,
        }
    }
}
