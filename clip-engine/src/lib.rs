mod matrix;
pub use matrix::*;

mod timeline;
pub use timeline::*;

mod legacy_clip;
pub use legacy_clip::*;

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
pub use clip::*;

mod tempo_util;

mod file_util;

mod conversion_util;

pub type ClipEngineResult<T> = Result<T, &'static str>;
