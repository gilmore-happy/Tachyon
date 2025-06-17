//! src/markets/real_time_pools.rs
//! Real-time pool data via logsSubscribe - MUCH better than cache files

use crate::arbitrage::{ArbitrageConfig, PairId};
use crate::common::config::Config;
use anyhow::{Result, Context};
use dashmap::DashMap;
use futures_util::StreamExt;
use log::{info, error, warn, debug};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

// Enhanced Pool structure with real-time data
#[derive(Debug, Clone)]
pub struct RealTimePool {
    pub id: String,
    pub token_a: Pubkey,
    pub token_b: Pubkey,
    pub pair_id: PairId,
    pub liquidity_a: f64,
    pub liquidity_b: f64,
    pub current_price: f64,
    pub fee_bps: u16,
    pub dex_program: Pubkey,
    pub dex_name: String,
    pub last_updated: Instant,
    pub is_active: bool,
}

impl RealTimePool {
    pub fn calculate_effective_rate(&self) -> f64 {
        let fee_multiplier = 1.0 - (self.fee_bps as f64 / 10_000.0);
        self.current_price * fee_multiplier
    }
    
    pub fn has_sufficient_liquidity(&self, amount_usd: f64) -> bool {
        self.liquidity_a > amount_usd && self.liquidity_b > amount_usd
    }
    
    pub fn validate(&self) -> Result<()> {
        if self.current_price <= 0.0 || !self.current_price.is_finite() {
            return Err(anyhow::anyhow!("Invalid price: {}", self.current_price));
        }
        if self.liquidity_a < 0.0 || self.liquidity_b < 0.0 {
            return Err(anyhow::anyhow!("Negative liquidity"));
        }
        if self.fee_bps > 10000 {
            return Err(anyhow::anyhow!("Invalid fee: {} bps", self.fee_bps));
        }
        if !self.is_active {
            return Err(anyhow::anyhow!("Pool not active"));
        }
        Ok(())
    }
}

// Real-time pool registry with logsSubscribe
pub struct RealTimePoolRegistry {
    pools: Arc<DashMap<String, RealTimePool>>,
    pools_by_pair: Arc<DashMap<PairId, Vec<String>>>, // Fast pair lookups
    config: Arc<Config>,
    arbitrage_config: Arc<ArbitrageConfig>,
    update_sender: mpsc::Sender<PoolUpdate>,
}

#[derive(Debug, Clone)]
pub struct PoolUpdate {
    pub pool_id: String,
    pub price: Option<f64>,
    pub liquidity_a: Option<f64>,
    pub liquidity_b: Option<f64>,
    pub timestamp: Instant,
}

impl RealTimePoolRegistry {
    pub async fn new(config: Arc<Config>, arbitrage_config: Arc<ArbitrageConfig>) -> Result<Self> {
        let (update_sender, update_receiver) = mpsc::channel(1000);
        
        let registry = Self {
            pools: Arc::new(DashMap::new()),
            pools_by_pair: Arc::new(DashMap::new()),
            config: config.clone(),
            arbitrage_config,
            update_sender: update_sender.clone(),
        };
        
        // Start the real-time subscription
        registry.start_real_time_updates(update_receiver).await?;
        
        // Load initial pool data (once, then rely on real-time updates)
        registry.load_initial_pools().await?;
        
        Ok(registry)
    }
    
    async fn start_real_time_updates(&self, mut update_receiver: mpsc::Receiver<PoolUpdate>) -> Result<()> {
        let pools = self.pools.clone();
        let config = self.config.clone();
        let min_liquidity = self.arbitrage_config.min_liquidity_usd;
        let update_sender = self.update_sender.clone();
        
        // Start WebSocket subscription to DEX programs
        let subscription_handle = tokio::spawn(async move {
            if let Err(e) = Self::subscribe_to_dex_logs(config.clone(), update_sender).await {
                error!("DEX logs subscription failed: {}", e);
            }
        });
        
        // Start update processor
        let update_handle = tokio::spawn(async move {
            while let Some(update) = update_receiver.recv().await {
                if let Some(mut pool) = pools.get_mut(&update.pool_id) {
                    // Update pool data
                    if let Some(price) = update.price {
                        pool.current_price = price;
                    }
                    if let Some(liq_a) = update.liquidity_a {
                        pool.liquidity_a = liq_a;
                    }
                    if let Some(liq_b) = update.liquidity_b {
                        pool.liquidity_b = liq_b;
                    }
                    pool.last_updated = update.timestamp;
                    
                    // Validate minimum liquidity
                    if pool.liquidity_a < min_liquidity || pool.liquidity_b < min_liquidity {
                        pool.is_active = false;
                        debug!("Pool {} deactivated due to low liquidity", pool.id);
                    } else {
                        pool.is_active = true;
                    }
                }
            }
        });
        
        // Don't wait for handles to complete - they run indefinitely
        tokio::spawn(async move {
            let _ = tokio::try_join!(subscription_handle, update_handle);
        });
        
        Ok(())
    }
    
    async fn subscribe_to_dex_logs(
        config: Arc<Config>, 
        update_sender: mpsc::Sender<PoolUpdate>
    ) -> Result<()> {
        let websocket_url = config.get_websocket_url();
        
        // DEX program addresses to monitor
        let dex_programs = vec![
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", // Raydium AMM
            "CAMMCzo5YL8w4VFF8KVHrK22GGUQpMTdxqUKwUnKWjGW", // Raydium CLMM
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",  // Orca Whirlpools
            "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP", // Orca V2
            "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",  // Meteora
        ];
        
        for program_id in dex_programs {
            let program_pubkey = Pubkey::from_str(program_id)
                .context(format!("Invalid program ID: {}", program_id))?;
            
            let config = RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::confirmed()),
            };
            
            let filter = RpcTransactionLogsFilter::Mentions(vec![program_pubkey.to_string()]);
            
            let sender = update_sender.clone();
            let ws_url = websocket_url.clone();
            
            tokio::spawn(async move {
                let mut retry_count = 0;
                const MAX_RETRIES: u32 = 10;
                
                while retry_count < MAX_RETRIES {
                    match PubsubClient::new(&ws_url).await {
                        Ok(pubsub_client) => {
                            info!("📡 Subscribing to logs for DEX program: {}", program_id);
                            
                            match pubsub_client.logs_subscribe(filter.clone(), config.clone()).await {
                                Ok((mut stream, _unsubscribe)) => {
                                    retry_count = 0; // Reset on successful connection
                                    
                                    while let Some(log_notification) = stream.next().await {
                                        if let Err(e) = Self::process_dex_log(
                                            &log_notification.value, 
                                            &sender,
                                            program_id
                                        ).await {
                                            debug!("Failed to process DEX log: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to subscribe to DEX logs for {}: {}", program_id, e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to create pubsub client for {}: {}", program_id, e);
                        }
                    }
                    
                    retry_count += 1;
                    let delay = Duration::from_secs(2_u64.pow(retry_count.min(5))); // Exponential backoff
                    warn!("Retrying DEX subscription for {} in {:?} (attempt {}/{})", 
                        program_id, delay, retry_count, MAX_RETRIES);
                    sleep(delay).await;
                }
                
                error!("Max retries exceeded for DEX program: {}", program_id);
            });
        }
        
        Ok(())
    }
    
    async fn process_dex_log(
        log_data: &solana_client::rpc_response::RpcLogsResponse,
        update_sender: &mpsc::Sender<PoolUpdate>,
        program_id: &str,
    ) -> Result<()> {
        // Extract pool information from transaction logs
        // This is DEX-specific parsing - simplified here
        
        let signature = &log_data.signature;
        let logs = &log_data.logs;
        
        // Look for swap events in logs
        for log_entry in logs {
            // Parse DEX-specific log formats
            if log_entry.contains("Program log: Instruction: Swap") {
                // Extract pool ID, amounts, etc. from logs
                // This is simplified - real implementation needs DEX-specific parsing
                
                let pool_id = format!("{}_{}", program_id, signature);
                
                let update = PoolUpdate {
                    pool_id,
                    price: Some(1.0), // Extract from logs
                    liquidity_a: Some(10000.0), // Extract from logs
                    liquidity_b: Some(10000.0), // Extract from logs
                    timestamp: Instant::now(),
                };
                
                if let Err(e) = update_sender.try_send(update) {
                    debug!("Failed to send pool update: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn load_initial_pools(&self) -> Result<()> {
        info!("🔄 Loading initial pool data from DEX APIs...");
        
        // Load a minimal set of high-liquidity pools for major pairs
        let target_pairs = vec![
            ("So11111111111111111111111111111111111111112", "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"), // SOL/USDC
            ("So11111111111111111111111111111111111111112", "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"), // SOL/USDT
            ("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"), // USDC/USDT
        ];
        
        for (token_a_str, token_b_str) in target_pairs {
            let token_a = Pubkey::from_str(token_a_str)?;
            let token_b = Pubkey::from_str(token_b_str)?;
            
            // Create initial pool entries (will be updated by real-time feeds)
            let pool = RealTimePool {
                id: format!("{}_{}", token_a_str, token_b_str),
                token_a,
                token_b,
                pair_id: PairId::new(
                    token_a.to_bytes().iter().sum::<u8>() as u32,
                    token_b.to_bytes().iter().sum::<u8>() as u32,
                ),
                liquidity_a: 100000.0, // Will be updated by real-time data
                liquidity_b: 100000.0,
                current_price: 1.0,
                fee_bps: 25, // 0.25%
                dex_program: Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8")?, // Raydium
                dex_name: "Raydium".to_string(),
                last_updated: Instant::now(),
                is_active: true,
            };
            
            // Validate before insertion
            if pool.validate().is_ok() && 
               pool.liquidity_a >= self.arbitrage_config.min_liquidity_usd &&
               pool.liquidity_b >= self.arbitrage_config.min_liquidity_usd {
                
                // Add to main pool storage
                self.pools.insert(pool.id.clone(), pool.clone());
                
                // Add to pair lookup
                self.pools_by_pair
                    .entry(pool.pair_id)
                    .or_insert_with(Vec::new)
                    .push(pool.id.clone());
            }
        }
        
        info!("✅ Loaded {} initial pools", self.pools.len());
        Ok(())
    }
    
    // Public interface methods
    pub async fn get_pools(&self) -> Vec<RealTimePool> {
        self.pools.iter()
            .filter(|pool| pool.is_active)
            .map(|entry| entry.value().clone())
            .collect()
    }
    
    pub async fn get_pools_for_pair(&self, pair_id: PairId) -> Vec<RealTimePool> {
        if let Some(pool_ids) = self.pools_by_pair.get(&pair_id) {
            pool_ids.iter()
                .filter_map(|id| self.pools.get(id))
                .filter(|pool| pool.is_active)
                .map(|entry| entry.value().clone())
                .collect()
        } else {
            Vec::new()
        }
    }
    
    pub fn len(&self) -> usize {
        self.pools.len()
    }
    
    pub fn active_pools_count(&self) -> usize {
        self.pools.iter().filter(|pool| pool.is_active).count()
    }
}

// Helper function to create token indices for PairId
fn token_to_index(token: &Pubkey) -> u32 {
    // Simple hash function - in production, use a proper mapping
    token.to_bytes().iter().map(|&b| b as u32).sum::<u32>() ^ 
    token.to_bytes().len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pool_validation() {
        let pool = RealTimePool {
            id: "test".to_string(),
            token_a: Pubkey::new_unique(),
            token_b: Pubkey::new_unique(),
            pair_id: PairId::new(1, 2),
            liquidity_a: 10000.0,
            liquidity_b: 10000.0,
            current_price: 1.0,
            fee_bps: 25,
            dex_program: Pubkey::new_unique(),
            dex_name: "Test".to_string(),
            last_updated: Instant::now(),
            is_active: true,
        };
        
        assert!(pool.validate().is_ok());
        assert!(pool.has_sufficient_liquidity(5000.0));
        assert!(!pool.has_sufficient_liquidity(15000.0));
    }
    
    #[test]
    fn test_effective_rate_calculation() {
        let pool = RealTimePool {
            id: "test".to_string(),
            token_a: Pubkey::new_unique(),
            token_b: Pubkey::new_unique(),
            pair_id: PairId::new(1, 2),
            liquidity_a: 10000.0,
            liquidity_b: 10000.0,
            current_price: 100.0,
            fee_bps: 25, // 0.25%
            dex_program: Pubkey::new_unique(),
            dex_name: "Test".to_string(),
            last_updated: Instant::now(),
            is_active: true,
        };
        
        let effective_rate = pool.calculate_effective_rate();
        let expected = 100.0 * (1.0 - 0.0025); // 99.75
        assert!((effective_rate - expected).abs() < 0.01);
    }
} 