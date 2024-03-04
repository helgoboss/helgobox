use crate::infrastructure::plugin::{BackboneShell, InstanceShell};
use crate::infrastructure::proto::{
    occasional_global_update, occasional_instance_update, OccasionalGlobalUpdate,
    OccasionalInstanceUpdate,
};

pub fn create_initial_global_updates() -> Vec<OccasionalGlobalUpdate> {
    use occasional_global_update::Update;
    fn create(updates: impl Iterator<Item = Update>) -> Vec<OccasionalGlobalUpdate> {
        updates
            .into_iter()
            .map(|u| OccasionalGlobalUpdate { update: Some(u) })
            .collect()
    }
    let global_updates = [
        Update::midi_input_devices(),
        Update::midi_output_devices(),
        // TODO-high-ms3 Update when changed
        Update::audio_input_channels(),
        Update::controller_presets(&BackboneShell::get().controller_preset_manager().borrow()),
        Update::main_presets(&BackboneShell::get().main_preset_manager().borrow()),
        Update::controller_config(&BackboneShell::get().controller_manager().borrow()),
        Update::license_info(&BackboneShell::get().license_manager().borrow()),
        Update::playtime_license_state(),
    ];
    create(global_updates.into_iter())
}

pub fn create_initial_instance_updates(
    instance_shell: &InstanceShell,
) -> Vec<OccasionalInstanceUpdate> {
    use occasional_instance_update::Update;
    fn create(updates: impl Iterator<Item = Update>) -> Vec<OccasionalInstanceUpdate> {
        updates
            .into_iter()
            .map(|u| OccasionalInstanceUpdate { update: Some(u) })
            .collect()
    }
    let instance_updates = [Update::settings(instance_shell)];
    create(instance_updates.into_iter())
}
