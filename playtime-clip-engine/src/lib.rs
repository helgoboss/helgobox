#[macro_use]
mod tracing_util;

mod application;
mod processing;

mod metrics_util;

pub use application::matrix::*;

pub use processing::real_time_matrix::*;

mod timeline;
pub use timeline::*;

pub use application::clip_data::*;

use reaper_high::{Project, Reaper};
use reaper_medium::{MeasureMode, PositionInBeats, PositionInSeconds};

pub use application::clip_content::*;

pub use processing::buffer::*;

pub use processing::supplier::*;

pub use application::column::*;

pub use processing::column_source::*;

pub use processing::slot::*;

use crate::metrics_util::init_metrics;
pub use processing::clip::*;

mod file_util;

mod conversion_util;

pub type ClipEngineResult<T> = Result<T, &'static str>;

/// Must be called as early as possible.
///
/// - before creating a matrix
/// - preferably in the main thread
pub fn init() {
    init_metrics();
}
