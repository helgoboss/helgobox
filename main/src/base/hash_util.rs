use std::hash::{Hash, Hasher};

/// Calculates a 64-bit non-crypto hash directly from the given bytes.
///
/// A bit faster than the streaming version.
pub fn calculate_non_crypto_hash_one_shot(payload: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(payload)
}

/// Calculates a 64-bit non-crypto hash from the given hashable type.
///
/// If you already have a slice of bytes, use the one-shot version instead.
pub fn calculate_non_crypto_hash<T: Hash>(t: &T) -> u64 {
    let mut hasher = create_non_crypto_hasher();
    t.hash(&mut hasher);
    hasher.finish()
}

/// Creates a hasher for calculating a 64-bit non-crypto hash.
pub fn create_non_crypto_hasher() -> impl Hasher {
    xxhash_rust::xxh3::Xxh3::new()
}
