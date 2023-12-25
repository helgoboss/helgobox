use crate::domain::{
    AudioBlockProps, ControlEvent, IncomingMidiMessage, MidiEvent, PluginParamIndex, PluginParams,
    ProcessorContext, RawParamValue, UnitId,
};
use crate::infrastructure::data::UnitData;
use crate::infrastructure::plugin::unit_shell::UnitShell;
use crate::infrastructure::plugin::InstanceParamContainer;
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::UnitPanel;
use anyhow::{bail, ensure, Context};
use base::{blocking_read_lock, blocking_write_lock, non_blocking_try_read_lock};
use fragile::Fragile;
use std::iter::once;
use std::sync::{Arc, RwLock};
use swell_ui::SharedView;
use vst::plugin::HostCallback;

/// Represents a Helgobox instance in the infrastructure layer.
///
/// Each Helgobox FX instance owns exactly one Helgobox instance. The instance shell owns all
/// its associated unit shells.
///
/// This was previously simply a part of the `RealearnPlugin` struct.
#[derive(Debug)]
pub struct InstanceShell {
    panel: Fragile<SharedView<InstancePanel>>,
    // TODO-low Not too cool that we need to make this fragile. Related to reaper-high cells.
    processor_context: Fragile<ProcessorContext>,
    main_unit_shell: UnitShell,
    param_container: Arc<InstanceParamContainer>,
    /// Additional unit shells.
    ///
    /// This needs to be protected because we might add/remove/set instances in the main
    /// thread at a later point in time. There should be no contention whatsoever unless
    /// we are in the process of modifying the set of instances, which happens in the main thread.
    ///
    /// That's why each access from the audio thread should fail fast if there's contention. It
    /// should never wait for a lock. That way, the audio thread will not be blocked for sure.
    additional_unit_shells: RwLock<Vec<UnitShell>>,
}

impl InstanceShell {
    /// Creates an instance shell with exactly one unit shell.
    pub fn new(
        processor_context: ProcessorContext,
        param_container: Arc<InstanceParamContainer>,
        panel: SharedView<InstancePanel>,
    ) -> Self {
        Self {
            main_unit_shell: UnitShell::new(
                processor_context.clone(),
                param_container.clone(),
                SharedView::downgrade(&panel),
            ),
            additional_unit_shells: Default::default(),
            param_container,
            panel: Fragile::new(panel),
            processor_context: Fragile::new(processor_context),
        }
    }

    pub fn main_unit_shell(&self) -> &UnitShell {
        &self.main_unit_shell
    }

    pub fn additional_unit_panel_count(&self) -> usize {
        blocking_read_lock(&self.additional_unit_shells, "additional_unit_panel_count").len()
    }

    pub fn find_unit_panel_by_index(&self, index: Option<usize>) -> Option<SharedView<UnitPanel>> {
        match index {
            None => Some(self.main_unit_shell.panel().clone()),
            Some(i) => self.find_additional_unit_panel_by_index(i),
        }
    }

    fn find_additional_unit_panel_by_index(&self, index: usize) -> Option<SharedView<UnitPanel>> {
        blocking_read_lock(
            &self.additional_unit_shells,
            "find_additional_unit_panel_by_index",
        )
        .get(index)
        .map(|unit_shell| unit_shell.panel().clone())
    }

    pub fn add_unit(&self) -> UnitId {
        let unit_shell = UnitShell::new(
            self.processor_context.get().clone(),
            self.param_container.clone(),
            SharedView::downgrade(self.panel.get()),
        );
        let id = unit_shell.id();
        blocking_write_lock(&self.additional_unit_shells, "add_unit").push(unit_shell);
        id
    }

    pub fn remove_unit(&self, index: usize) -> anyhow::Result<()> {
        let mut guard = blocking_write_lock(&self.additional_unit_shells, "remove_unit");
        ensure!(index < guard.len(), "unit doesn't exist");
        guard.remove(index);
        Ok(())
    }

    /// Returns the state of the current unit in serialized form.
    ///
    /// This must not lock any data that's accessed from real-time threads.
    ///
    /// Must be called from the main thread.
    pub fn save(&self, params: &PluginParams) -> Vec<u8> {
        let unit_data = self.create_unit_data_internal(params);
        serde_json::to_vec(&unit_data).expect("couldn't serialize unit data")
    }

    /// Must be called from the main thread.
    pub fn create_unit_data(&self, params: &PluginParams) -> UnitData {
        self.create_unit_data_internal(params)
    }

    fn create_unit_data_internal(&self, params: &PluginParams) -> UnitData {
        let model = self.main_unit_shell.model().borrow();
        UnitData::from_model(&model, params)
    }

    pub fn set_all_parameters(&self, params: PluginParams) {
        self.main_unit_shell.set_all_parameters(params);
    }

    pub fn set_single_parameter(&self, index: PluginParamIndex, value: RawParamValue) {
        self.main_unit_shell.set_single_parameter(index, value);
    }

    /// Restores instance shell state from the given serialized data.
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
        let unit_data: UnitData = match serde_json::from_slice(data) {
            Ok(d) => d,
            Err(e) => {
                bail!(
                    "ReaLearn couldn't restore this unit: {}\n\nPlease also attach the following text when reporting this: \n\n{}",
                    e,
                    std::str::from_utf8(data).unwrap_or("UTF-8 decoding error")
                )
            }
        };
        self.main_unit_shell.apply_unit_data(&unit_data)
    }

    pub fn apply_unit_data(&self, unit_data: &UnitData) -> anyhow::Result<PluginParams> {
        self.main_unit_shell.apply_unit_data(unit_data)
    }

    /// Forwards the given <FX input> MIDI event to all units.
    ///
    /// To be called from real-time thread.
    pub fn process_incoming_midi_from_plugin(
        &self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        is_transport_start: bool,
        host: HostCallback,
    ) {
        let Some(unit_shells) = non_blocking_try_read_lock(&self.additional_unit_shells) else {
            // Better miss one block than blocking the entire audio thread
            return;
        };
        for instance_shell in once(&self.main_unit_shell).chain(&*unit_shells) {
            instance_shell.process_incoming_midi_from_vst(event, is_transport_start, host);
        }
    }

    /// Invokes the processing function for each unit.
    ///
    /// To be called from real-time thread (in the plug-in's processing function).
    pub fn run_from_plugin(
        &self,
        #[cfg(feature = "playtime")] buffer: &mut vst::buffer::AudioBuffer<f64>,
        #[cfg(feature = "playtime")] block_props: AudioBlockProps,
        host: HostCallback,
    ) {
        let Some(unit_shells) = non_blocking_try_read_lock(&self.additional_unit_shells) else {
            // Better miss one block than blocking the entire audio thread
            return;
        };
        for unit_shell in once(&self.main_unit_shell).chain(&*unit_shells) {
            unit_shell.run_from_vst(
                #[cfg(feature = "playtime")]
                buffer,
                #[cfg(feature = "playtime")]
                block_props,
                host,
            );
        }
    }

    /// Informs all units about a sample rate change.
    ///
    /// To be called from main thread.
    pub fn set_sample_rate(&self, rate: f32) {
        // Called in main thread. There should not be any contention from audio thread because
        // this is only called when plug-in suspended.
        let unit_shells = blocking_read_lock(&self.additional_unit_shells, "set_sample_rate");
        for unit_shell in once(&self.main_unit_shell).chain(&*unit_shells) {
            unit_shell.set_sample_rate(rate);
        }
    }
}
