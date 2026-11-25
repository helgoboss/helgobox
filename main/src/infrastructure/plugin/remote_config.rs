use image::RgbaImage;
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct HelgoboxRemoteConfig {
    pub sentry: SentryRemoteConfig,
    pub plugin: PluginRemoteConfig,
    pub app: AppRemoteConfig,
    pub announcement: Option<Announcement>,
}

#[derive(Debug)]
pub struct AugmentedHelgoboxRemoteConfig {
    pub remote_config: HelgoboxRemoteConfig,
    pub announcement_image: Option<RgbaImage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Announcement {
    pub id: String,
    pub image_url: String,
    pub target_url: Option<String>,
    pub audience: AnnouncementAudience,
}

impl Announcement {
    pub fn audience_matches(&self) -> bool {
        match self.audience {
            AnnouncementAudience::Everyone => true,
            AnnouncementAudience::PlaytimeUnlicensed => {
                #[cfg(feature = "playtime")]
                {
                    let engine = playtime_clip_engine::PlaytimeEngine::get().main();
                    !engine.has_valid_license()
                }
                #[cfg(not(feature = "playtime"))]
                {
                    true
                }
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncementAudience {
    #[default]
    Everyone,
    PlaytimeUnlicensed,
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

impl AugmentedHelgoboxRemoteConfig {
    pub async fn fetch() -> anyhow::Result<Self> {
        let response = reqwest::get("https://helgoboss.org/projects/helgobox/config.yaml").await?;
        let yaml = response.text().await?;
        let remote_config: HelgoboxRemoteConfig = serde_yaml::from_str(&yaml)?;
        let announcement_image = if let Some(announcement) = &remote_config.announcement {
            fetch_image(&announcement.image_url).await.ok()
        } else {
            None
        };
        let augmented = AugmentedHelgoboxRemoteConfig {
            announcement_image,
            remote_config,
        };
        Ok(augmented)
    }
}

async fn fetch_image(url: &str) -> anyhow::Result<RgbaImage> {
    let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
    let image = image::load_from_memory(&bytes)?.into_rgba8();
    Ok(image)
}
