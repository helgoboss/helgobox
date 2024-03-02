use helgoboss_license_api::persistence::LicenseData;
use serde::Serialize;

#[derive(Clone, Eq, PartialEq, Debug, Serialize)]
pub struct LicenseInfo {
    pub licenses: Vec<ValidatedLicense>,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize)]
pub struct ValidatedLicense {
    pub license: LicenseData,
    pub valid: bool,
}
