use crate::application::{
    AutoUnitData, ControllerPresetUsage, ControllerSuitability, MainPresetSuitability,
};
use crate::base::notification::notify_user_on_anyhow_error;
use crate::domain::{parse_hex_string, DeviceControlInput, DeviceFeedbackOutput, OscDeviceId};
use crate::infrastructure::data::PresetInfo;
use crate::infrastructure::plugin::{BackboneShell, InstanceShellInfo};
use anyhow::Context;
use base::byte_pattern::BytePattern;
use base::hash_util::NonCryptoHashMap;
use base::Global;
use realearn_api::persistence::{
    Controller, ControllerConnection, ControllerPresetMetaData, MainPresetMetaData,
    MidiControllerConnection,
};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::cmp::Ordering;
use std::str::FromStr;
use wildmatch::WildMatch;

/// To be called whenever some event might lead to addition/removal of auto units.
pub fn update_auto_units_async() {
    Global::task_support()
        .do_later_in_main_thread_from_main_thread_asap(|| {
            update_auto_units();
        })
        .unwrap();
}

fn update_auto_units() {
    tracing::debug!("Updating auto units...");
    // Get a list of all enabled controllers
    let controller_manager = BackboneShell::get().controller_manager().borrow();
    let controllers = controller_manager
        .controller_config()
        .controllers
        .iter()
        .filter(|c| c.enabled);
    // Build global auto units
    let mut global_auto_units: NonCryptoHashMap<_, _> = controllers
        .filter_map(build_auto_unit_from_controller)
        .map(|au| (au.controller_id.clone(), au))
        .collect();
    // Sort all instances in a project-first
    let mut project_first_instances =
        BackboneShell::get().with_instance_shell_infos(|infos| infos.to_vec());
    project_first_instances.sort_unstable_by(instance_comparator);
    // Give each instance the chance to give a veto for a global auto unit. Or even instantiate
    // its own local auto unit, in which case the project-first ordering is important
    // (because project instances have priority over monitoring FX instances when it comes to
    // controller overrides).
    for instance_shell in project_first_instances
        .iter()
        .filter_map(|i| i.instance_shell.upgrade())
    {
        instance_shell.judge_auto_unit_candidates(&mut global_auto_units);
    }
    // Distribute the remaining global auto units in reverse order (monitoring FX first).
    // Reason: We want the global units to be as long-lived as possible. If there's a ReaLearn
    // instance on the monitoring FX chain and global control is enabled for it, it should get
    // most of the global auto units.
    for instance_shell in project_first_instances
        .iter()
        .rev()
        .filter_map(|i| i.instance_shell.upgrade())
    {
        let result = instance_shell.apply_auto_units(&mut global_auto_units);
        notify_user_on_anyhow_error(result);
    }
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
        controller_palette_color: controller.palette_color,
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
            .find_preset_info_by_id(id)
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
    match connection {
        ControllerConnection::Midi(con) => {
            // This is a MIDI controller
            let identity_pattern_suitability =
                if let Some(p) = &controller_preset_meta_data.midi_identity_pattern {
                    get_suitability_of_midi_identity_pattern_for_controller(p, con)
                } else {
                    // Controller preset doesn't define any identity pattern, which could just be laziness
                    ControllerSuitability::MaybeSuitable
                };
            if identity_pattern_suitability.is_not_suitable() {
                // Identity doesn't match, this is a no-go
                return ControllerSuitability::NotSuitable;
            }
            let patterns = &controller_preset_meta_data.midi_output_port_patterns;
            if patterns.is_empty() {
                // Controller preset doesn't define any MIDI output port patterns. This is perfectly valid.
                return identity_pattern_suitability;
            }
            // MIDI output port patterns given. Take them into account as well.
            let Some(output_port) = con.output_port else {
                // If the controller has not output, it can't match an output port pattern.
                return ControllerSuitability::NotSuitable;
            };
            let out_dev_id = MidiOutputDeviceId::new(output_port.get() as _);
            let Some(out_dev_name) = Reaper::get().midi_output_device_by_id(out_dev_id).name()
            else {
                // If the controller doesn't have a name / is not available, it can't match an output port pattern.
                return ControllerSuitability::NotSuitable;
            };
            let out_dev_name = out_dev_name.into_string();
            if midi_output_port_patterns_match(patterns, &out_dev_name) {
                identity_pattern_suitability
            } else {
                ControllerSuitability::NotSuitable
            }
        }
        ControllerConnection::Osc(_) => {
            // This is an OSC controller
            if controller_preset_meta_data.midi_identity_pattern.is_some()
                || !controller_preset_meta_data
                    .midi_output_port_patterns
                    .is_empty()
            {
                // We have a preset for a MIDI controller but this is an OSC controller.
                ControllerSuitability::NotSuitable
            } else {
                // We don't have any checks for OSC
                ControllerSuitability::MaybeSuitable
            }
        }
    }
}

fn get_suitability_of_midi_identity_pattern_for_controller(
    midi_identity_pattern: &str,
    connection: &MidiControllerConnection,
) -> ControllerSuitability {
    match &connection.identity_response {
        None => {
            // Controller doesn't expect any specific identity response.
            ControllerSuitability::MaybeSuitable
        }
        Some(identity_response) => {
            if midi_identity_response_matches_pattern(identity_response, midi_identity_pattern) {
                ControllerSuitability::Suitable
            } else {
                ControllerSuitability::NotSuitable
            }
        }
    }
}

fn midi_identity_response_matches_pattern(
    identity_response: &str,
    midi_identity_pattern: &str,
) -> bool {
    match BytePattern::from_str(midi_identity_pattern) {
        Ok(byte_pattern) => {
            match parse_hex_string(identity_response) {
                Ok(identity_response_bytes) => {
                    if byte_pattern.matches(&identity_response_bytes) {
                        tracing::debug!("Pattern matches identity response");
                        true
                    } else {
                        false
                    }
                }
                Err(_) => {
                    // Invalid response
                    tracing::warn!(
                        "Invalid MIDI identity response in controller: {identity_response}",
                    );
                    false
                }
            }
        }
        Err(_) => {
            // Invalid pattern
            tracing::warn!(
                "Invalid MIDI identity pattern in controller preset: {midi_identity_pattern}",
            );
            false
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

/// Order between instances. Starting with the ones in the current project (ascending by track and
/// position in FX chain), then in other projects (ascending by track and position in FX chain)
/// and then on the monitoring FX chain (ascending by position in FX chain).
fn instance_comparator(a: &InstanceShellInfo, b: &InstanceShellInfo) -> Ordering {
    use Ordering::*;
    // Monitoring FX chain (track == None) trumps track FX chain (track == Some)
    let fx_a = a.processor_context.containing_fx();
    let fx_b = b.processor_context.containing_fx();
    match (fx_a.track(), fx_b.track()) {
        (None, Some(_)) => Greater,
        (Some(_), None) => Less,
        (None, None) => {
            // Both are on monitoring FX chain. Higher position in chain trumps lower position.
            fx_a.index().cmp(&fx_b.index())
        }
        (Some(track_a), Some(track_b)) => {
            // Both are on tracks. Other project trumps current project.
            let current_project = Reaper::get().current_project();
            match (
                track_a.project() == current_project,
                track_b.project() == current_project,
            ) {
                (false, true) => return Greater,
                (true, false) => return Less,
                _ => {}
            };
            // Both instances are in the same project. Master track trumps normal track.
            match (track_a.index(), track_b.index()) {
                (None, Some(_)) => Greater,
                (Some(_), None) => Less,
                (None, None) => {
                    // Both are on master track. Higher position in chain trumps lower position.
                    fx_a.index().cmp(&fx_b.index())
                }
                (Some(index_a), Some(index_b)) => {
                    // Both are on normal tracks. Now ascending by track and index.
                    let ord = index_a.cmp(&index_b);
                    if ord.is_ne() {
                        return ord;
                    }
                    fx_a.index().cmp(&fx_b.index())
                }
            }
        }
    }
}

pub fn midi_output_port_patterns_match(patterns: &[String], out_port_name: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let lower_case_out_port_name = out_port_name.to_lowercase();
    patterns.iter().any(|pattern| {
        let wild_match = WildMatch::new(&pattern.to_lowercase());
        wild_match.matches(&lower_case_out_port_name)
    })
}
