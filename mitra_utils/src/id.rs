use std::time::SystemTime;

use apx_core::crypto::hashes::sha256;
use chrono::{DateTime, Utc};
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use ulid::Ulid;
use uuid::Uuid;

/// Produces new lexicographically sortable ID
pub fn generate_ulid() -> Uuid {
    let ulid = Ulid::new();
    Uuid::from(ulid)
}

pub fn generate_deterministic_ulid(
    seed: &str,
    datetime: DateTime<Utc>,
) -> Uuid {
    let seed_hash = sha256(seed.as_bytes());
    // Using specific RNG for reproducibility
    let mut rng = ChaCha12Rng::from_seed(seed_hash);
    let system_time = SystemTime::from(datetime);
    let ulid = Ulid::from_datetime_with_source(system_time, &mut rng);
    Uuid::from(ulid)
}
