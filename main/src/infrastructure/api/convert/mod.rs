pub mod defaults;
pub mod from_data;
pub mod to_data;

type ConversionResult<T> = Result<T, Box<dyn std::error::Error>>;

fn convert_multiple<A, B>(
    input: Vec<A>,
    f: impl Fn(A) -> ConversionResult<B>,
) -> ConversionResult<Vec<B>> {
    input.into_iter().map(f).collect()
}
