use crate::infrastructure::plugin::{BackboneShell, InstanceShell};
use crate::infrastructure::proto::{
    occasional_global_update, occasional_instance_update, qualified_occasional_unit_update,
    OccasionalGlobalUpdate, OccasionalInstanceUpdate, QualifiedOccasionalUnitUpdate, Scope,
    Severity,
};
use reaper_high::Reaper;
use std::iter;

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
        Update::host_color_scheme(),
        Update::arrangement_play_state(Reaper::get().current_project().play_state()),
        // TODO-high-playtime-after-release Update when changed
        Update::audio_input_channels(),
        Update::resample_modes(),
        Update::pitch_shift_modes(),
        Update::controller_presets(&BackboneShell::get().controller_preset_manager().borrow()),
        Update::main_presets(&BackboneShell::get().main_preset_manager().borrow()),
        Update::controller_config(&BackboneShell::get().controller_manager().borrow()),
        Update::license_info(&BackboneShell::get().license_manager().borrow()),
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
    let fixed_instance_updates = [
        Update::settings(instance_shell),
        Update::units(instance_shell),
    ];
    let reaper_version = Reaper::get().version();
    let reaper_revision = reaper_version.revision();
    let mut warnings = vec![];
    if cfg!(feature = "playtime") && reaper_revision < MIN_REAPER_VERSION_FOR_PLAYTIME {
        let msg = format!("You are using REAPER version {reaper_revision}, which is not optimal for running Playtime. You may experience issues of all kinds! For an optimal experience, please upgrade to at least REAPER version {MIN_REAPER_VERSION_FOR_PLAYTIME}!");
        warnings.push(Update::warning(Severity::High, Some(Scope::Playtime), msg))
    }
    create(
        fixed_instance_updates
            .into_iter()
            .chain(iter::once(Update::warnings(warnings))),
    )
}

const MIN_REAPER_VERSION_FOR_PLAYTIME: &str = "7.17";

pub fn create_initial_unit_updates(
    instance_shell: &InstanceShell,
) -> Vec<QualifiedOccasionalUnitUpdate> {
    use qualified_occasional_unit_update::Update;
    instance_shell
        .all_unit_models()
        .map(|unit_model| {
            let unit_model = unit_model.borrow();
            QualifiedOccasionalUnitUpdate {
                unit_id: unit_model.unit_id().into(),
                update: Some(Update::controller_routing(&unit_model)),
            }
        })
        .collect()
}
