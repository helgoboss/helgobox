use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InfoEvent {
    AutoAddedController(AutoAddedControllerEvent),
    PlaytimeActivatedSuccessfully,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct AutoAddedControllerEvent {
    pub controller_id: String,
}
