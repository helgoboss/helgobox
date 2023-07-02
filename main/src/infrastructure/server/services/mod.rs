#[cfg(feature = "playtime")]
pub mod playtime_service;

pub struct RealearnServices {
    #[cfg(feature = "playtime")]
    pub playtime_service: playtime_service::PlaytimeService,
}
