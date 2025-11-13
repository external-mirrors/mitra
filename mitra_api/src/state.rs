use std::collections::HashMap;
use std::time::Instant;

use mitra_services::media::MediaStorage;
use tokio::sync::Mutex;

pub struct TimedCache {
    store: HashMap<String, (Instant, String)>,
    ttl: u64,
    size: usize,
}

impl TimedCache {
    pub fn new(ttl: u64, size: usize) -> Self {
        Self { store: HashMap::new(), ttl, size }
    }

    /// Removes expired entries
    fn evict(&mut self) -> () {
        // Remove expired
        self.store.retain(|_key, (t, _val)| {
            t.elapsed().as_secs() < self.ttl
        });
        if self.store.len() > self.size {
            // Remove extra items
            let mut sorted: Vec<_> = self.store.iter()
                .map(|(key, (t, _val))| (key, t))
                .collect();
            sorted.sort_by_key(|(_key, t)| **t);
            let latest: Vec<_> = sorted.into_iter()
                .rev()
                .take(self.size)
                .map(|(key, _t)| key.clone())
                .collect();
            self.store.retain(|key, (_t, _val)| latest.contains(key));
        };
    }

    pub fn set(&mut self, key: String, val: String) -> () {
        self.evict();
        self.store.insert(key, (Instant::now(), val));
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        self.evict();
        match self.store.get(key) {
            Some((_t, val)) => Some(val),
            None => None,
        }
    }
}

const POST_CACHE_EXPIRY_TIME: u64 = 60 * 60; // 1 hour
const POST_CACHE_SIZE: usize = 100;

// https://actix.rs/docs/application/#shared-mutable-state
pub struct AppState {
    pub post_id_cache: Mutex<TimedCache>,
    pub media_storage: MediaStorage,
}

impl AppState {
    pub fn init(media_storage: MediaStorage) -> Self {
        Self {
            post_id_cache: Mutex::new(TimedCache::new(POST_CACHE_EXPIRY_TIME, POST_CACHE_SIZE)),
            media_storage: media_storage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timed_cache() {
        let mut cache = TimedCache::new(60, 10);
        let key = "key";
        let value_1 = "value1";
        let value_2 = "value2";
        assert_eq!(cache.get(key), None);
        cache.set(key.to_string(), value_1.to_string());
        assert_eq!(cache.get(key), Some(value_1));
        cache.set(key.to_string(), value_2.to_string());
        assert_eq!(cache.get(key), Some(value_2));
    }

    #[test]
    fn test_timed_cache_expiration() {
        let mut cache = TimedCache::new(0, 10);
        let key = "key";
        let value = "value";
        cache.set(key.to_string(), value.to_string());
        // Immediately expired
        assert_eq!(cache.get(key), None);
    }

    #[test]
    fn test_timed_cache_size() {
        let mut cache = TimedCache::new(60, 1);
        let key_1 = "key1";
        let key_2 = "key2";
        let value_1 = "value1";
        let value_2 = "value2";
        cache.set(key_1.to_string(), value_1.to_string());
        assert_eq!(cache.get(key_1), Some(value_1));
        cache.set(key_2.to_string(), value_2.to_string());
        assert_eq!(cache.get(key_1), None); // removed
        assert_eq!(cache.get(key_2), Some(value_2));
    }
}
