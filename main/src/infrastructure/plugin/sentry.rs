use crate::infrastructure::plugin::remote_config::SentryRemoteConfig;
use reaper_high::{PluginInfo, Reaper, SentryConfig};

pub fn init_sentry(
    plugin_info: &PluginInfo,
    remote_config: &SentryRemoteConfig,
) -> anyhow::Result<()> {
    let sentry_config = SentryConfig {
        plugin_info,
        dsn: remote_config.dsn.parse()?,
        in_app_include: vec!["helgobox"],
    };
    Reaper::get().init_sentry(sentry_config);
    Ok(())
}
