#[cfg(feature = "playtime")]
pub mod helgobox_service;

pub struct Services {
    #[cfg(feature = "playtime")]
    pub helgobox_service: helgobox_service::DefaultHelgoboxServiceServer,
}
