use crate::domain::{
    AudioBlockProps, ControlEvent, IncomingMidiMessage, Instance, InstanceHandler, InstanceId,
    MidiEvent, ProcessorContext, RealTimeInstance, SharedInstance, SharedRealTimeInstance, UnitId,
};
use crate::infrastructure::data::{InstanceData, InstanceOrUnitData, UnitData};
use crate::infrastructure::plugin::unit_shell::UnitShell;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::UnitPanel;
use anyhow::{bail, ensure, Context};
use base::{
    blocking_read_lock, blocking_write_lock, non_blocking_lock, non_blocking_try_read_lock,
    tracing_debug, SenderToRealTimeThread,
};
use fragile::Fragile;
use playtime_clip_engine::base::Matrix;
use reaper_high::Project;
use std::cell::RefCell;
use std::iter::once;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};
use swell_ui::SharedView;
use vst::plugin::HostCallback;

const REAL_TIME_INSTANCE_TASK_QUEUE_SIZE: usize = 200;

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
    instance_id: InstanceId,
    instance: Fragile<SharedInstance>,
    rt_instance: SharedRealTimeInstance,
    main_unit_shell: UnitShell,
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

impl Drop for InstanceShell {
    fn drop(&mut self) {
        tracing_debug!("Dropping InstanceShell...");
        BackboneShell::get().unregister_instance(self.instance_id);
    }
}

#[derive(Debug)]
struct CustomInstanceHandler {
    project: Option<Project>,
}

impl InstanceHandler for CustomInstanceHandler {
    #[cfg(feature = "playtime")]
    fn clip_matrix_changed(
        &self,
        instance_id: InstanceId,
        matrix: &Matrix,
        events: &[playtime_clip_engine::base::ClipMatrixEvent],
        is_poll: bool,
    ) {
        // TODO-medium If we would make the instance ID generic, we could save the string conversion
        BackboneShell::get().clip_engine_hub().clip_matrix_changed(
            &instance_id.to_string(),
            matrix,
            events,
            is_poll,
            self.project,
        );
    }

    #[cfg(feature = "playtime")]
    fn process_control_surface_change_event_for_clip_engine(
        &self,
        instance_id: InstanceId,
        matrix: &Matrix,
        events: &[reaper_high::ChangeEvent],
    ) {
        BackboneShell::get()
            .clip_engine_hub()
            .send_occasional_matrix_updates_caused_by_reaper(
                &instance_id.to_string(),
                matrix,
                events,
            );
    }
}

impl InstanceShell {
    /// Creates an instance shell with exactly one unit shell.
    pub fn new(
        instance_id: InstanceId,
        processor_context: ProcessorContext,
        panel: SharedView<InstancePanel>,
    ) -> Self {
        let main_unit_id = UnitId::next();
        let instance_handler = CustomInstanceHandler {
            project: processor_context.project(),
        };
        let (real_time_instance_task_sender, real_time_instance_task_receiver) =
            SenderToRealTimeThread::new_channel(
                "real-time instance tasks",
                REAL_TIME_INSTANCE_TASK_QUEUE_SIZE,
            );
        let instance = Instance::new(
            instance_id,
            main_unit_id,
            processor_context.clone(),
            BackboneShell::get().instance_event_sender().clone(),
            Box::new(instance_handler),
            #[cfg(feature = "playtime")]
            BackboneShell::get().clip_matrix_event_sender().clone(),
            #[cfg(feature = "playtime")]
            BackboneShell::get().normal_audio_hook_task_sender().clone(),
            #[cfg(feature = "playtime")]
            real_time_instance_task_sender,
        );
        let rt_instance = RealTimeInstance::new(real_time_instance_task_receiver);
        let rt_instance = Arc::new(Mutex::new(rt_instance));
        let instance_id = instance.id();
        let instance = Rc::new(RefCell::new(instance));
        BackboneShell::get().register_instance(
            instance_id,
            Rc::downgrade(&instance),
            rt_instance.clone(),
        );
        let main_unit_shell = UnitShell::new(
            main_unit_id,
            instance_id,
            processor_context.clone(),
            instance.clone(),
            Arc::downgrade(&rt_instance),
            SharedView::downgrade(&panel),
            true,
        );
        Self {
            instance: Fragile::new(instance),
            rt_instance,
            main_unit_shell,
            additional_unit_shells: Default::default(),
            panel: Fragile::new(panel),
            processor_context: Fragile::new(processor_context),
            instance_id,
        }
    }

    pub fn instance(&self) -> &SharedInstance {
        self.instance.get()
    }

    pub fn main_unit_shell(&self) -> &UnitShell {
        &self.main_unit_shell
    }

    pub fn additional_unit_count(&self) -> usize {
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
        let unit_shell = self.create_additional_unit_shell();
        let id = unit_shell.id();
        blocking_write_lock(&self.additional_unit_shells, "add_unit").push(unit_shell);
        id
    }

    fn create_additional_unit_shell(&self) -> UnitShell {
        UnitShell::new(
            UnitId::next(),
            self.instance_id,
            self.processor_context.get().clone(),
            self.instance.get().clone(),
            Arc::downgrade(&self.rt_instance),
            SharedView::downgrade(self.panel.get()),
            false,
        )
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
    pub fn save(&self) -> Vec<u8> {
        let instance_data = self.create_data();
        let data = InstanceOrUnitData::InstanceData(instance_data);
        serde_json::to_vec(&data).expect("couldn't serialize instance data")
    }

    pub fn create_data(&self) -> InstanceData {
        let additional_unit_datas =
            blocking_read_lock(&self.additional_unit_shells, "create_instance_data")
                .iter()
                .map(|us| UnitData::from_model(&us.model().borrow()))
                .collect();
        let instance = self.instance.get().borrow();
        InstanceData {
            main_unit: UnitData::from_model(&self.main_unit_shell.model().borrow()),
            additional_units: additional_unit_datas,
            pot_state: instance.save_pot_unit(),
            #[cfg(feature = "playtime")]
            clip_matrix: {
                instance.clip_matrix().map(|matrix| {
                    crate::infrastructure::data::ClipMatrixRefData::Own(Box::new(matrix.save()))
                })
            },
        }
    }

    /// Restores instance shell state from the given serialized data.
    ///
    /// This must not lock any data that's accessed from real-time threads.
    ///
    /// To be called from main thread.
    pub fn load(&self, data: &[u8]) -> anyhow::Result<()> {
        let left_json_object_brace = data
            .iter()
            .position(|b| *b == 0x7b)
            .context("couldn't find left JSON brace in bank data")?;
        // ReaLearn C++ saved some IPlug binary data in front of the actual JSON object. Find
        // start of JSON data.
        let data = &data[left_json_object_brace..];
        let data: InstanceOrUnitData = match serde_json::from_slice(data) {
            Ok(d) => d,
            Err(e) => {
                bail!(
                    "ReaLearn couldn't restore this unit: {}\n\nPlease also attach the following text when reporting this: \n\n{}",
                    e,
                    std::str::from_utf8(data).unwrap_or("UTF-8 decoding error")
                )
            }
        };
        self.apply_data_internal(data)?;
        Ok(())
    }

    pub fn apply_data(&self, instance_data: InstanceOrUnitData) -> anyhow::Result<()> {
        self.apply_data_internal(instance_data)
    }

    fn apply_data_internal(&self, instance_data: InstanceOrUnitData) -> anyhow::Result<()> {
        let instance_data = instance_data.into_instance_data();
        let instance = self.instance();
        // Pot state
        instance
            .borrow_mut()
            .restore_pot_unit(instance_data.pot_state.clone());
        // Clip matrix
        #[cfg(feature = "playtime")]
        {
            if let Some(matrix_ref) = instance_data.clip_matrix {
                match matrix_ref {
                    crate::infrastructure::data::ClipMatrixRefData::Own(m) => {
                        crate::application::get_or_insert_owned_clip_matrix(
                            Rc::downgrade(self.main_unit_shell.model()),
                            &mut instance.borrow_mut(),
                        )
                        .load(*m.clone())?;
                    }
                    crate::infrastructure::data::ClipMatrixRefData::Foreign(_) => {
                        bail!("Referring to the clip matrix of another instance is not supported anymore!");
                    }
                };
            } else {
                instance.borrow_mut().set_clip_matrix(None);
            }
        }
        // Main unit
        self.main_unit_shell.apply_data(&instance_data.main_unit)?;
        // Additional units
        let additional_unit_shells: anyhow::Result<Vec<UnitShell>> = instance_data
            .additional_units
            .into_iter()
            .map(|ud| {
                let unit_shell = self.create_additional_unit_shell();
                unit_shell.apply_data(&ud)?;
                Ok(unit_shell)
            })
            .collect();
        *blocking_write_lock(&self.additional_unit_shells, "InstanceShell apply_data") =
            additional_unit_shells?;
        Ok(())
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
        #[cfg(feature = "playtime")]
        non_blocking_lock(&*self.rt_instance, "RealTimeInstance").run_from_vst(buffer, block_props);
        let Some(unit_shells) = non_blocking_try_read_lock(&self.additional_unit_shells) else {
            // Better miss one block than blocking the entire audio thread
            return;
        };
        for unit_shell in once(&self.main_unit_shell).chain(&*unit_shells) {
            unit_shell.run_from_vst(host);
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
