use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use validator::Validate;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct License {
    payload: LicensePayload,
    signature: String,
}

impl License {
    pub fn new(payload: LicensePayload, signature: String) -> Self {
        Self { payload, signature }
    }

    pub fn payload(&self) -> &LicensePayload {
        &self.payload
    }

    pub fn signature(&self) -> &str {
        &self.signature
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Validate)]
pub struct LicensePayloadInput {
    #[validate(length(min = 1))]
    pub product_id: String,
    #[validate(length(min = 1))]
    pub name: String,
    #[validate(email)]
    pub email: String,
    pub version: u32,
    pub kind: LicenseKind,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct LicensePayload {
    product_id: String,
    name: String,
    email: String,
    version: u32,
    kind: LicenseKind,
    /// Unix timestamp (seconds since 1970-01-01 00:00:00).
    created_on: u64,
}

impl LicensePayload {
    pub fn new(input: LicensePayloadInput) -> Result<Self, Box<dyn Error>> {
        input.validate()?;
        let payload = Self {
            product_id: input.product_id,
            name: input.name,
            email: input.email,
            version: input.version,
            kind: input.kind,
            created_on: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        };
        Ok(payload)
    }

    pub fn product_id(&self) -> &str {
        &self.product_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn kind(&self) -> &LicenseKind {
        &self.kind
    }

    pub fn created_on(&self) -> u64 {
        self.created_on
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum LicenseKind {
    Personal,
    Business,
}
