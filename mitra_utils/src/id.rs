use std::array;

use apx_core::crypto::hashes::sha256;
use chrono::{DateTime, Utc};
use uuid::{Builder, Uuid};

/// Produces new lexicographically sortable ID
pub fn generate_ulid() -> Uuid {
    Uuid::now_v7()
}

pub fn generate_deterministic_ulid(
    seed: &str,
    datetime: DateTime<Utc>,
) -> Uuid {
    let seed_hash = sha256(seed.as_bytes());
    let random_bytes: [u8; 10] = array::from_fn(|idx| {
        seed_hash.get(idx).copied().unwrap_or_default()
    });
    let timestamp_millis = datetime.timestamp_millis()
        .try_into()
        .expect("number of milliseconds should be positive");
    let builder = Builder::from_unix_timestamp_millis(
        timestamp_millis,
        &random_bytes,
    );
    builder.into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_deterministic_ulid() {
        let seed = "abc1";
        let datetime = Utc::now();
        let id_1 = generate_deterministic_ulid(seed, datetime);
        let id_2 = generate_deterministic_ulid(seed, datetime);
        let another_seed = "abc2";
        let id_3 = generate_deterministic_ulid(another_seed, datetime);
        assert_eq!(id_1, id_2);
        assert_ne!(id_1, id_3);
    }
}
