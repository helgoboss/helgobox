use crate::runtime::{License, LicenseKind, LicensePayload, LicensedProduct};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use validator::{Validate, ValidationError};

/// Data of a license, suitable for being persisted, can be invalid.
///
/// Serialization and deserialization must be backward-compatible because we persist this on disk!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Validate, Serialize, Deserialize)]
pub struct LicenseData {
    #[validate]
    pub payload: LicensePayloadData,
    #[validate(length(min = 1))]
    pub signature: String,
}

/// Data of a license payload, suitable for being persisted, can be invalid.
///
/// Serialization and deserialization must be backward-compatible because we persist this on disk!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Validate, Serialize, Deserialize)]
pub struct LicensePayloadData {
    #[validate(length(min = 1))]
    pub name: String,
    #[validate(email)]
    pub email: String,
    pub kind: LicenseKind,
    #[validate(length(min = 1))]
    #[validate]
    pub products: Vec<LicensedProductData>,
}

/// Data of a licensed product, suitable for being persisted, can be invalid.
///
/// Serialization and deserialization must be backward-compatible because we persist this on disk!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Validate, Serialize, Deserialize)]
#[validate(schema(function = "validate_product"))]
pub struct LicensedProductData {
    #[validate(length(min = 1))]
    pub id: String,
    pub min_version: u32,
    pub max_version: u32,
}

impl From<License> for LicenseData {
    fn from(value: License) -> Self {
        Self {
            payload: value.payload().clone().into(),
            signature: hex::encode(value.signature()),
        }
    }
}

impl TryFrom<LicenseData> for License {
    type Error = Box<dyn Error>;

    fn try_from(data: LicenseData) -> Result<Self, Self::Error> {
        data.validate()?;
        let payload = data.payload.try_into()?;
        let signature = hex::decode(data.signature)?;
        Ok(License::new(payload, signature))
    }
}

impl From<LicensePayload> for LicensePayloadData {
    fn from(value: LicensePayload) -> Self {
        Self {
            name: value.name,
            email: value.email,
            kind: value.kind,
            products: value.products.into_iter().map(|p| p.into()).collect(),
        }
    }
}

impl TryFrom<LicensePayloadData> for LicensePayload {
    type Error = Box<dyn Error>;

    fn try_from(data: LicensePayloadData) -> Result<Self, Self::Error> {
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
}

impl From<LicensedProduct> for LicensedProductData {
    fn from(value: LicensedProduct) -> Self {
        Self {
            id: value.id,
            min_version: value.min_version,
            max_version: value.max_version,
        }
    }
}

fn validate_product(product: &LicensedProductData) -> Result<(), ValidationError> {
    if product.min_version > product.max_version {
        return Err(ValidationError::new("invalid_version_range"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_deserialization() -> Result<(), Box<dyn Error>> {
        // Given
        let license_data = LicenseData {
            payload: LicensePayloadData {
                name: "Joe".to_string(),
                email: "joe@example.org".to_string(),
                kind: LicenseKind::Personal,
                products: vec![LicensedProductData {
                    id: "foo".to_string(),
                    min_version: 1,
                    max_version: 1,
                }],
            },
            signature: "00010a".to_string(),
        };
        // When
        let license: License = license_data.try_into()?;
        // Then
        assert_eq!(license.payload().name(), "Joe");
        assert_eq!(license.payload().email(), "joe@example.org");
        assert!(license.payload().created_on() > 0);
        assert_eq!(license.payload().kind(), LicenseKind::Personal);
        let product = license.payload().products().first().ok_or("no product")?;
        assert_eq!(product.id(), "foo");
        assert_eq!(product.version_range(), 1..=1);
        assert_eq!(license.signature(), &[0x00, 0x01, 0x0a]);
        Ok(())
    }

    #[test]
    fn failed_deserialization() -> Result<(), Box<dyn Error>> {
        // Given
        let license_data = LicenseData {
            payload: LicensePayloadData {
                name: "Joe".to_string(),
                email: "joe".to_string(),
                kind: LicenseKind::Personal,
                products: vec![],
            },
            signature: "".to_string(),
        };
        // When
        let license: Result<License, _> = license_data.try_into();
        // Then
        license.expect_err("should error due to invalid data");
        Ok(())
    }

    #[test]
    fn successful_serialization() -> Result<(), Box<dyn Error>> {
        // Given
        let original_license_data = LicenseData {
            payload: LicensePayloadData {
                name: "Joe".to_string(),
                email: "joe@example.org".to_string(),
                kind: LicenseKind::Personal,
                products: vec![LicensedProductData {
                    id: "foo".to_string(),
                    min_version: 1,
                    max_version: 1,
                }],
            },
            signature: "00010a".to_string(),
        };
        let license: License = original_license_data.clone().try_into()?;
        // When
        let serialized_license_data: LicenseData = license.into();
        // Then
        assert_eq!(original_license_data, serialized_license_data);
        Ok(())
    }
}
