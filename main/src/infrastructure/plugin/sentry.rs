use reaper_high::{PluginInfo, Reaper, SentryConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct HelgoboxRemoteConfig {
    sentry: SentryRemoteConfig,
}

#[derive(Serialize, Deserialize, Debug)]
struct SentryRemoteConfig {
    dsn: String,
}

/// Initializes Sentry error reporting.
pub async fn init_sentry(plugin_info: PluginInfo) {
    if let Err(e) = init_sentry_internal(&plugin_info).await {
        tracing::warn!(msg = "Couldn't initialize Sentry error reporting", ?e)
    }
}

async fn init_sentry_internal(plugin_info: &PluginInfo) -> anyhow::Result<()> {
    let response = reqwest::get("https://helgoboss.org/projects/helgobox/config.yaml").await?;
    let yaml = response.text().await?;
    let remote_config: HelgoboxRemoteConfig = serde_yaml::from_str(&yaml)?;
    let sentry_config = SentryConfig {
        plugin_info,
        dsn: remote_config.sentry.dsn.parse()?,
        in_app_include: vec!["helgobox"],
    };
    Reaper::get().init_sentry(sentry_config);
    Ok(())
}
