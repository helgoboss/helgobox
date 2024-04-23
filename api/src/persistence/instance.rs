use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub control: InstanceControlSettings,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct InstanceControlSettings {
    /// Whether auto units will be created for all controllers that have a main preset set.
    pub global_control_enabled: bool,
    // Local overrides of controller settings.
    //
    // If global control is enabled, each override will alter the behavior of the corresponding
    // controller.
    //
    // If global control is disabled, each override which has a main preset set will enable
    // that specific controller. This way you can selectively enable controllers, either with
    // the global default preset or with your own one.
    //
    // TODO-high-playtime-after-release Controller overrides are not yet implemented because about doubts.
    //  What if the user deletes the controller? Then all project/instance that have
    //  an override of that controller will reference a now gone controller. Consequently, the
    //  overrides will not work anymore. Ideas:
    //  1. Memorize the original controller data as part of the override and update it whenever
    //     the global controller changes. Then we can use that data if the controller is gone.
    //     => the project will still work but it will be disconnected.
    //  2. Don't actually use the global controller anymore once there's an override ... that's like
    //     disconnecting immediately.
    //  3. GOOD SOLUTION FOR NOW Don't provide the possibility for overrides. Force user to create
    //     a ReaLearn setup that is completely self-containing (the other extreme instead of something
    //     in-between), including the preset content.
    //  4. Is the controller role idea better after all? I don't think so. Yes, it allows a bit
    //     more instance-specific tuning of global control without depending on particular
    //     controllers. However, the kind of tuning that it allows is far from exhaustive. Also,
    //     it's opinionated (clip/daw roles) and has other issues (being harder to grasp and
    //     awkward when it comes to all-in-one controllers that do both clip/DAW control).
    //  5. INTERESTING If all we need is the possibility to disable e.g. global DAW control for a
    //     specific instance, we could simply let the main preset declare which usage role
    //     it implements (e.g. the "DAW control" role) and allow the instance to switch on/off
    //     roles - which will cause the main preset to be loaded or not.
    // pub controller_overrides: Vec<ControllerOverride>,
}

// #[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
// pub struct ControllerOverride {
//     /// ID of the controller which should be overridden.
//     pub controller_id: String,
//     /// If this is `None`, the controller default main preset will be used.
//     pub main_preset: Option<CompartmentPresetId>,
// }
