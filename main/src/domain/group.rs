use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct GroupId {
    uuid: Uuid,
}

impl GroupId {
    pub fn is_default(&self) -> bool {
        self.uuid.is_nil()
    }

    pub fn random() -> GroupId {
        GroupId {
            uuid: Uuid::new_v4(),
        }
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}
