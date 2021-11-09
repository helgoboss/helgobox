use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Internal technical group identifier, not persistent.
///
/// Goals: Quick lookup, guaranteed uniqueness, cheap copy
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

impl FromStr for GroupId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = Uuid::from_str(s).map_err(|_| "group ID must be a valid UUID")?;
        Ok(Self { uuid })
    }
}

/// A potentially user-defined group identifier, persistent
///
/// Goals: For external references (e.g. from API or in projection)
#[derive(
    Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Display, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct GroupKey(String);

impl GroupKey {
    pub fn random() -> Self {
        Self(nanoid::nanoid!())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for GroupKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for GroupKey {
    fn from(v: String) -> Self {
        Self(v)
    }
}

impl From<GroupKey> for String {
    fn from(v: GroupKey) -> Self {
        v.0
    }
}
