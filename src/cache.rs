//! Generic sync TTL cache trait and in-memory implementation.
//!
//! **Design principles:**
//! - Sync (non-async) trait: appropriate for in-memory operations.
//! - Per-entry TTL with default 60s, configurable per cache instance.
//! - Max entries cap (default 10k) with LRU eviction to bound memory.
//! - Lazy expiration on `get`, no background sweeper threads.
//! - Thread-safe via `DashMap` for concurrent access without global locks.

use dashmap::DashMap;
use std::time::{Duration, SystemTime};

/// A generic synchronous cache trait.
///
/// Provides basic key-value storage with TTL support and explicit invalidation.
/// Implementations should ensure thread safety and bounded memory usage.
pub trait Cache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Retrieve a value by key.
    ///
    /// Returns `Some(v)` if the key exists and has not expired; `None` otherwise.
    fn get(&self, key: &K) -> Option<V>;

    /// Store or update a value by key.
    fn set(&self, key: K, value: V);

    /// Invalidate a single key.
    fn invalidate(&self, key: &K);

    /// Invalidate all entries.
    fn invalidate_all(&self);
}

/// Entry metadata: value + expiration time.
#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    inserted_at: SystemTime,
}

/// In-memory TTL cache with LRU eviction.
///
/// - **TTL:** Per-entry, configurable per cache instance (default 60s).
/// - **Max entries:** Capped (default 10k); when exceeded, least-recently-used entries are evicted.
/// - **Thread safety:** Backed by `DashMap` for lock-free concurrent access.
/// - **Expiration:** Lazily checked on `get`; expired entries are removed.
pub struct MemoryCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    store: DashMap<K, CacheEntry<V>>,
    ttl: Duration,
    max_entries: usize,
}

impl<K, V> MemoryCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new cache with default TTL (60s) and max entries (10k).
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
            ttl: Duration::from_secs(60),
            max_entries: 10_000,
        }
    }

    /// Create a new cache with custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            store: DashMap::new(),
            ttl,
            max_entries: 10_000,
        }
    }

    /// Create a new cache with custom TTL and max entries.
    pub fn with_ttl_and_max(ttl: Duration, max_entries: usize) -> Self {
        Self {
            store: DashMap::new(),
            ttl,
            max_entries,
        }
    }

    /// Check if an entry has expired.
    fn is_expired(&self, entry: &CacheEntry<V>) -> bool {
        entry
            .inserted_at
            .elapsed()
            .map(|elapsed| elapsed > self.ttl)
            .unwrap_or(true)
    }

    /// Evict least-recently-used entry when capacity is reached.
    ///
    /// Simple heuristic: remove the entry with the oldest `inserted_at`.
    fn evict_oldest(&self) {
        if self.store.len() >= self.max_entries
            && let Some((oldest_key, _)) = self
                .store
                .iter()
                .min_by_key(|item| item.value().inserted_at)
                .map(|item| (item.key().clone(), item.value().clone()))
        {
            let _ = self.store.remove(&oldest_key);
        }
    }
}

impl<K, V> Default for MemoryCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Cache<K, V> for MemoryCache<K, V>
where
    K: Clone + Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn get(&self, key: &K) -> Option<V> {
        if let Some(entry_ref) = self.store.get(key) {
            let entry = entry_ref.clone();
            if !self.is_expired(&entry) {
                return Some(entry.value);
            }
        }

        // Remove expired entry.
        let _ = self.store.remove(key);
        None
    }

    fn set(&self, key: K, value: V) {
        // Evict if at capacity before inserting.
        self.evict_oldest();

        let entry = CacheEntry {
            value,
            inserted_at: SystemTime::now(),
        };
        self.store.insert(key, entry);
    }

    fn invalidate(&self, key: &K) {
        let _ = self.store.remove(key);
    }

    fn invalidate_all(&self) {
        self.store.clear();
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
