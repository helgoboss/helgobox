#[macro_use]
mod tracing_util;

pub mod main;
pub mod rt;
pub use timeline::*;

mod metrics_util;

mod timeline;

mod file_util;

mod conversion_util;

type ClipEngineResult<T> = Result<T, &'static str>;

/// Must be called as early as possible.
///
/// - before creating a matrix
/// - preferably in the main thread
pub fn init() {
    metrics_util::init_metrics();
}
