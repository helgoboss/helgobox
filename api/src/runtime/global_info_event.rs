use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GlobalInfoEvent {
    Generic(GenericGlobalInfoEvent),
    AutoAddedController(AutoAddedControllerEvent),
    PlaytimeActivationSucceeded,
    PlaytimeActivationFailed,
}

impl GlobalInfoEvent {
    pub fn generic(message: impl Into<String>) -> Self {
        Self::Generic(GenericGlobalInfoEvent {
            message: message.into(),
        })
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct GenericGlobalInfoEvent {
    pub message: String,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct AutoAddedControllerEvent {
    pub controller_id: String,
}
