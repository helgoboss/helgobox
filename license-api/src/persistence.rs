use crate::runtime::LicenseKind;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

/// Data of a license payload, suitable for being persisted, can be invalid.
///
/// Serialization and deserialization must be deterministic because we persist this on disk!
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
/// Serialization and deserialization must be deterministic because we persist this on disk!
#[derive(Clone, Eq, PartialEq, Hash, Debug, Validate, Serialize, Deserialize)]
#[validate(schema(function = "validate_product"))]
pub struct LicensedProductData {
    #[validate(length(min = 1))]
    pub id: String,
    pub min_version: u32,
    pub max_version: u32,
}

fn validate_product(product: &LicensedProductData) -> Result<(), ValidationError> {
    if product.min_version > product.max_version {
        return Err(ValidationError::new("invalid_version_range"));
    }
    Ok(())
}
