#[macro_use]
mod tracing_util;

pub mod main;
pub mod rt;

mod metrics_util;

mod timeline;
pub use timeline::*;

mod file_util;

mod conversion_util;

mod mutex_util;

pub mod midi_util;

type ClipEngineResult<T> = Result<T, &'static str>;

pub struct ErrorWithPayload<T> {
    pub message: &'static str,
    pub payload: T,
}

impl<T> ErrorWithPayload<T> {
    pub const fn new(message: &'static str, payload: T) -> Self {
        Self { message, payload }
    }

    pub fn map_payload<R>(self, f: impl FnOnce(T) -> R) -> ErrorWithPayload<R> {
        ErrorWithPayload {
            payload: f(self.payload),
            message: self.message,
        }
    }
}

/// Must be called as early as possible.
///
/// - before creating a matrix
/// - preferably in the main thread
pub fn init() {
    metrics_util::init_metrics();
}
