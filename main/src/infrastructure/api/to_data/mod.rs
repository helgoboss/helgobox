mod compartment;
mod glue;
mod group;
mod mapping;
mod parameter;
mod source;
mod target;

pub use compartment::*;
pub use mapping::*;
use source::*;

type ConversionResult<T> = Result<T, Box<dyn std::error::Error>>;

fn convert_multiple<A, B>(
    input: Vec<A>,
    f: impl Fn(A) -> ConversionResult<B>,
) -> ConversionResult<Vec<B>> {
    input.into_iter().map(f).collect()
}
