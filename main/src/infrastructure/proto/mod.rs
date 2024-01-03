mod helgobox_proto;
pub use helgobox_proto::*;

mod helgobox_proto_ext;
pub use helgobox_proto_ext::*;

mod grpc_service;
pub use grpc_service::*;

mod request_handler;
pub use request_handler::*;

mod hub;
pub use hub::*;

mod senders;
pub use senders::*;

mod initial_events;
pub use initial_events::*;

/// The app will check this version and bail if it's not compatible!
///
/// Its major number must be increased whenever there's a breaking change in the API that we provide
/// to the app. This includes the Protobuf/gRPC API and things related to embedding (such as the
/// way how the host callback works).
///
/// It's important to get this right in order to get good error messages from customers. Knowing
/// that they just have an incompatible version mix (e.g. due to manual installing) makes it
/// trivial to respond to bug reports.
pub const HOST_API_VERSION: &str = "1.0.0";
