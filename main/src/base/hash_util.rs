use std::hash::{Hash, Hasher};

// TODO-high We should replace this with two-x-hash!
pub fn calculate_non_crypto_hash<T: Hash>(t: &T) -> u64 {
    let mut s = seahash::SeaHasher::new();
    t.hash(&mut s);
    s.finish()
}
