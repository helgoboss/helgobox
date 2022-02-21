mod metrics_util;

mod matrix;
pub use matrix::*;

mod real_time_matrix;
pub use real_time_matrix::*;

mod timeline;
pub use timeline::*;

mod clip_data;
pub use clip_data::*;

use reaper_high::{Project, Reaper};
use reaper_medium::{MeasureMode, PositionInBeats, PositionInSeconds};

mod clip_content;
pub use clip_content::*;

mod source_util;

mod buffer;
pub use buffer::*;

mod supplier;
pub use supplier::*;

mod column;
pub use column::*;

mod column_source;
pub use column_source::*;

mod slot;
pub use slot::*;

mod clip;
use crate::metrics_util::init_metrics;
pub use clip::*;

mod tempo_util;

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
