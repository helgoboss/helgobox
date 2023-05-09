use crate::base::hash_util;
use walkdir::DirEntry;

pub fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Converts something like "test" to something like
/// "a9/4a/8fe5ccb19ba61c4c0873d391e987.RfxChain" for the purpose to not get too many
/// files in one directory.
pub fn hash_to_dir_structure(input: impl AsRef<[u8]>, suffix: &str) -> String {
    let hash = hash_util::calculate_persistent_non_crypto_hash_one_shot(input.as_ref());
    let first_byte = hash.rotate_left(8) & 0xff;
    let second_byte = hash.rotate_left(16) & 0xff;
    // Remaining: 112 bits = 14 bytes = 28 hex chars
    let remaining = hash & 0xffffffffffffffffffffffffffff;
    format!("{first_byte:02x}/{second_byte:02x}/{remaining:028x}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_to_dir_structure_simple() {
        assert_eq!(
            hash_to_dir_structure("test", ".RfxChain"),
            "6c/78/e0e3bd51d358d01e758642b85fb8.RfxChain".to_string()
        );
    }
}
