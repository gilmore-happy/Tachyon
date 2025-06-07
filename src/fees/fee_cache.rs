use anyhow::Result;
use log::{error, info, warn};
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct CachedFeeData {
    pub base_fee: u64,
    pub percentile_75: u64,
    pub percentile_90: u64,
    pub percentile_95: u64,
    pub max_recent_fee: u64,
    pub timestamp: Instant,
}

pub struct FeeCache {
    rpc_client: Arc<RpcClient>,
    cache: Arc<RwLock<Option<CachedFeeData>>>,
    cache_duration: Duration,
}

impl FeeCache {
    pub fn new(rpc_client: Arc<RpcClient>, cache_duration_secs: u64) -> Self {
        Self {
            rpc_client,
            cache: Arc::new(RwLock::new(None)),
            cache_duration: Duration::from_secs(cache_duration_secs),
        }
    }

    /// Get cached fee data or fetch new data if cache is stale
    pub async fn get_fee_data(&self) -> Result<CachedFeeData> {
        let cache_read = self.cache.read().await;

        // Check if cache is valid
        if let Some(cached_data) = &*cache_read {
            if cached_data.timestamp.elapsed() < self.cache_duration {
                return Ok(cached_data.clone());
            }
        }
        drop(cache_read);

        // Cache is stale or empty, fetch new data
        self.refresh_cache().await
    }

    /// Force refresh the cache
    pub async fn refresh_cache(&self) -> Result<CachedFeeData> {
        match self.fetch_recent_fees().await {
            Ok(fee_data) => {
                let mut cache_write = self.cache.write().await;
                *cache_write = Some(fee_data.clone());
                info!(
                    "📊 Fee cache refreshed: base={}, p75={}, p90={}, p95={}, max={}",
                    fee_data.base_fee,
                    fee_data.percentile_75,
                    fee_data.percentile_90,
                    fee_data.percentile_95,
                    fee_data.max_recent_fee
                );
                Ok(fee_data)
            }
            Err(e) => {
                error!("Failed to fetch recent fees: {:?}", e);

                // Return cached data if available, even if stale
                let cache_read = self.cache.read().await;
                if let Some(cached_data) = &*cache_read {
                    warn!("Using stale fee cache due to RPC error");
                    Ok(cached_data.clone())
                } else {
                    // No cache available, return default
                    Ok(CachedFeeData {
                        base_fee: 10_000,
                        percentile_75: 10_000,
                        percentile_90: 25_000,
                        percentile_95: 50_000,
                        max_recent_fee: 100_000,
                        timestamp: Instant::now(),
                    })
                }
            }
        }
    }

    /// Fetch recent prioritization fees from RPC
    async fn fetch_recent_fees(&self) -> Result<CachedFeeData> {
        let recent_fees = self.rpc_client.get_recent_prioritization_fees(&[]).await?;

        if recent_fees.is_empty() {
            warn!("No recent prioritization fees available");
            return Ok(CachedFeeData {
                base_fee: 10_000,
                percentile_75: 10_000,
                percentile_90: 25_000,
                percentile_95: 50_000,
                max_recent_fee: 100_000,
                timestamp: Instant::now(),
            });
        }

        // Extract fee values and sort
        let mut fee_values: Vec<u64> = recent_fees
            .iter()
            .map(|f| f.prioritization_fee)
            .filter(|&f| f > 0) // Filter out zero fees
            .collect();

        if fee_values.is_empty() {
            fee_values.push(10_000); // Default if all fees are zero
        }

        fee_values.sort_unstable();

        // Calculate percentiles
        let len = fee_values.len();
        let base_fee = fee_values[len / 2]; // Median
        let percentile_75 = fee_values[(len as f64 * 0.75) as usize];
        let percentile_90 = fee_values[(len as f64 * 0.90) as usize];
        let percentile_95 = fee_values[(len as f64 * 0.95) as usize];
        let max_recent_fee = *fee_values.last().unwrap();

        Ok(CachedFeeData {
            base_fee,
            percentile_75,
            percentile_90,
            percentile_95,
            max_recent_fee,
            timestamp: Instant::now(),
        })
    }

    /// Start background refresh task
    pub fn start_background_refresh(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.cache_duration);
            loop {
                interval.tick().await;
                if let Err(e) = self.refresh_cache().await {
                    error!("Background fee cache refresh failed: {:?}", e);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fee_cache_basic() {
        // Test implementation would go here
    }
}
