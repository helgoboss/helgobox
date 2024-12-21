use crate::domain::{
    format_as_pretty_hex, MidiInDevsConfig, MidiOutDevsConfig, RequestMidiDeviceIdentityReply,
};
use crate::infrastructure::data::MainPresetSelectionConditions;
use crate::infrastructure::plugin::{ini_util, BackboneShell};
use anyhow::{anyhow, bail};
use helgobox_api::persistence::{
    CompartmentPresetId, Controller, ControllerConnection, MidiControllerConnection, MidiInputPort,
    MidiOutputPort,
};
use helgobox_api::runtime::{AutoAddedControllerEvent, GlobalInfoEvent};
use reaper_high::Reaper;
use reaper_medium::MidiOutputDeviceId;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ControllerProbe {
    pub out_dev_id: MidiOutputDeviceId,
    pub result: Result<SuccessfulControllerProbe, FailedControllerProbe>,
}

impl Display for ControllerProbe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let dev = Reaper::get().midi_output_device_by_id(self.out_dev_id);
        writeln!(
            f,
            "=== MIDI output device [{}] {:?}\n",
            self.out_dev_id,
            dev.name().unwrap_or_default()
        )?;
        match &self.result {
            Ok(probe) => probe.fmt(f)?,
            Err(probe) => probe.fmt(f)?,
        }
        writeln!(f)?;
        Ok(())
    }
}

pub struct SuccessfulControllerProbe {
    pub outcome: ControllerProbeOutcome,
    pub payload: ControllerProbePayload,
}

impl SuccessfulControllerProbe {
    pub fn new(outcome: ControllerProbeOutcome, payload: ControllerProbePayload) -> Self {
        Self { outcome, payload }
    }
}

impl Display for SuccessfulControllerProbe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Outcome: {}\n\n{}", &self.outcome, &self.payload)
    }
}

pub struct FailedControllerProbe {
    pub error: anyhow::Error,
    pub payload: ControllerProbePayload,
}

impl FailedControllerProbe {
    pub fn new(error: anyhow::Error, payload: ControllerProbePayload) -> Self {
        Self { error, payload }
    }
}

impl Display for FailedControllerProbe {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error: {:#?}\n\n{}", &self.error, &self.payload)
    }
}

#[derive(Default)]
pub struct ControllerProbePayload {
    pub identity_reply: Option<RequestMidiDeviceIdentityReply>,
}

impl Display for ControllerProbePayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(r) = &self.identity_reply {
            let in_dev = Reaper::get().midi_input_device_by_id(r.input_device_id);
            writeln!(
                f,
                "Identity reply: {}\nInput device: [{}] {:?}",
                &r.device_inquiry_reply,
                in_dev.id(),
                in_dev.name().unwrap_or_default(),
            )?
        }
        Ok(())
    }
}

pub enum ControllerProbeOutcome {
    OutputInUseAlready(Controller),
    InputInUseAlready(Controller),
    CreatedController(Controller),
}

impl Display for ControllerProbeOutcome {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ControllerProbeOutcome::OutputInUseAlready(c) => {
                write!(
                    f,
                    "Output port already in use by controller \"{}\"",
                    &c.name
                )?;
            }
            ControllerProbeOutcome::InputInUseAlready(c) => {
                write!(f, "Input port already in use by controller \"{}\"", &c.name)?;
            }
            ControllerProbeOutcome::CreatedController(c) => {
                write!(f, "Created controller \"{}\"", &c.name)?;
            }
        }
        Ok(())
    }
}

pub async fn detect_controllers(
    out_dev_ids: Vec<MidiOutputDeviceId>,
) -> anyhow::Result<Vec<ControllerProbe>> {
    let reaper_is_compatible = Reaper::get()
        .medium_reaper()
        .low()
        .pointers()
        .midi_init
        .is_none();
    if reaper_is_compatible {
        // REAPER version < 6.47
        bail!("Detecting controllers is not possible in REAPER versions < 6.47.");
    }
    // Prevent multiple tasks of this kind from running at the same time
    static IS_RUNNING: AtomicBool = AtomicBool::new(false);
    if IS_RUNNING.swap(true, Ordering::Relaxed) {
        bail!("Controller detection is already running. Please try again later.");
    }
    // Now, let's go
    let mut probes = vec![];
    for out_dev_id in out_dev_ids {
        let outcome = maybe_create_controller_for_device(out_dev_id).await;
        let probe = ControllerProbe {
            out_dev_id,
            result: outcome,
        };
        probes.push(probe)
    }
    IS_RUNNING.store(false, Ordering::Relaxed);
    Ok(probes)
}

async fn maybe_create_controller_for_device(
    out_dev_id: MidiOutputDeviceId,
) -> Result<SuccessfulControllerProbe, FailedControllerProbe> {
    let mut payload = ControllerProbePayload::default();
    // Don't create controller if there is one already which uses that device
    let existing_controller = BackboneShell::get()
        .controller_manager()
        .borrow()
        .find_controller_connected_to_midi_output(out_dev_id)
        .cloned();
    if let Some(controller) = existing_controller {
        // Output used already by existing controller
        return Ok(SuccessfulControllerProbe::new(
            ControllerProbeOutcome::OutputInUseAlready(controller),
            payload,
        ));
    }
    // Make sure that MIDI output device is enabled
    tracing::debug!(msg = "Temporarily enabling MIDI output device", %out_dev_id);
    let old_midi_outs = MidiOutDevsConfig::from_reaper();
    let tmp_midi_outs = old_midi_outs.with_dev_enabled(out_dev_id);
    tmp_midi_outs.apply_to_reaper();
    // Temporarily enable all input devices (so they can listen to the device identity reply)
    tracing::debug!(msg = "Temporarily enabling all input devices...");
    let old_midi_ins = MidiInDevsConfig::from_reaper();
    let tmp_midi_ins = MidiInDevsConfig::ALL_ENABLED;
    tmp_midi_ins.apply_to_reaper();
    // Apply changes
    Reaper::get().medium_reaper().low().midi_init(-1, -1);
    // Send device identity request to MIDI output device
    tracing::debug!(msg = "Sending device request to MIDI output device...", %out_dev_id);
    let identity_reply_result = BackboneShell::get()
        .request_midi_device_identity(out_dev_id, None)
        .await;
    // As soon as possible, reset MIDI devices to old state (we don't want to leave traces)
    tracing::debug!(msg = "Resetting MIDI output and input devices to previous state...");
    old_midi_outs.apply_to_reaper();
    old_midi_ins.apply_to_reaper();
    Reaper::get().medium_reaper().low().midi_init(-1, -1);
    // Process identity reply
    let identity_reply = match identity_reply_result {
        Ok(r) => r,
        Err(e) => {
            return Err(FailedControllerProbe::new(e, payload));
        }
    };
    payload.identity_reply = Some(identity_reply.clone());
    let in_dev_id = identity_reply.input_device_id;
    tracing::info!(
        msg = "Received identity reply from MIDI device",
        %out_dev_id,
        %in_dev_id,
        reply = %identity_reply.device_inquiry_reply
    );
    //  Check if input already used by existing controller
    tracing::debug!(msg = "Check if input used already used by existing controller...");
    let existing_controller = BackboneShell::get()
        .controller_manager()
        .borrow()
        .find_controller_connected_to_midi_input(in_dev_id)
        .cloned();
    if let Some(controller) = existing_controller {
        // Input already used by existing controller
        tracing::debug!(msg = "Input already used");
        return Ok(SuccessfulControllerProbe::new(
            ControllerProbeOutcome::InputInUseAlready(controller),
            payload,
        ));
    }
    // Neither output nor input used yet. Maybe this is a known controller!
    let controller_preset_manager = BackboneShell::get().controller_preset_manager().borrow();
    let out_port_name = Reaper::get().midi_output_device_by_id(out_dev_id).name();
    let Some(out_port_name) = out_port_name else {
        let probe = FailedControllerProbe::new(
            anyhow!("MIDI output device doesn't return name / is not available"),
            payload,
        );
        return Err(probe);
    };
    let out_port_name = out_port_name.to_string_lossy();
    tracing::info!(msg = "Input not yet used. Finding matching controller preset...", %out_port_name);
    let controller_preset = controller_preset_manager
        .find_controller_preset_compatible_with_device(
            &identity_reply.device_inquiry_reply.message,
            &out_port_name,
        );
    let Some(controller_preset) = controller_preset else {
        let probe =
            FailedControllerProbe::new(anyhow!("no controller preset matching device"), payload);
        return Err(probe);
    };
    let device_name = controller_preset.common.meta_data.device_name.as_ref();
    let Some(device_name) = device_name else {
        let probe = FailedControllerProbe::new(
            anyhow!("controller preset doesn't have device name"),
            payload,
        );
        return Err(probe);
    };
    // Search for suitable main preset
    let controller_preset_id = &controller_preset.common.id;
    tracing::debug!(msg = "Found controller preset", %controller_preset_id);
    tracing::debug!(msg = "Finding main preset...");
    let main_preset_manager = BackboneShell::get().main_preset_manager().borrow();
    let conditions = MainPresetSelectionConditions {
        at_least_one_instance_has_playtime_clip_matrix: {
            BackboneShell::get()
                .find_first_helgobox_instance_matching(|info| {
                    let Some(instance) = info.instance.upgrade() else {
                        return false;
                    };
                    let instance_state = instance.borrow();
                    instance_state.has_clip_matrix()
                })
                .is_some()
        },
    };
    let main_preset = main_preset_manager.find_most_suitable_main_preset_for_schemes(
        &controller_preset.specific_meta_data.provided_schemes,
        conditions,
    );
    tracing::debug!(msg = "Main preset result available", ?main_preset);
    let default_main_preset = main_preset.map(|mp| CompartmentPresetId::new(mp.common.id.clone()));
    // Make sure the involved MIDI devices are enabled
    tracing::debug!(
        "Enabling MIDI input device {in_dev_id} and MIDI output device {out_dev_id}..."
    );
    let new_midi_outs = old_midi_outs.with_dev_enabled(out_dev_id);
    new_midi_outs.apply_to_reaper();
    let new_midi_ins = old_midi_ins.with_dev_enabled(in_dev_id);
    new_midi_ins.apply_to_reaper();
    Reaper::get().medium_reaper().low().midi_init(-1, -1);
    // Persist the changes
    tracing::debug!(
        msg = "Persisting MIDI in/out dev changes...",
        ?new_midi_ins,
        ?new_midi_outs
    );
    if let Err(e) = write_midi_devs_config_to_reaper_ini(new_midi_ins, new_midi_outs) {
        let probe = FailedControllerProbe::new(
            e.context("Failed to write MIDI device config to reaper.ini"),
            payload,
        );
        return Err(probe);
    };
    // Auto-create controller
    tracing::debug!("Auto-creating controller...");
    let controller = Controller {
        id: "".to_string(),
        name: device_name.clone(),
        enabled: true,
        palette_color: None,
        connection: Some(ControllerConnection::Midi(MidiControllerConnection {
            identity_response: Some(format_as_pretty_hex(
                &identity_reply.device_inquiry_reply.message,
            )),
            input_port: Some(MidiInputPort::new(
                identity_reply.input_device_id.get() as u32
            )),
            output_port: Some(MidiOutputPort::new(out_dev_id.get() as u32)),
        })),
        default_controller_preset: None,
        default_main_preset,
    };
    let save_outcome = BackboneShell::get()
        .controller_manager()
        .borrow_mut()
        .save_controller(controller.clone());
    let save_outcome = match save_outcome {
        Ok(o) => o,
        Err(e) => {
            return Err(FailedControllerProbe::new(
                e.context("Couldn't save controller"),
                payload,
            ));
        }
    };
    BackboneShell::get()
        .proto_hub()
        .notify_about_global_info_event(GlobalInfoEvent::AutoAddedController(
            AutoAddedControllerEvent {
                controller_id: save_outcome.id,
            },
        ));
    let probe = SuccessfulControllerProbe::new(
        ControllerProbeOutcome::CreatedController(controller),
        payload,
    );
    Ok(probe)
}

fn write_midi_devs_config_to_reaper_ini(
    midi_in_devs: MidiInDevsConfig,
    midi_out_devs: MidiOutDevsConfig,
) -> anyhow::Result<()> {
    let reaper = Reaper::get();
    let reaper_ini = reaper.medium_reaper().get_ini_file(|p| p.to_path_buf());
    // Replace existing entries
    for (key, val) in midi_in_devs
        .to_ini_entries()
        .chain(midi_out_devs.to_ini_entries())
    {
        ini_util::write_ini_entry(reaper_ini.as_str(), "REAPER", key, val.to_string())?;
    }
    Ok(())
}
