use std::hash::{Hash, Hasher};

pub fn calculate_non_crypto_hash<T: Hash>(t: &T) -> u64 {
    let mut s = seahash::SeaHasher::new();
    t.hash(&mut s);
    s.finish()
}
