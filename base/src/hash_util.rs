use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hash, Hasher};
use xxhash_rust::xxh3::{Xxh3Default, Xxh3DefaultBuilder};

/// The default choice for hashing in Helgobox.
pub type NonCryptoHashBuilder = Xxh3DefaultBuilder;

/// The default choice for hashing in Helgobox.
pub type NonCryptoHasher = Xxh3Default;

/// The default choice for hash maps in Helgobox.
pub type NonCryptoHashMap<K, V> = HashMap<K, V, NonCryptoHashBuilder>;

/// The default choice for hash sets in Helgobox.
pub type NonCryptoHashSet<T> = HashSet<T, NonCryptoHashBuilder>;

/// The default choice for index maps in Helgobox.
pub type NonCryptoIndexMap<K, V> = IndexMap<K, V, NonCryptoHashBuilder>;

/// The default choice for index sets in Helgobox.
pub type NonCryptoIndexSet<T> = IndexSet<T, NonCryptoHashBuilder>;

pub fn clone_to_other_hash_map<
    K: Eq + Hash + Clone,
    V: Clone,
    S1: BuildHasher,
    S2: BuildHasher + Default,
>(
    non_crypto: &HashMap<K, V, S1>,
) -> HashMap<K, V, S2> {
    non_crypto
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn convert_into_other_hash_map<K: Eq + Hash, V, S1: BuildHasher, S2: BuildHasher + Default>(
    non_crypto: HashMap<K, V, S1>,
) -> HashMap<K, V, S2> {
    non_crypto.into_iter().collect()
}

pub fn convert_into_other_hash_set<K: Eq + Hash, S1: BuildHasher, S2: BuildHasher + Default>(
    non_crypto: HashSet<K, S1>,
) -> HashSet<K, S2> {
    non_crypto.into_iter().collect()
}

/// Calculates a 64-bit non-crypto hash directly from the given bytes.
///
/// A bit faster than the streaming version.
pub fn calculate_non_crypto_hash_one_shot(payload: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(payload)
}

/// Calculates a 128-bit non-crypto hash directly from the given bytes suitable for persistence.
///
/// This implementation must not change!
pub fn calculate_persistent_non_crypto_hash_one_shot(payload: &[u8]) -> PersistentHash {
    // Don't change the hash function! It's used e.g. for file names.
    PersistentHash(xxhash_rust::xxh3::xxh3_128(payload))
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
    NonCryptoHasher::new()
}

/// Creates a builder for a hasher for calculating a 64-bit non-crypto hash.
pub fn create_non_crypto_hash_builder() -> NonCryptoHashBuilder {
    NonCryptoHashBuilder::new()
}

/// This newtype should be used whenever it matters to keep a stable hash function, for example
/// when the hashes are going to be persisted.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct PersistentHash(u128);

impl PersistentHash {
    pub fn get(&self) -> u128 {
        self.0
    }
}

#[derive(Default)]
pub struct PersistentHasher(Xxh3Default);

impl PersistentHasher {
    pub fn new() -> Self {
        // Don't change the wrapped hasher! It's used e.g. for file names.
        Self::default()
    }

    pub fn digest_128(&self) -> PersistentHash {
        PersistentHash(self.0.digest128())
    }
}

impl Hasher for PersistentHasher {
    fn finish(&self) -> u64 {
        self.0.finish()
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }
}
