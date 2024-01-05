use crate::persistence::ControllerRoleKind;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct InstanceSettings {
    // Would have liked to use an EnumSet here but I couldn't make it serialize as list
    // by using #[enumset(serialize_repr = "list")], no idea why.
    pub auto_loaded_controller_roles: HashSet<ControllerRoleKind>,
}
