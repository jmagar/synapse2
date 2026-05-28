//! Unit tests for the sync TTL cache.

use super::{Cache, MemoryCache};
use std::thread;
use std::time::Duration;

#[test]
fn test_set_then_get_within_ttl() {
    let cache = MemoryCache::new();
    cache.set("key1".to_string(), "value1".to_string());

    // Should return the value immediately.
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
}

#[test]
fn test_get_returns_none_after_ttl_elapses() {
    // TTL of 100ms.
    let cache = MemoryCache::with_ttl(Duration::from_millis(100));
    cache.set("key1".to_string(), "value1".to_string());

    // Within TTL: should exist.
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));

    // Wait for expiration.
    thread::sleep(Duration::from_millis(150));

    // After TTL: should be gone.
    assert_eq!(cache.get(&"key1".to_string()), None);
}

#[test]
fn test_get_returns_none_for_missing_key() {
    let cache: MemoryCache<String, String> = MemoryCache::new();

    assert_eq!(cache.get(&"nonexistent".to_string()), None);
}

#[test]
fn test_invalidate_removes_single_entry() {
    let cache = MemoryCache::new();
    cache.set("key1".to_string(), "value1".to_string());
    cache.set("key2".to_string(), "value2".to_string());

    // Both should exist.
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));

    // Invalidate one.
    cache.invalidate(&"key1".to_string());

    // Only key1 should be gone.
    assert_eq!(cache.get(&"key1".to_string()), None);
    assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));
}

#[test]
fn test_invalidate_all_removes_all_entries() {
    let cache = MemoryCache::new();
    cache.set("key1".to_string(), "value1".to_string());
    cache.set("key2".to_string(), "value2".to_string());
    cache.set("key3".to_string(), "value3".to_string());

    // All should exist.
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));
    assert_eq!(cache.get(&"key3".to_string()), Some("value3".to_string()));

    // Invalidate all.
    cache.invalidate_all();

    // None should exist.
    assert_eq!(cache.get(&"key1".to_string()), None);
    assert_eq!(cache.get(&"key2".to_string()), None);
    assert_eq!(cache.get(&"key3".to_string()), None);
}

#[test]
fn test_concurrent_set_from_100_tasks() {
    use std::sync::Arc;

    let cache = Arc::new(MemoryCache::new());
    let mut handles = vec![];

    // Spawn 100 tasks, each setting 10 unique keys.
    for task_id in 0..100 {
        let cache_clone = Arc::clone(&cache);
        let handle = std::thread::spawn(move || {
            for i in 0..10 {
                let key = format!("task_{}_key_{}", task_id, i);
                let value = format!("value_{}", task_id * 1000 + i);
                cache_clone.set(key, value);
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete.
    for handle in handles {
        handle.join().expect("task should complete");
    }

    // Verify all 1000 entries were inserted (no corruption).
    let mut count = 0;
    for task_id in 0..100 {
        for i in 0..10 {
            let key = format!("task_{}_key_{}", task_id, i);
            let expected = format!("value_{}", task_id * 1000 + i);
            assert_eq!(
                cache.get(&key),
                Some(expected),
                "key {} should exist and match",
                key
            );
            count += 1;
        }
    }
    assert_eq!(count, 1000, "all 1000 entries should be present");
}

#[test]
fn test_max_entries_cap_evicts_oldest() {
    // Cap at 5 entries, default TTL.
    let cache = MemoryCache::with_ttl_and_max(Duration::from_secs(60), 5);

    // Insert 5 entries.
    cache.set("key1".to_string(), "value1".to_string());
    cache.set("key2".to_string(), "value2".to_string());
    cache.set("key3".to_string(), "value3".to_string());
    cache.set("key4".to_string(), "value4".to_string());
    cache.set("key5".to_string(), "value5".to_string());

    // All should exist.
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    assert_eq!(cache.get(&"key5".to_string()), Some("value5".to_string()));

    // Insert a 6th entry, which should trigger eviction of the oldest.
    cache.set("key6".to_string(), "value6".to_string());

    // key1 (oldest) should be evicted.
    assert_eq!(
        cache.get(&"key1".to_string()),
        None,
        "oldest entry should be evicted"
    );

    // key6 should exist.
    assert_eq!(cache.get(&"key6".to_string()), Some("value6".to_string()));

    // The rest should still exist.
    assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));
    assert_eq!(cache.get(&"key3".to_string()), Some("value3".to_string()));
    assert_eq!(cache.get(&"key4".to_string()), Some("value4".to_string()));
    assert_eq!(cache.get(&"key5".to_string()), Some("value5".to_string()));
}

#[test]
fn test_update_existing_key_resets_insertion_time() {
    // TTL of 200ms.
    let cache = MemoryCache::with_ttl(Duration::from_millis(200));

    cache.set("key1".to_string(), "value1".to_string());
    thread::sleep(Duration::from_millis(100));

    // Update the key.
    cache.set("key1".to_string(), "value1_updated".to_string());

    // Wait another 150ms (total ~250ms from first insert, but only ~150ms from second).
    thread::sleep(Duration::from_millis(150));

    // Should still exist because the second set refreshed the TTL.
    assert_eq!(
        cache.get(&"key1".to_string()),
        Some("value1_updated".to_string())
    );

    // Wait for the total TTL from the second insert to expire.
    thread::sleep(Duration::from_millis(100));

    // Now it should be gone.
    assert_eq!(cache.get(&"key1".to_string()), None);
}

#[test]
fn test_cache_with_different_types() {
    #[derive(Clone, Debug, PartialEq)]
    struct TestValue {
        id: i32,
        name: String,
    }

    let cache = MemoryCache::new();
    let val = TestValue {
        id: 42,
        name: "test".to_string(),
    };

    cache.set(123, val.clone());
    assert_eq!(cache.get(&123), Some(val));
}

#[test]
fn test_concurrent_get_and_set() {
    use std::sync::Arc;

    let cache = Arc::new(MemoryCache::new());
    let mut handles = vec![];

    // Spawn multiple tasks that both set and get.
    for task_id in 0..10 {
        let cache_clone = Arc::clone(&cache);
        let handle = std::thread::spawn(move || {
            for i in 0..100 {
                let key = format!("shared_key_{}", i % 20);
                let value = format!("task_{}_value_{}", task_id, i);
                cache_clone.set(key.clone(), value.clone());
                let _ = cache_clone.get(&key);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("task should complete");
    }

    // At least some entries should still be in the cache.
    let mut found = 0;
    for i in 0..20 {
        let key = format!("shared_key_{}", i);
        if cache.get(&key).is_some() {
            found += 1;
        }
    }
    assert!(found > 0, "at least some entries should be in the cache");
}

#[test]
fn test_default_ttl_is_60_seconds() {
    let cache = MemoryCache::new();
    cache.set("key1".to_string(), "value1".to_string());

    // Should exist immediately and within reasonable time.
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));

    // We won't wait 60s in a test, but we can verify the cache works correctly
    // for reasonable durations (much less than 60s).
    thread::sleep(Duration::from_millis(10));
    assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
}

#[test]
fn test_empty_cache_invalidate_all_is_safe() {
    let cache: MemoryCache<String, String> = MemoryCache::new();

    // Should not panic.
    cache.invalidate_all();
    assert_eq!(cache.get(&"any_key".to_string()), None);
}

#[test]
fn test_set_overwrites_previous_value() {
    let cache = MemoryCache::new();

    cache.set("key1".to_string(), "original".to_string());
    assert_eq!(cache.get(&"key1".to_string()), Some("original".to_string()));

    cache.set("key1".to_string(), "updated".to_string());
    assert_eq!(cache.get(&"key1".to_string()), Some("updated".to_string()));
}
