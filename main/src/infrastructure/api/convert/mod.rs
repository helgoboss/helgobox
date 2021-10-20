mod glue;
mod mapping;
mod source;

use glue::*;
pub use mapping::*;
use source::*;

type ConversionResult<T> = Result<T, Box<dyn std::error::Error>>;
