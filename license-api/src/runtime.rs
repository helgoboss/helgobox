use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// A complete license (payload and signature).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct License {
    payload: LicensePayload,
    signature: Vec<u8>,
}

/// The payload of a license (who, how, when, what).
///
/// Serialization must be deterministic because the signature is built from it!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize)]
pub struct LicensePayload {
    pub(crate) name: String,
    pub(crate) email: String,
    pub(crate) kind: LicenseKind,
    /// Unix timestamp (seconds since 1970-01-01 00:00:00).
    pub(crate) created_on: u64,
    pub(crate) products: Vec<LicensedProduct>,
}

/// A license kind (personal or business).
///
/// Serialization must be deterministic because the signature is built from it!
///
/// Serialization and deserialization must be backward-compatible because we persist this on disk!
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub enum LicenseKind {
    Personal,
    Business,
}

/// A product that can be licensed.
///
/// Serialization must be deterministic because the signature is built from it!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize)]
pub struct LicensedProduct {
    pub(crate) id: String,
    pub(crate) min_version: u32,
    pub(crate) max_version: u32,
}

impl License {
    /// Creates a license.
    pub fn new(payload: LicensePayload, signature: Vec<u8>) -> Self {
        Self { payload, signature }
    }

    /// Returns the license payload.
    pub fn payload(&self) -> &LicensePayload {
        &self.payload
    }

    /// Returns the license signature.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

impl LicensePayload {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn kind(&self) -> LicenseKind {
        self.kind
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn deterministic_serialization() -> Result<(), Box<dyn Error>> {
        // Given
        let payload = LicensePayload {
            name: "Joe".to_string(),
            email: "joe@example.org".to_string(),
            kind: LicenseKind::Personal,
            created_on: 1000,
            products: vec![LicensedProduct {
                id: "foo".to_string(),
                min_version: 1,
                max_version: 1,
            }],
        };
        // When
        let json_1 = serde_json::to_string(&payload)?;
        let json_2 = serde_json::to_string(&payload)?;
        // Then
        assert_eq!(json_1, json_2);
        Ok(())
    }
}
