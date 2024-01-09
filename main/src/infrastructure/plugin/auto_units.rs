use crate::application::{
    AutoUnitData, ControllerPresetUsage, ControllerSuitability, MainPresetSuitability,
};
use crate::domain::{DeviceControlInput, DeviceFeedbackOutput, OscDeviceId};
use crate::infrastructure::data::PresetInfo;
use crate::infrastructure::plugin::BackboneShell;
use anyhow::Context;
use realearn_api::persistence::{
    Controller, ControllerConnection, ControllerPresetMetaData, MainPresetMetaData,
};
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::str::FromStr;

pub fn determine_auto_units() -> Vec<AutoUnitData> {
    let controller_manager = BackboneShell::get().controller_manager().borrow();
    let controllers = &controller_manager.controller_config().controllers;
    controllers
        .iter()
        .filter_map(build_auto_unit_from_controller)
        .collect()
}

fn build_auto_unit_from_controller(controller: &Controller) -> Option<AutoUnitData> {
    // Ignore if no connection info or no main preset
    let connection = controller.connection.as_ref()?;
    let main_preset_id = controller.default_main_preset.as_ref()?;
    let main_preset_manager = BackboneShell::get().main_preset_manager().borrow();
    let main_preset_info = main_preset_manager.find_preset_info_by_id(main_preset_id.get())?;
    // Choose suitable controller preset
    let default_controller_preset_id = controller
        .default_controller_preset
        .as_ref()
        .map(|id| id.get().to_string());
    let controller_preset_usage = choose_suitable_controller_preset_id(
        connection,
        main_preset_info,
        default_controller_preset_id,
    )
    .ok()?;
    // Translate connection info
    let (input, output) = translate_connection_info(connection);
    // Ignore if neither input nor output given
    if input.is_none() && output.is_none() {
        return None;
    }
    // Ignore if input not connected
    if let Some(input) = input {
        if !input_is_connected(input) {
            return None;
        }
    }
    // Ignore if output not connected
    if let Some(output) = output {
        if !output_is_connected(output) {
            return None;
        }
    }
    // Build data
    let data = AutoUnitData {
        controller_id: controller.id.clone(),
        controller_preset_usage,
        input,
        output,
        main_preset_id: main_preset_id.get().to_string(),
    };
    Some(data)
}

fn choose_suitable_controller_preset_id(
    connection: &ControllerConnection,
    main_preset_info: &PresetInfo<MainPresetMetaData>,
    default_controller_preset_id: Option<String>,
) -> anyhow::Result<Option<ControllerPresetUsage>> {
    let controller_preset_manager = BackboneShell::get().controller_preset_manager().borrow();
    if let Some(id) = &default_controller_preset_id {
        // Check if default preset meets main preset requirements. Then simply use that one.
        let controller_preset_info = controller_preset_manager
            .find_preset_info_by_id(&id)
            .context("default controller preset not found")?;
        let main_preset_suitability = get_suitability_of_controller_preset_for_main_preset(
            &controller_preset_info.specific_meta_data,
            &main_preset_info.specific_meta_data,
        );
        if main_preset_suitability.is_generally_suitable() {
            let usage = ControllerPresetUsage {
                controller_preset_id: id.clone(),
                main_preset_suitability: Some(main_preset_suitability),
                // The user should make sure that it suites his controller, this won't be checked.
                controller_suitability: None,
            };
            return Ok(Some(usage));
        }
    }
    // Default controller preset not given or doesn't meet requirements. Search for a suitable one.
    let chosen_candidate = controller_preset_manager
        .preset_infos()
        .iter()
        .filter_map(|info| {
            let main_preset_suitability = get_suitability_of_controller_preset_for_main_preset(
                &info.specific_meta_data,
                &main_preset_info.specific_meta_data,
            );
            if !main_preset_suitability.is_generally_suitable() {
                return None;
            }
            Some((info, main_preset_suitability))
        })
        // We have a list of controller presets that are suitable in terms of main preset
        // requirements. Now we need to look for the ones that match our controller.
        .filter_map(|(info, main_preset_suitability)| {
            let controller_suitability = get_suitability_of_controller_preset_for_controller(
                &info.specific_meta_data,
                connection,
            );
            if controller_suitability == ControllerSuitability::NotSuitable {
                return None;
            }
            Some((info, main_preset_suitability, controller_suitability))
        })
        // We have a list of candidates. Now do the ranking. Take the one with the highest combined
        // suitability.
        .max_by_key(|(_, main_preset_suitability, controller_suitability)| {
            (*main_preset_suitability, *controller_suitability)
        });
    let usage = chosen_candidate
        .map(
            |(info, main_preset_suitability, controller_suitability)| ControllerPresetUsage {
                controller_preset_id: info.common.id.clone(),
                main_preset_suitability: Some(main_preset_suitability),
                controller_suitability: Some(controller_suitability),
            },
        )
        .or_else(|| {
            // No suitable preset found. Not good but maybe the default controller preset just misses
            // the necessary meta data. So choose that one and hope that it works.
            let controller_preset_id = default_controller_preset_id?;
            let usage = ControllerPresetUsage {
                controller_preset_id,
                main_preset_suitability: None,
                controller_suitability: None,
            };
            Some(usage)
        });
    Ok(usage)
}

/// The returned number is the number of matching schemes. The higher the number, the more suitable.
fn get_suitability_of_controller_preset_for_main_preset(
    controller_preset_meta_data: &ControllerPresetMetaData,
    main_preset_meta_data: &MainPresetMetaData,
) -> MainPresetSuitability {
    let intersection_count = main_preset_meta_data
        .used_schemes
        .intersection(&controller_preset_meta_data.provided_schemes)
        .count();
    MainPresetSuitability::new(intersection_count as u8)
}

fn get_suitability_of_controller_preset_for_controller(
    controller_preset_meta_data: &ControllerPresetMetaData,
    connection: &ControllerConnection,
) -> ControllerSuitability {
    match &controller_preset_meta_data.midi_identity_pattern {
        None => {
            // Controller preset doesn't define any pattern, which could just be laziness
            ControllerSuitability::MaybeSuitable
        }
        Some(pattern) => {
            match connection {
                ControllerConnection::Midi(c) => match &c.identity_response {
                    None => ControllerSuitability::MaybeSuitable,
                    Some(identity_response) => {
                        // TODO-high CONTINUE This should be a pattern, not an exact match
                        if identity_response == pattern {
                            ControllerSuitability::Suitable
                        } else {
                            ControllerSuitability::NotSuitable
                        }
                    }
                },
                ControllerConnection::Osc(_) => ControllerSuitability::NotSuitable,
            }
        }
    }
}

fn translate_connection_info(
    connection: &ControllerConnection,
) -> (Option<DeviceControlInput>, Option<DeviceFeedbackOutput>) {
    match connection {
        ControllerConnection::Midi(c) => {
            let input = c
                .input_port
                .map(|p| DeviceControlInput::Midi(MidiInputDeviceId::new(p.get() as u8)));
            let output = c
                .output_port
                .map(|p| DeviceFeedbackOutput::Midi(MidiOutputDeviceId::new(p.get() as u8)));
            (input, output)
        }
        ControllerConnection::Osc(c) => {
            let dev_id = c
                .osc_device_id
                .as_ref()
                .and_then(|id| OscDeviceId::from_str(id.get()).ok());
            if let Some(dev_id) = dev_id {
                (
                    Some(DeviceControlInput::Osc(dev_id)),
                    Some(DeviceFeedbackOutput::Osc(dev_id)),
                )
            } else {
                (None, None)
            }
        }
    }
}

fn input_is_connected(input: DeviceControlInput) -> bool {
    match input {
        DeviceControlInput::Midi(id) => MidiInputDevice::new(id).is_connected(),
        DeviceControlInput::Osc(id) => osc_device_is_connected(id),
    }
}

fn output_is_connected(output: DeviceFeedbackOutput) -> bool {
    match output {
        DeviceFeedbackOutput::Midi(id) => MidiOutputDevice::new(id).is_connected(),
        DeviceFeedbackOutput::Osc(id) => osc_device_is_connected(id),
    }
}

fn osc_device_is_connected(_dev_id: OscDeviceId) -> bool {
    // No easy way to check with OSC. Just return true for now.
    true
}
