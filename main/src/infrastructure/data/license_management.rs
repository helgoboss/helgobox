use anyhow::{anyhow, Context};
use helgoboss_license_api::persistence::{LicenseData, LicenseKey};
use helgoboss_license_api::runtime::License;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

pub type SharedLicenseManager = Rc<RefCell<LicenseManager>>;

pub trait LicenseManagerEventHandler: Debug {
    fn licenses_changed(&self, source: &LicenseManager);
}

#[derive(Debug)]
pub struct LicenseManager {
    licensing_file_path: PathBuf,
    licensing: Licensing,
    event_handler: Box<dyn LicenseManagerEventHandler>,
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
    pub fn new(
        licensing_file_path: PathBuf,
        event_handler: Box<dyn LicenseManagerEventHandler>,
    ) -> Self {
        let mut manager = Self {
            licensing_file_path,
            licensing: Default::default(),
            event_handler,
        };
        let _ = manager.load();
        manager
    }

    pub fn licenses(&self) -> &[License] {
        &self.licensing.licenses
    }

    pub fn add_license(&mut self, key: LicenseKey) -> anyhow::Result<()> {
        let license_data = LicenseData::try_from_key(&key)?;
        let license = License::try_from(license_data)?;
        self.licensing.licenses.push(license);
        self.save()?;
        self.event_handler.licenses_changed(self);
        Ok(())
    }

    fn load(&mut self) -> anyhow::Result<()> {
        let json = fs::read_to_string(&self.licensing_file_path)
            .with_context(|| "couldn't read licensing file")?;
        let data: LicensingData =
            serde_json::from_str(&json).with_context(|| "licensing file has wrong format")?;
        self.licensing = data.into();
        Ok(())
    }

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
