use crate::hash_util::NonCryptoHashSet;
use std::error::Error;
use std::fmt::Display;
use std::hash::Hash;

#[derive(Debug, derive_more::Display)]
pub struct ValidationError(String);

impl Error for ValidationError {}

#[allow(clippy::unnecessary_filter_map)]
pub fn ensure_no_duplicate<T>(list_label: &str, iter: T) -> Result<(), ValidationError>
where
    T: IntoIterator,
    T::Item: Eq + Hash + Display,
{
    use std::fmt::Write;
    let mut uniq = NonCryptoHashSet::default();
    let duplicates: NonCryptoHashSet<_> = iter
        .into_iter()
        .filter_map(|d| {
            if uniq.contains(&d) {
                Some(d)
            } else {
                uniq.insert(d);
                None
            }
        })
        .collect();
    if duplicates.is_empty() {
        Ok(())
    } else {
        let mut s = format!("Found the following duplicate {list_label}: ");
        for (i, d) in duplicates.into_iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            let _ = write!(&mut s, "{d}");
        }
        Err(ValidationError(s))
    }
}
