use sha1::{Digest, Sha1};
use walkdir::DirEntry;

pub fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Converts something like "test" to something like
/// "a9/4a/8fe5ccb19ba61c4c0873d391e987982fbbd3.RfxChain" for the purpose to not get too many
/// files in one directory.
pub fn hash_to_dir_structure(input: impl AsRef<[u8]>, suffix: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input);
    let array = hasher.finalize();
    format!(
        "{:x}/{:x}/{}{suffix}",
        array[0],
        array[1],
        hex::encode(&array[2..])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_to_dir_structure_simple() {
        assert_eq!(
            hash_to_dir_structure("test", ".RfxChain"),
            "a9/4a/8fe5ccb19ba61c4c0873d391e987982fbbd3.RfxChain".to_string()
        );
    }
}
