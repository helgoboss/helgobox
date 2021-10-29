use std::hash::{Hash, Hasher};

pub fn calculate_non_crypto_hash<T: Hash>(t: &T) -> u64 {
    let mut hasher = twox_hash::XxHash64::default();
    t.hash(&mut hasher);
    hasher.finish()
}
