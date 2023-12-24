use crate::domain::{
    AudioBlockProps, ControlEvent, IncomingMidiMessage, MidiEvent, PluginParamIndex, PluginParams,
    ProcessorContext, RawParamValue,
};
use crate::infrastructure::data::SessionData;
use crate::infrastructure::plugin::instance_shell::InstanceShell;
use crate::infrastructure::plugin::UnitParameterContainer;
use crate::infrastructure::ui::unit_panel::UnitPanel;
use anyhow::{bail, Context};
use base::{blocking_read_lock, non_blocking_try_read_lock};
use std::sync::{Arc, RwLock};
use swell_ui::SharedView;
use vst::plugin::HostCallback;

/// Represents a Helgobox Unit in the infrastructure layer.
///
/// Each Helgobox FX instance owns exactly one Helgobox unit shell. The unit shell shell owns all
/// its associated instance shells.
///
/// This was previously simply a part of the `RealearnPlugin` struct.
#[derive(Debug)]
pub struct UnitShell {
    main_instance_shell: InstanceShell,
    /// All contained instance shells.
    ///
    /// This needs to be protected because we might add/remove/set instances in the main
    /// thread at a later point in time. There should be no contention whatsoever unless
    /// we are in the process of modifying the set of instances, which happens in the main thread.
    ///
    /// That's why each access from the audio thread should fail fast if there's contention. It
    /// should never wait for a lock. That way, the audio thread will not be blocked for sure.
    instance_shells: RwLock<Vec<InstanceShell>>,
}

impl UnitShell {
    /// Creates a unit shell with exactly one instance shell.
    pub fn new(
        processor_context: ProcessorContext,
        unit_parameter_container: Arc<UnitParameterContainer>,
        unit_panel: SharedView<UnitPanel>,
    ) -> Self {
        let main_instance_shell = InstanceShell::new(
            processor_context.clone(),
            unit_parameter_container,
            unit_panel,
        );
        Self {
            main_instance_shell,
            instance_shells: Default::default(),
        }
    }

    /// Returns the state of the current unit in serialized form.
    ///
    /// This must not lock any data that's accessed from real-time threads.
    ///
    /// Must be called from the main thread.
    pub fn save(&self, params: &PluginParams) -> Vec<u8> {
        let session_data = self.create_session_data_internal(params);
        serde_json::to_vec(&session_data).expect("couldn't serialize session data")
    }

    /// Must be called from the main thread.
    pub fn create_session_data(&self, params: &PluginParams) -> SessionData {
        self.create_session_data_internal(params)
    }

    fn create_session_data_internal(&self, params: &PluginParams) -> SessionData {
        let model = self.main_instance_shell.model().borrow();
        SessionData::from_model(&model, params)
    }

    pub fn set_all_parameters(&self, params: PluginParams) {
        self.main_instance_shell.set_all_parameters(params);
    }

    pub fn set_single_parameter(&self, index: PluginParamIndex, value: RawParamValue) {
        self.main_instance_shell.set_single_parameter(index, value);
    }

    /// Restores unit shell state from the given serialized data.
    ///
    /// This must not lock any data that's accessed from real-time threads.
    ///
    /// To be called from main thread.
    pub fn load(&self, data: &[u8]) -> anyhow::Result<PluginParams> {
        let left_json_object_brace = data
            .iter()
            .position(|b| *b == 0x7b)
            .context("couldn't find left JSON brace in bank data")?;
        // ReaLearn C++ saved some IPlug binary data in front of the actual JSON object. Find
        // start of JSON data.
        let data = &data[left_json_object_brace..];
        let session_data: SessionData = match serde_json::from_slice(data) {
            Ok(d) => d,
            Err(e) => {
                bail!(
                    "ReaLearn couldn't restore this session: {}\n\nPlease also attach the following text when reporting this: \n\n{}",
                    e,
                    std::str::from_utf8(data).unwrap_or("UTF-8 decoding error")
                )
            }
        };
        self.main_instance_shell.apply_session_data(&session_data)
    }

    pub fn apply_session_data(&self, session_data: &SessionData) -> anyhow::Result<PluginParams> {
        self.main_instance_shell.apply_session_data(session_data)
    }

    /// Forwards the given <FX input> MIDI event to all instances.
    ///
    /// To be called from real-time thread.
    pub fn process_incoming_midi_from_plugin(
        &self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        is_transport_start: bool,
        host: HostCallback,
    ) {
        let Some(instance_shells) = non_blocking_try_read_lock(&self.instance_shells) else {
            // Better miss one block than blocking the entire audio thread
            return;
        };
        for instance_shell in &*instance_shells {
            instance_shell.process_incoming_midi_from_vst(event, is_transport_start, host);
        }
    }

    /// Invokes the processing function for each instance.
    ///
    /// To be called from real-time thread (in the plug-in's processing function).
    pub fn run_from_plugin(
        &self,
        #[cfg(feature = "playtime")] buffer: &mut vst::buffer::AudioBuffer<f64>,
        #[cfg(feature = "playtime")] block_props: AudioBlockProps,
        host: HostCallback,
    ) {
        let Some(instance_shells) = non_blocking_try_read_lock(&self.instance_shells) else {
            // Better miss one block than blocking the entire audio thread
            return;
        };
        for instance_shell in &*instance_shells {
            instance_shell.run_from_vst(
                #[cfg(feature = "playtime")]
                buffer,
                #[cfg(feature = "playtime")]
                block_props,
                host,
            );
        }
    }

    /// Informs all instances about a sample rate change.
    ///
    /// To be called from main thread.
    pub fn set_sample_rate(&self, rate: f32) {
        // Called in main thread. There should not be any contention from audio thread because
        // this is only called when plug-in suspended.
        let instance_shells = blocking_read_lock(&self.instance_shells, "set_sample_rate");
        for instance_shell in &*instance_shells {
            instance_shell.set_sample_rate(rate);
        }
    }
}
