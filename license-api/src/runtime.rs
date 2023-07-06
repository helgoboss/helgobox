use crate::persistence::LicensePayloadData;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::ops::RangeInclusive;
use std::time::{SystemTime, UNIX_EPOCH};
use validator::Validate;

/// A complete license (payload and signature).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct License {
    payload: LicensePayload,
    signature: String,
}

/// The payload of a license (who, how, when, what).
///
/// Serialization must be deterministic because the signature is built from it!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize)]
pub struct LicensePayload {
    name: String,
    email: String,
    kind: LicenseKind,
    /// Unix timestamp (seconds since 1970-01-01 00:00:00).
    created_on: u64,
    products: Vec<LicensedProduct>,
}

/// A license kind (personal or business).
///
/// Serialization must be deterministic because the signature is built from it!
///
/// Serialization and deserialization must be deterministic because we persist this on disk!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum LicenseKind {
    Personal,
    Business,
}

/// A product that can be licensed.
///
/// Serialization must be deterministic because the signature is built from it!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize)]
pub struct LicensedProduct {
    id: String,
    min_version: u32,
    max_version: u32,
}

impl License {
    /// Creates a license.
    pub fn new(payload: LicensePayload, signature: String) -> Self {
        Self { payload, signature }
    }

    /// Returns the license payload.
    pub fn payload(&self) -> &LicensePayload {
        &self.payload
    }

    /// Returns the license signature.
    pub fn signature(&self) -> &str {
        &self.signature
    }
}

impl LicensePayload {
    /// Creates a license payload if the given data is valid.
    pub fn new(data: LicensePayloadData) -> Result<Self, Box<dyn Error>> {
        data.validate()?;
        let payload = Self {
            name: data.name,
            email: data.email,
            kind: data.kind,
            created_on: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            products: data
                .products
                .into_iter()
                .map(|data| LicensedProduct {
                    id: data.id,
                    min_version: data.min_version,
                    max_version: data.max_version,
                })
                .collect(),
        };
        Ok(payload)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn kind(&self) -> &LicenseKind {
        &self.kind
    }

    pub fn created_on(&self) -> u64 {
        self.created_on
    }

    pub fn products(&self) -> &[LicensedProduct] {
        &self.products
    }
}

impl LicensedProduct {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn version_range(&self) -> RangeInclusive<u32> {
        self.min_version..=self.max_version
    }
}
