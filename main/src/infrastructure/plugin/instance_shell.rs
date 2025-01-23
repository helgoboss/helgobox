use crate::application::{AutoUnitData, SharedUnitModel};
use crate::domain::{
    ControlEvent, IncomingMidiMessage, Instance, InstanceHandler, InstanceId, MidiEvent,
    ProcessorContext, SharedInstance, SharedRealTimeInstance, UnitId,
};
use crate::infrastructure::data::{InstanceData, UnitData};
use crate::infrastructure::plugin::unit_shell::UnitShell;
use crate::infrastructure::plugin::{update_auto_units_async, BackboneShell};
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::UnitPanel;
use anyhow::{bail, Context};
use base::hash_util::NonCryptoHashMap;
use base::{blocking_read_lock, blocking_write_lock, non_blocking_try_read_lock};
use fragile::Fragile;
use helgobox_api::persistence::{instance_features, InstanceSettings};
use playtime_api::persistence::FlexibleMatrix;
use reaper_high::Project;
use std::cell::RefCell;
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
        tracing::debug!("Dropping InstanceShell...");
        if BackboneShell::is_loaded() {
            BackboneShell::get().unregister_instance(self.instance_id);
        }
    }
}

#[derive(Debug)]
struct CustomInstanceHandler {
    #[allow(unused)]
    project: Option<Project>,
}

impl InstanceHandler for CustomInstanceHandler {
    #[cfg(feature = "playtime")]
    fn clip_matrix_changed(
        &self,
        instance_id: InstanceId,
        matrix: &playtime_clip_engine::base::Matrix,
        events: &[playtime_clip_engine::base::ClipMatrixEvent],
        is_poll: bool,
    ) {
        // TODO-medium If we would make the instance ID generic, we could save the string conversion
        BackboneShell::get().proto_hub().notify_clip_matrix_changed(
            instance_id,
            matrix,
            events,
            is_poll,
        );
    }

    #[cfg(feature = "playtime")]
    fn process_control_surface_change_event_for_clip_engine(
        &self,
        instance_id: InstanceId,
        matrix: &playtime_clip_engine::base::Matrix,
        events: &[reaper_high::ChangeEvent],
    ) {
        BackboneShell::get()
            .proto_hub()
            .send_occasional_matrix_updates_caused_by_reaper(instance_id, matrix, events);
    }
}

impl InstanceShell {
    pub fn rt_instance(&self) -> SharedRealTimeInstance {
        self.rt_instance.clone()
    }

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
        let (instance, rt_instance) = Instance::new(
            instance_id,
            main_unit_id,
            processor_context.clone(),
            BackboneShell::get().instance_event_sender().clone(),
            Box::new(instance_handler),
            #[cfg(feature = "playtime")]
            BackboneShell::get().clip_matrix_event_sender().clone(),
            BackboneShell::get().normal_audio_hook_task_sender().clone(),
        );
        let rt_instance = Arc::new(Mutex::new(rt_instance));
        let instance_id = instance.id();
        let instance = Rc::new(RefCell::new(instance));
        let main_unit_shell = UnitShell::new(
            main_unit_id,
            Some("Main".into()),
            instance_id,
            processor_context.clone(),
            instance.clone(),
            Arc::downgrade(&rt_instance),
            SharedView::downgrade(&panel),
            true,
            None,
        );
        Self {
            instance: Fragile::new(instance),
            rt_instance: rt_instance.clone(),
            main_unit_shell,
            settings: Default::default(),
            additional_unit_shells: Default::default(),
            panel: Fragile::new(panel),
            processor_context: Fragile::new(processor_context),
            instance_id,
        }
    }

    pub fn processor_context(&self) -> ProcessorContext {
        self.processor_context.get().clone()
    }

    pub fn settings(&self) -> InstanceSettings {
        self.settings.get().borrow().clone()
    }

    pub fn toggle_global_control(&self) {
        self.change_settings(|settings| settings.control.global_control_enabled ^= true);
    }

    pub fn change_settings(&self, f: impl FnOnce(&mut InstanceSettings)) {
        f(&mut self.settings.get().borrow_mut());
        self.handle_changed_settings();
    }

    fn handle_changed_settings(&self) {
        update_auto_units_async();
        BackboneShell::get()
            .proto_hub()
            .notify_instance_settings_changed(self);
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

    pub fn all_unit_models(&self) -> impl Iterator<Item = SharedUnitModel> {
        let additional_unit_models = self.additional_unit_models();
        once(self.main_unit_shell().model().clone()).chain(additional_unit_models)
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
        self.find_unit_prop_by_id_simple(id, |i, unit_shell| (i, unit_shell.model().clone()))
    }

    pub fn panel(&self) -> &SharedView<InstancePanel> {
        self.panel.get()
    }

    pub fn find_unit_panel_by_id(&self, id: Option<UnitId>) -> Option<SharedView<UnitPanel>> {
        self.find_unit_prop_by_id_simple(id, |_, unit_shell| unit_shell.panel().clone())
    }

    pub fn find_unit_prop_by_id_simple<R>(
        &self,
        id: Option<UnitId>,
        f: impl FnOnce(Option<usize>, &UnitShell) -> R,
    ) -> Option<R> {
        match id {
            None => Some(f(None, &self.main_unit_shell)),
            Some(i) => self.find_unit_prop_by_id(i, f),
        }
    }

    fn find_unit_prop_by_id<R>(
        &self,
        id: UnitId,
        f: impl FnOnce(Option<usize>, &UnitShell) -> R,
    ) -> Option<R> {
        if self.main_unit_shell.id() == id {
            return Some(f(None, &self.main_unit_shell));
        }
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

    fn create_additional_unit_shell(&self, auto_unit: Option<AutoUnitData>) -> UnitShell {
        let initial_name = auto_unit.as_ref().and_then(|au| {
            BackboneShell::get()
                .controller_manager()
                .borrow()
                .find_controller_by_id(&au.controller_id)?
                .name
                .clone()
                .into()
        });
        UnitShell::new(
            UnitId::next(),
            initial_name,
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
        BackboneShell::get()
            .proto_hub()
            .notify_instance_units_changed(self);
        #[cfg(feature = "playtime")]
        if let Some(m) = self.instance().borrow().clip_matrix() {
            m.notify_control_units_changed();
        }
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

    /// The idea is that this instance removes each auto unit candidate from the given map if it
    /// feels like it shouldn't be loaded. Which can be the case if it wants to use its input/output
    /// in another way (global auto unit override).
    pub fn judge_auto_unit_candidates(
        &self,
        auto_unit_candidates: &mut NonCryptoHashMap<String, AutoUnitData>,
    ) {
        // TODO-medium In future, we can build local auto units here, that is, check if the
        //  controller should be used in a different way in this instance and create an appropriate
        //  auto unit. We should do that only if the instance belongs to the current project.
        auto_unit_candidates.retain(|_, auto_unit_candidate| {
            if !self
                .main_unit_shell
                .model()
                .borrow()
                .is_fine_with_global_auto_unit(auto_unit_candidate)
            {
                return false;
            }
            let additional_unit_shells = blocking_read_lock(
                &self.additional_unit_shells,
                "participate_in_global_auto_unit_calculation",
            );
            if !additional_unit_shells.iter().all(|s| {
                s.model()
                    .borrow()
                    .is_fine_with_global_auto_unit(auto_unit_candidate)
            }) {
                return false;
            }
            true
        });
    }

    /// Here the instance gets the chance to load some of the given auto units. If it decides to
    /// load it, it must remove it from the map in order to prevent the unit to be loaded
    /// in other instances as well (would cause input/output conflicts).
    pub fn apply_auto_units(
        &self,
        desired_auto_units: &mut NonCryptoHashMap<String, AutoUnitData>,
    ) -> anyhow::Result<()> {
        if !self.settings.get().borrow().control.global_control_enabled {
            // Global control is not enabled. Remove auto units if some exist.
            blocking_write_lock(&self.additional_unit_shells, "apply_auto_units")
                .retain(|u| u.model().borrow().auto_unit().is_none());
            self.notify_units_changed();
            return Ok(());
        }
        {
            // Include only auto units that are suitable for this instance
            let mut additional_unit_shells =
                blocking_write_lock(&self.additional_unit_shells, "apply_auto_units");
            // At first we update or remove any existing auto units
            additional_unit_shells.retain_mut(|unit_shell| {
                let mut unit_model = unit_shell.model().borrow_mut();
                if let Some(existing_auto_unit) = unit_model.auto_unit() {
                    // This is an existing auto unit
                    if let Some(matching_auto_unit) = self.remove_auto_unit_if_requirements_met(
                        desired_auto_units,
                        &existing_auto_unit.controller_id,
                    ) {
                        // The existing auto unit must be updated
                        unit_model.update_auto_unit(matching_auto_unit);
                        true
                    } else {
                        // The existing auto unit is obsolete
                        false
                    }
                } else {
                    // This is a manual unit
                    true
                }
            });
            // All required auto units that are still left must be added
            desired_auto_units.retain(|_, auto_unit| {
                if !self.has_all_features_required_by_main_preset(&auto_unit.main_preset_id) {
                    // Our instance doesn't satisfy the requirements. Don't consume auto unit.
                    return true;
                }
                tracing::debug!(msg = "Creating auto-unit shell", ?auto_unit);
                let unit_shell = self.create_additional_unit_shell(Some(auto_unit.clone()));
                additional_unit_shells.push(unit_shell);
                false
            });
        }
        self.notify_units_changed();
        Ok(())
    }

    /// Returns the state of the current unit in serialized form.
    ///
    /// This must not lock any data that's accessed from real-time threads.
    ///
    /// Must be called from the main thread.
    ///
    /// # Errors
    ///
    /// This fails if the instance is already mutably borrowed (reentrancy issue).
    pub fn save(&self) -> anyhow::Result<Vec<u8>> {
        let instance_data = self
            .create_data()
            .context("couldn't create instance data")?;
        let bytes =
            serde_json::to_vec(&instance_data).context("couldn't serialize instance data")?;
        Ok(bytes)
    }

    fn remove_auto_unit_if_requirements_met(
        &self,
        auto_units: &mut NonCryptoHashMap<String, AutoUnitData>,
        controller_id: &str,
    ) -> Option<AutoUnitData> {
        let auto_unit = auto_units.get(controller_id)?;
        if self.has_all_features_required_by_main_preset(&auto_unit.main_preset_id) {
            auto_units.remove(controller_id)
        } else {
            None
        }
    }

    fn has_all_features_required_by_main_preset(&self, main_preset_id: &str) -> bool {
        let main_preset_manager = BackboneShell::get().main_preset_manager().borrow();
        let Some(preset) = main_preset_manager.find_preset_info_by_id(main_preset_id) else {
            return false;
        };
        preset
            .specific_meta_data
            .required_features
            .iter()
            .all(|f| self.has_feature(f))
    }

    fn has_feature(&self, feature: &str) -> bool {
        match feature {
            instance_features::PLAYTIME => self.instance.get().borrow().has_clip_matrix(),
            _ => false,
        }
    }

    /// # Errors
    ///
    /// This fails if the instance is already mutably borrowed (reentrancy issue).
    pub fn create_data(&self) -> anyhow::Result<InstanceData> {
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
        let unit_model = self.main_unit_shell.model().try_borrow()?;
        let data = InstanceData {
            main_unit: UnitData::from_model(&unit_model),
            additional_units: additional_unit_datas,
            settings: self.settings.get().borrow().clone(),
            pot_state: instance.save_pot_unit(),
            // TODO-medium Subject to improvement: If ReaLearn has been compiled without playtime feature, a
            //  potential Playtime matrix in the instance data will be silently discarded and disappear when saving
            //  next time. It would be better to memorize the data in the shell.
            clip_matrix: {
                #[cfg(feature = "playtime")]
                {
                    instance.clip_matrix().map(|matrix| {
                        crate::infrastructure::data::ClipMatrixRefData::Own(Box::new(matrix.save()))
                    })
                }
                #[cfg(not(feature = "playtime"))]
                None
            },
            custom_data: instance.custom_data().clone(),
        };
        Ok(data)
    }

    /// Restores instance shell state from the given serialized data.
    ///
    /// This must not lock any data that's accessed from real-time threads.
    ///
    /// To be called from main thread.
    pub fn load(self: SharedInstanceShell, data: &[u8]) -> anyhow::Result<()> {
        let left_json_object_brace = data
            .iter()
            .position(|b| *b == 0x7b)
            .context("couldn't find left JSON brace in bank data")?;
        // ReaLearn C++ saved some IPlug binary data in front of the actual JSON object. Find
        // start of JSON data.
        let data = &data[left_json_object_brace..];
        let instance_data = InstanceData::parse(data)?;
        self.apply_data_internal(instance_data)?;
        Ok(())
    }

    pub fn apply_data(
        self: SharedInstanceShell,
        instance_data: InstanceData,
    ) -> anyhow::Result<()> {
        self.apply_data_internal(instance_data)
    }

    fn apply_data_internal(
        self: SharedInstanceShell,
        instance_data: InstanceData,
    ) -> anyhow::Result<()> {
        let instance = self.instance();
        // General properties
        *self.settings.get().borrow_mut() = instance_data.settings;
        // Pot state
        instance
            .borrow_mut()
            .restore_pot_unit(instance_data.pot_state.clone());
        // Custom data
        instance
            .borrow_mut()
            .set_custom_data(instance_data.custom_data);
        // Playtime matrix
        #[cfg(feature = "playtime")]
        {
            if let Some(matrix_ref) = instance_data.clip_matrix {
                match matrix_ref {
                    crate::infrastructure::data::ClipMatrixRefData::Own(m) => {
                        let mut instance = self.instance().borrow_mut();
                        self.clone()
                            .get_or_insert_owned_clip_matrix(&mut instance)?
                            .load(*m)?;
                    }
                    crate::infrastructure::data::ClipMatrixRefData::Foreign(_) => {
                        bail!("Referring to the Playtime matrix of another instance is not supported anymore!");
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
        update_auto_units_async();
        // Inform hub
        BackboneShell::get()
            .proto_hub()
            .notify_everything_in_instance_has_changed(self.instance_id);
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
    pub fn run_from_plugin(&self, host: HostCallback) {
        let Some(unit_shells) = non_blocking_try_read_lock(&self.additional_unit_shells) else {
            // Better miss one block than blocking the entire audio thread
            return;
        };
        for unit_shell in once(&self.main_unit_shell).chain(&*unit_shells) {
            unit_shell.run_from_vst(host);
        }
    }

    #[cfg(feature = "playtime")]
    pub fn run_playtime_from_plugin(
        &self,
        buffer: &mut vst::buffer::AudioBuffer<f64>,
        block_props: crate::domain::AudioBlockProps,
    ) {
        base::non_blocking_lock(&*self.rt_instance, "RealTimeInstance")
            .run_from_vst(buffer, block_props);
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

    /// Fails if Playtime feature is not available.
    pub fn insert_owned_clip_matrix_if_necessary(self: SharedInstanceShell) -> anyhow::Result<()> {
        #[cfg(not(feature = "playtime"))]
        {
            bail!("Playtime feature not available")
        }
        #[cfg(feature = "playtime")]
        {
            let mut instance = self.instance.get().borrow_mut();
            if instance.clip_matrix().is_some() {
                return Ok(());
            }
            self.clone()
                .get_or_insert_owned_clip_matrix(&mut instance)?;
            // For convenience, we automatically switch global control on. This, combined with automatically creating
            // a controller for a known device (see `maybe_create_controller_for_device`) leads to newly connected
            // controllers working automagically!
            self.change_settings(|settings| settings.control.global_control_enabled = true);
            Ok(())
        }
    }

    pub fn load_clip_matrix(
        self: SharedInstanceShell,
        matrix: Option<FlexibleMatrix>,
    ) -> anyhow::Result<()> {
        #[cfg(not(feature = "playtime"))]
        {
            let _ = matrix;
            bail!("Playtime feature not enabled");
        }
        #[cfg(feature = "playtime")]
        {
            let mut instance = self.instance().borrow_mut();
            if let Some(matrix) = matrix {
                self.clone()
                    .get_or_insert_owned_clip_matrix(&mut instance)?
                    .load(matrix)?;
            } else {
                instance.set_clip_matrix(None);
            }
            Ok(())
        }
    }

    /// Returns and - if necessary - installs an owned Playtime matrix from/into the given instance.
    ///
    /// If this instance already contains an owned Playtime matrix, returns it. If not, creates
    /// and installs one, removing a possibly existing foreign matrix reference.
    #[cfg(feature = "playtime")]
    pub fn get_or_insert_owned_clip_matrix(
        self: SharedInstanceShell,
        instance: &mut Instance,
    ) -> anyhow::Result<&mut playtime_clip_engine::base::Matrix> {
        let main_unit_model = Rc::downgrade(self.main_unit_shell.model());
        let weak_instance_shell = Arc::downgrade(&self);
        let create_handler =
            move |instance: &Instance| -> Box<dyn playtime_clip_engine::base::ClipMatrixHandler> {
                let handler = crate::infrastructure::plugin::MatrixHandler::new(
                    instance.id(),
                    instance.audio_hook_task_sender.clone(),
                    instance.real_time_instance_task_sender.clone(),
                    instance.playtime.clip_matrix_event_sender.clone(),
                    weak_instance_shell,
                    main_unit_model,
                );
                Box::new(handler)
            };
        instance.create_and_install_clip_matrix_if_necessary(create_handler)?;
        Ok(instance.clip_matrix_mut().unwrap())
    }
}
