use crate::application::{AutoUnitData, SharedUnitModel};
use crate::domain::{
    AudioBlockProps, ControlEvent, IncomingMidiMessage, Instance, InstanceHandler, InstanceId,
    MidiEvent, ProcessorContext, SharedInstance, SharedRealTimeInstance, UnitId,
};
use crate::infrastructure::data::{InstanceData, InstanceOrUnitData, UnitData};
use crate::infrastructure::plugin::unit_shell::UnitShell;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::UnitPanel;
use anyhow::{bail, Context};
use base::{
    blocking_read_lock, blocking_write_lock, non_blocking_lock, non_blocking_try_read_lock,
    tracing_debug,
};
use enumset::EnumSet;
use fragile::Fragile;
use playtime_clip_engine::base::Matrix;
use realearn_api::persistence::{ControllerRoleKind, InstanceSettings};
use reaper_high::Project;
use std::cell::RefCell;
use std::collections::HashMap;
use std::iter::once;
use std::rc::Rc;
use std::sync;
use std::sync::{Arc, Mutex, RwLock};
use swell_ui::SharedView;
use vst::plugin::HostCallback;

pub type SharedInstanceShell = Arc<InstanceShell>;
pub type WeakInstanceShell = sync::Weak<InstanceShell>;

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
    settings: Fragile<RefCell<InstanceSettings>>,
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
        BackboneShell::get().proto_hub().notify_clip_matrix_changed(
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
            .proto_hub()
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
    ) -> (Self, SharedRealTimeInstance) {
        let main_unit_id = UnitId::next();
        let instance_handler = CustomInstanceHandler {
            project: processor_context.project(),
        };
        let (instance, rt_instance) = Instance::new(
            instance_id,
            main_unit_id,
            processor_context.clone(),
            BackboneShell::get().instance_event_sender().clone(),
            Box::new(instance_handler),
            #[cfg(feature = "playtime")]
            BackboneShell::get().clip_matrix_event_sender().clone(),
            #[cfg(feature = "playtime")]
            BackboneShell::get().normal_audio_hook_task_sender().clone(),
        );
        let rt_instance = Arc::new(Mutex::new(rt_instance));
        let instance_id = instance.id();
        let instance = Rc::new(RefCell::new(instance));
        let main_unit_shell = UnitShell::new(
            main_unit_id,
            instance_id,
            processor_context.clone(),
            instance.clone(),
            Arc::downgrade(&rt_instance),
            SharedView::downgrade(&panel),
            true,
            None,
        );
        let shell = Self {
            instance: Fragile::new(instance),
            rt_instance: rt_instance.clone(),
            main_unit_shell,
            settings: Default::default(),
            additional_unit_shells: Default::default(),
            panel: Fragile::new(panel),
            processor_context: Fragile::new(processor_context),
            instance_id,
        };
        (shell, rt_instance)
    }

    pub fn settings(&self) -> InstanceSettings {
        self.settings.get().borrow().clone()
    }

    pub fn set_settings(&self, settings: InstanceSettings) -> anyhow::Result<()> {
        *self.settings.get().borrow_mut() = settings;
        let auto_units = BackboneShell::get().determine_auto_units();
        self.apply_auto_units(&auto_units)?;
        BackboneShell::get()
            .proto_hub()
            .notify_instance_settings_changed(self);
        Ok(())
    }

    pub fn instance_id(&self) -> InstanceId {
        self.instance_id
    }

    pub fn instance(&self) -> &SharedInstance {
        self.instance.get()
    }

    pub fn main_unit_shell(&self) -> &UnitShell {
        &self.main_unit_shell
    }

    pub fn additional_unit_models(&self) -> Vec<SharedUnitModel> {
        blocking_read_lock(&self.additional_unit_shells, "additional_unit_models")
            .iter()
            .map(|s| s.model().clone())
            .collect()
    }

    pub fn additional_unit_count(&self) -> usize {
        blocking_read_lock(&self.additional_unit_shells, "additional_unit_panel_count").len()
    }

    pub fn find_unit_index_and_model_by_id(
        &self,
        id: Option<UnitId>,
    ) -> Option<(Option<usize>, SharedUnitModel)> {
        self.find_unit_prop_by_id(id, |i, unit_shell| (i, unit_shell.model().clone()))
    }

    pub fn find_unit_panel_by_id(&self, id: Option<UnitId>) -> Option<SharedView<UnitPanel>> {
        self.find_unit_prop_by_id(id, |_, unit_shell| unit_shell.panel().clone())
    }

    pub fn find_unit_prop_by_id<R>(
        &self,
        id: Option<UnitId>,
        f: impl FnOnce(Option<usize>, &UnitShell) -> R,
    ) -> Option<R> {
        match id {
            None => Some(f(None, &self.main_unit_shell)),
            Some(i) => self.find_additional_unit_prop_by_id(i, f),
        }
    }

    fn find_additional_unit_prop_by_id<R>(
        &self,
        id: UnitId,
        f: impl FnOnce(Option<usize>, &UnitShell) -> R,
    ) -> Option<R> {
        blocking_read_lock(
            &self.additional_unit_shells,
            "find_additional_unit_prop_by_index",
        )
        .iter()
        .enumerate()
        .find(|(_, u)| u.id() == id)
        .map(|(i, u)| f(Some(i), u))
    }

    pub fn add_unit(&self) -> UnitId {
        let unit_shell = self.create_additional_unit_shell(None);
        let id = unit_shell.id();
        blocking_write_lock(&self.additional_unit_shells, "add_unit").push(unit_shell);
        self.notify_units_changed();
        id
    }

    pub fn set_auto_loaded_controller_roles(
        &self,
        roles: EnumSet<ControllerRoleKind>,
    ) -> anyhow::Result<()> {
        self.settings
            .get()
            .borrow_mut()
            .auto_loaded_controller_roles = roles.iter().collect();
        let auto_units = BackboneShell::get().determine_auto_units();
        self.apply_auto_units(&auto_units)
    }

    fn create_additional_unit_shell(&self, auto_unit: Option<AutoUnitData>) -> UnitShell {
        UnitShell::new(
            UnitId::next(),
            self.instance_id,
            self.processor_context.get().clone(),
            self.instance.get().clone(),
            Arc::downgrade(&self.rt_instance),
            SharedView::downgrade(self.panel.get()),
            false,
            auto_unit,
        )
    }

    fn notify_units_changed(&self) {
        self.panel.get().notify_units_changed();
        self.main_unit_shell.panel().notify_units_changed();
        for unit_shell in blocking_read_lock(&self.additional_unit_shells, "add_unit").iter() {
            unit_shell.panel().notify_units_changed();
        }
    }

    pub fn unit_exists(&self, id: UnitId) -> bool {
        blocking_read_lock(&self.additional_unit_shells, "unit_exists")
            .iter()
            .any(|u| u.id() == id)
    }

    pub fn remove_unit(&self, id: UnitId) -> anyhow::Result<()> {
        {
            let mut additional_unit_shells =
                blocking_write_lock(&self.additional_unit_shells, "remove_unit");
            additional_unit_shells.retain(|u| u.id() != id);
        }
        self.notify_units_changed();
        Ok(())
    }

    pub fn apply_auto_units(&self, required_auto_units: &[AutoUnitData]) -> anyhow::Result<()> {
        {
            let mut required_auto_units: HashMap<_, _> = required_auto_units
                .iter()
                .map(|au| (au.extract_id(), au))
                .collect();
            let mut additional_unit_shells =
                blocking_write_lock(&self.additional_unit_shells, "apply_auto_units");
            // At first we update or remove any existing auto units
            additional_unit_shells.retain_mut(|unit_shell| {
                let mut unit_model = unit_shell.model().borrow_mut();
                if let Some(existing_auto_unit) = unit_model.auto_unit() {
                    // This is an existing auto unit
                    if self
                        .settings
                        .get()
                        .borrow()
                        .auto_loaded_controller_roles
                        .contains(&existing_auto_unit.role_kind)
                    {
                        // This kind of auto unit is still desirable in terms of the role kind.
                        if let Some(matching_auto_unit) =
                            required_auto_units.remove(&existing_auto_unit.extract_id())
                        {
                            // The existing auto unit must be updated
                            unit_model.update_auto_unit(matching_auto_unit.clone());
                            true
                        } else {
                            // The existing auto unit is obsolete
                            false
                        }
                    } else {
                        // This kind of auto unit is not desirable anymore in terms of role kind.
                        false
                    }
                } else {
                    // This is a manual unit
                    true
                }
            });
            // All required auto units that are still left must be added
            for auto_unit in required_auto_units.into_values() {
                if self
                    .settings
                    .get()
                    .borrow()
                    .auto_loaded_controller_roles
                    .contains(&auto_unit.role_kind)
                {
                    tracing_debug!("Creating auto-unit shell");
                    let unit_shell = self.create_additional_unit_shell(Some(auto_unit.clone()));
                    additional_unit_shells.push(unit_shell);
                }
            }
        }
        self.notify_units_changed();
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
                .filter_map(|us| {
                    let model = us.model().borrow();
                    if model.auto_unit().is_some() {
                        // Auto units are not saved
                        return None;
                    }
                    Some(UnitData::from_model(&model))
                })
                .collect();
        let instance = self.instance.get().borrow();
        InstanceData {
            main_unit: UnitData::from_model(&self.main_unit_shell.model().borrow()),
            additional_units: additional_unit_datas,
            settings: self.settings.get().borrow().clone(),
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
        // General properties
        *self.settings.get().borrow_mut() = instance_data.settings;
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
                let unit_shell = self.create_additional_unit_shell(None);
                unit_shell.apply_data(&ud)?;
                Ok(unit_shell)
            })
            .collect();
        *blocking_write_lock(&self.additional_unit_shells, "InstanceShell apply_data") =
            additional_unit_shells?;
        // Apply auto units
        let auto_units = BackboneShell::get().determine_auto_units();
        self.apply_auto_units(&auto_units)?;
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