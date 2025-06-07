//! src/markets/lockless_cache.rs
//
// This module provides a high-performance, thread-safe cache for market data.
// It is designed for high-throughput, low-latency concurrent access, which is
// essential for the arbitrage bot's simulation engine.
//
// The core of this module is `DashMap`, a concurrent hash map that avoids the
// global lock contention issues of a standard `Arc<RwLock<HashMap<...>>>`.
// It achieves this by splitting the map into many small, independently-locked
// shards, allowing multiple threads to access different keys simultaneously.

use crate::markets::types::Market;
use dashmap::DashMap;
use log::info;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A snapshot of the cache's performance statistics.
#[derive(Debug)]
pub struct CacheStatistics {
    pub total_markets: usize,
    pub total_reads: u64,
    pub total_writes: u64,
    pub hit_ratio: f64,
}

/// Internal statistics tracking for the cache, using atomics for lock-free counting.
#[derive(Default)]
struct AtomicCacheStats {
    hits: AtomicU64,
    misses: AtomicU64,
    writes: AtomicU64,
}

/// A high-performance, thread-safe cache for real-time market data.
/// It allows for highly concurrent reads and writes without a global lock.
#[derive(Clone)]
pub struct LocklessMarketCache {
    /// The core concurrent hash map storing market data, indexed by market ID.
    markets: Arc<DashMap<String, Market>>,
    /// Lock-free counters for monitoring cache performance.
    stats: Arc<AtomicCacheStats>,
}

impl LocklessMarketCache {
    /// Creates a new, empty lockless market cache.
    pub fn new() -> Self {
        Self {
            markets: Arc::new(DashMap::new()),
            stats: Arc::new(AtomicCacheStats::default()),
        }
    }

    /// Inserts or updates a market in the cache.
    /// This is a fast, thread-safe operation.
    pub fn insert(&self, market: Market) {
        self.stats.writes.fetch_add(1, Ordering::Relaxed);
        self.markets.insert(market.id.clone(), market);
    }

    /// Retrieves a single market from the cache by its ID.
    /// This is the primary, high-performance read method, intended for use in hot paths.
    /// It returns a clone of the market data.
    pub fn get(&self, market_id: &str) -> Option<Market> {
        match self.markets.get(market_id) {
            Some(market_ref) => {
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                Some(market_ref.value().clone())
            }
            None => {
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// **PERFORMANCE WARNING:**
    /// Creates a standard `HashMap` containing a clone of all markets in the cache.
    /// This is a potentially slow, `O(n)` operation that iterates over the entire cache.
    /// It should NOT be used in latency-sensitive hot paths. Use this for tasks like
    /// initial setup, periodic analysis, or debugging.
    pub fn get_all_as_hashmap(&self) -> HashMap<String, Market> {
        self.markets
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Returns a snapshot of the cache's current performance statistics.
    pub fn get_stats(&self) -> CacheStatistics {
        let hits = self.stats.hits.load(Ordering::Relaxed);
        let misses = self.stats.misses.load(Ordering::Relaxed);
        let total_accesses = hits + misses;

        let hit_ratio = if total_accesses == 0 {
            1.0 // No accesses yet, so 100% successful.
        } else {
            hits as f64 / total_accesses as f64
        };

        CacheStatistics {
            total_markets: self.markets.len(),
            // Total reads = hits + misses
            total_reads: total_accesses,
            total_writes: self.stats.writes.load(Ordering::Relaxed),
            hit_ratio,
        }
    }

    /// Logs the current cache statistics.
    pub fn log_stats(&self) {
        let stats = self.get_stats();
        info!(
            "Lockless Cache Stats: Markets: {}, Reads: {}, Writes: {}, Hit Ratio: {:.2}%",
            stats.total_markets,
            stats.total_reads,
            stats.total_writes,
            stats.hit_ratio * 100.0
        );
    }
}

// Implement the Default trait for convenience.
impl Default for LocklessMarketCache {
    fn default() -> Self {
        Self::new()
    }
}
