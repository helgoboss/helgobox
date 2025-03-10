use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct HelgoboxRemoteConfig {
    pub sentry: SentryRemoteConfig,
    pub plugin: PluginRemoteConfig,
    pub app: AppRemoteConfig,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SentryRemoteConfig {
    pub dsn: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginRemoteConfig {
    pub latest_version: Version,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AppRemoteConfig {
    pub latest_version: Version,
}

impl HelgoboxRemoteConfig {
    pub async fn fetch() -> anyhow::Result<Self> {
        let response = reqwest::get("https://helgoboss.org/projects/helgobox/config.yaml").await?;
        let yaml = response.text().await?;
        let remote_config: HelgoboxRemoteConfig = serde_yaml::from_str(&yaml)?;
        Ok(remote_config)
    }
}
