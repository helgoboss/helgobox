use anyhow::{anyhow, Context};
use helgoboss_license_api::persistence::{LicenseData, LicenseKey};
use helgoboss_license_api::runtime::License;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct LicenseManager {
    licensing_file_path: PathBuf,
    licensing: Licensing,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LicensingData {
    license_keys: Vec<LicenseKey>,
}

#[derive(Clone, Debug, Default)]
struct Licensing {
    licenses: Vec<License>,
}

impl LicenseManager {
    /// Creates a manager using the given licensing file.
    ///
    /// Immediately loads the licenses from the licensing file if it exists.
    pub fn new(licensing_file_path: PathBuf) -> Self {
        let mut manager = Self {
            licensing_file_path,
            licensing: Default::default(),
        };
        let _ = manager.load();
        manager
    }

    fn load(&mut self) -> anyhow::Result<()> {
        let json = fs::read_to_string(&self.licensing_file_path)
            .with_context(|| "couldn't read licensing file")?;
        let data: LicensingData =
            serde_json::from_str(&json).with_context(|| "licensing file has wrong format")?;
        self.licensing = data.into();
        Ok(())
    }

    #[allow(dead_code)]
    fn save(&mut self) -> anyhow::Result<()> {
        let data: LicensingData = self.licensing.clone().into();
        let json = serde_json::to_string_pretty(&data)
            .with_context(|| "couldn't serialize OSC device config")?;
        let parent_dir = self
            .licensing_file_path
            .parent()
            .ok_or_else(|| anyhow!("wrong licensing path"))?;
        fs::create_dir_all(parent_dir)
            .with_context(|| "couldn't create licensing file parent directory")?;
        fs::write(&self.licensing_file_path, json)
            .with_context(|| "couldn't write licensing file")?;
        Ok(())
    }

    pub fn licenses(&self) -> impl Iterator<Item = &License> + ExactSizeIterator {
        self.licensing.licenses.iter()
    }
}

impl From<LicensingData> for Licensing {
    fn from(value: LicensingData) -> Self {
        Self {
            licenses: value
                .license_keys
                .into_iter()
                .filter_map(|key| License::try_from(LicenseData::try_from_key(&key).ok()?).ok())
                .collect(),
        }
    }
}

impl From<Licensing> for LicensingData {
    fn from(value: Licensing) -> Self {
        Self {
            license_keys: value
                .licenses
                .into_iter()
                .map(|l| LicenseData::from(l).to_key())
                .collect(),
        }
    }
}
