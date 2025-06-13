//! src/markets/pools.rs

use crate::common::config::Config;
use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

#[derive(Debug, Clone)]
pub struct Pool {
    pub id: String,
    pub token_a: String,
    pub token_b: String,
    pub liquidity: f64,
}

pub struct PoolRegistry {
    pools: Arc<DashMap<String, Pool>>,
    last_updated: Arc<tokio::sync::RwLock<Instant>>,
    config: Arc<Config>,
}

impl PoolRegistry {
    pub async fn new(config: &Config) -> Result<Self> {
        let pools = load_all_pools(config).await?;
        let pool_map = DashMap::new();
        for pool in pools {
            pool_map.insert(pool.id.clone(), pool);
        }
        Ok(Self {
            pools: Arc::new(pool_map),
            last_updated: Arc::new(tokio::sync::RwLock::new(Instant::now())),
            config: Arc::new(config.clone()),
        })
    }

    pub async fn get_pools(&self, force_update: bool) -> Result<Vec<Pool>> {
        const POOL_TTL: Duration = Duration::from_secs(300);
        if force_update || self.last_updated.read().await.elapsed() > POOL_TTL {
            self.refresh().await?;
        }
        Ok(self.pools.iter().map(|e| e.value().clone()).collect())
    }

    async fn refresh(&self) -> Result<()> {
        let new_pools = load_all_pools(&self.config).await?;
        self.pools.clear();
        for pool in new_pools {
            self.pools.insert(pool.id.clone(), pool);
        }
        let mut last_updated = self.last_updated.write().await;
        *last_updated = Instant::now();
        info!("Refreshed {} pools", self.pools.len());
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.pools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub async fn load_all_pools(_config: &Config) -> Result<Vec<Pool>> {
    // Placeholder: Implement logic to fetch pools from all configured DEXs
    info!("Loading all pools (placeholder)...");
    Ok(vec![Pool {
        id: "pool1".to_string(),
        token_a: "SOL".to_string(),
        token_b: "USDC".to_string(),
        liquidity: 1_000_000.0,
    }])
}
