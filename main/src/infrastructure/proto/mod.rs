#[allow(clippy::enum_variant_names)]
mod generated;
pub use generated::*;

mod ext;

#[cfg(feature = "playtime")]
mod playtime_ext;

mod service_impl;
pub use service_impl::*;

mod request_handler;
pub use request_handler::*;

#[cfg(feature = "playtime")]
mod playtime_request_handler;
#[cfg(feature = "playtime")]
pub use playtime_request_handler::*;

mod hub;
pub use hub::*;

mod senders;
pub use senders::*;

mod initial_events;
pub use initial_events::*;

#[cfg(feature = "playtime")]
mod initial_playtime_events;
#[cfg(feature = "playtime")]
pub use initial_playtime_events::*;

/// The app will check this version and bail if it's not compatible!
///
/// Its major number must be increased whenever there's a breaking change in the API that we provide
/// to the app. This includes the Protobuf/gRPC API and things related to embedding (such as the
/// way how the host callback works).
///
/// It's important to get this right to get good error messages from customers. Knowing
/// that they just have an incompatible version mix (e.g., due to manual installing) makes it
/// trivial to respond to bug reports.
pub const HOST_API_VERSION: &str = "15.0.0";
