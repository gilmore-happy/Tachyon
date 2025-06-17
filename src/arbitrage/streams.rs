//! src/arbitrage/streams.rs
//! Real-time arbitrage opportunity streams with enhanced error handling

use arc_swap::ArcSwap;
use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};
use tracing::{warn};

use crate::arbitrage::types::{ArbOpportunity, ArbitrageEngineOptions, TokenInArb};
use crate::arbitrage::config::ArbitrageConfig;
use crate::common::config::Config;
use crate::markets::pools::Pool;

/// Enhanced arbitrage opportunity stream with circuit breaker and rate limiting
pub struct ArbitrageOpportunityStream {
    rpc_client: Arc<RpcClient>,
    config: Arc<Config>,
    arbitrage_config: Arc<ArbitrageConfig>,
    tokens: Vec<TokenInArb>,
    options: ArbitrageEngineOptions,
    consecutive_failures: Arc<ArcSwap<u32>>,
    last_successful_fetch: Arc<ArcSwap<Instant>>,
}

impl ArbitrageOpportunityStream {
    pub fn new(
        rpc_client: Arc<RpcClient>,
        config: Arc<Config>,
        arbitrage_config: Arc<ArbitrageConfig>,
        tokens: Vec<TokenInArb>,
        options: ArbitrageEngineOptions,
    ) -> Self {
        Self {
            rpc_client,
            config,
            arbitrage_config,
            tokens,
            options,
            consecutive_failures: Arc::new(ArcSwap::new(Arc::new(0))),
            last_successful_fetch: Arc::new(ArcSwap::new(Arc::new(Instant::now()))),
        }
    }

    /// Start the arbitrage opportunity stream
    pub async fn start_stream(
        &self,
        opportunity_sender: mpsc::Sender<ArbOpportunity>,
    ) -> Result<()> {
        let mut fetch_interval = interval(Duration::from_millis(self.options.fetch_interval_ms));
        let circuit_breaker_threshold = self.arbitrage_config.breaker_threshold;

        loop {
            fetch_interval.tick().await;

            // Check circuit breaker
            let failures = **self.consecutive_failures.load();
            if failures >= circuit_breaker_threshold {
                warn!("Circuit breaker open, skipping fetch (failures: {})", failures);
                
                // Exponential backoff for recovery
                let backoff_duration = Duration::from_millis(
                    self.options.fetch_interval_ms * 2_u64.pow(failures.min(10))
                );
                sleep(backoff_duration).await;
                continue;
            }

            // Fetch opportunities with timeout and error handling
            match self.fetch_opportunities().await {
                Ok(opportunities) => {
                    // Reset failure counter on success
                    self.consecutive_failures.store(Arc::new(0));
                    self.last_successful_fetch.store(Arc::new(Instant::now()));

                    // Send opportunities to processor
                    for opportunity in opportunities {
                        if opportunity_sender.try_send(opportunity).is_err() {
                            warn!("Opportunity channel full, dropping opportunity");
                        }
                    }
                }
                Err(e) => {
                    // Increment failure counter
                    let new_failures = failures + 1;
                    self.consecutive_failures.store(Arc::new(new_failures));
                    
                    warn!("Failed to fetch opportunities (attempt {}): {}", new_failures, e);
                }
            }
        }
    }

    async fn fetch_opportunities(&self) -> Result<Vec<ArbOpportunity>> {
        // Simplified opportunity fetching - replace with actual logic
        let pools = self.get_active_pools().await?;
        
        if pools.is_empty() {
            return Ok(vec![]);
        }

        // Use simple cross-DEX comparison instead of complex graph algorithms
        let opportunities = self.find_simple_arbitrage(&pools).await?;
        
        Ok(opportunities)
    }

    async fn get_active_pools(&self) -> Result<Vec<Pool>> {
        // Simplified pool fetching - should be replaced with real-time pool data
        // For now, return a basic set of pools for testing
        Ok(vec![
            Pool {
                id: "test_pool_1".to_string(),
                token_a: "So11111111111111111111111111111111111111112".to_string(), // SOL
                token_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC
                liquidity: 50000.0,
            },
            Pool {
                id: "test_pool_2".to_string(),
                token_a: "So11111111111111111111111111111111111111112".to_string(), // SOL
                token_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC
                liquidity: 75000.0,
            },
        ])
    }

    async fn find_simple_arbitrage(&self, pools: &[Pool]) -> Result<Vec<ArbOpportunity>> {
        // Simplified arbitrage detection
        // In production, this would use the SimpleArbitrageCalculator
        let mut opportunities = Vec::new();
        
        // Group pools by token pairs
        let mut pools_by_pair: std::collections::HashMap<String, Vec<&Pool>> = std::collections::HashMap::new();
        
        for pool in pools {
            let pair_key = format!("{}-{}", pool.token_a, pool.token_b);
            pools_by_pair.entry(pair_key).or_insert_with(Vec::new).push(pool);
        }
        
        // Find arbitrage opportunities between pools of the same pair
        for (_, pair_pools) in pools_by_pair {
            if pair_pools.len() >= 2 {
                // Simple cross-pool arbitrage detection
                for i in 0..pair_pools.len() {
                    for j in (i + 1)..pair_pools.len() {
                        if let Some(opportunity) = self.calculate_opportunity(pair_pools[i], pair_pools[j]).await? {
                            opportunities.push(opportunity);
                        }
                    }
                }
            }
        }
        
        // Sort by profit and take top opportunities
        opportunities.sort_by(|a, b| b.expected_profit_lamports.cmp(&a.expected_profit_lamports));
        opportunities.truncate(10); // Limit to top 10
        
        Ok(opportunities)
    }

    async fn calculate_opportunity(&self, _pool_a: &Pool, _pool_b: &Pool) -> Result<Option<ArbOpportunity>> {
        // Simplified opportunity calculation
        // In production, this would use proper price calculations and slippage estimation
        
        // Mock calculation for testing
        let price_diff_ratio = 0.005; // 0.5% price difference
        let expected_profit_usd = 25.0; // $25 profit
        let sol_price = 100.0; // Assume $100 SOL
        
        if expected_profit_usd < self.arbitrage_config.min_profit_usd {
            return Ok(None);
        }
        
        let expected_profit_lamports = (expected_profit_usd / sol_price * 1_000_000_000.0) as u64;
        
        let opportunity = ArbOpportunity {
            path: crate::arbitrage::types::SwapPath {
                id_paths: vec![1, 2],
                hops: 2,
                paths: vec![], // Will be filled by execution planner
            },
            expected_profit_lamports,
            timestamp_unix_nanos: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            execution_plan: vec![], // Will be filled by execution planner
            metadata: crate::arbitrage::types::OpportunityMetadata {
                estimated_gas_cost: self.arbitrage_config.gas_cost_lamports,
                net_profit_lamports: expected_profit_lamports as i64 - self.arbitrage_config.gas_cost_lamports as i64,
                profit_percentage_bps: (price_diff_ratio * 10000.0) as u16,
                risk_score: 50, // Medium risk
                source: crate::arbitrage::types::OpportunitySource::PriceDiscrepancy {
                    dex_a: crate::markets::types::DexLabel::Raydium,
                    dex_b: crate::markets::types::DexLabel::Orca,
                },
                max_latency_ms: 200,
            },
        };
        
        Ok(Some(opportunity))
    }

    /// Get stream health statistics
    pub fn get_health_stats(&self) -> StreamHealthStats {
        let failures = **self.consecutive_failures.load();
        let last_fetch = **self.last_successful_fetch.load();
        let time_since_last_success = last_fetch.elapsed();
        
        StreamHealthStats {
            consecutive_failures: failures,
            time_since_last_success_ms: time_since_last_success.as_millis() as u64,
            circuit_breaker_active: failures >= self.arbitrage_config.breaker_threshold,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamHealthStats {
    pub consecutive_failures: u32,
    pub time_since_last_success_ms: u64,
    pub circuit_breaker_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arbitrage::config::ArbitrageConfig;
    use crate::common::config::Config;
    
    #[tokio::test]
    async fn test_stream_health_tracking() {
        let config = Arc::new(Config::default());
        let arbitrage_config = Arc::new(ArbitrageConfig::default());
        let rpc_client = Arc::new(RpcClient::new("http://localhost:8899".to_string()));
        let tokens = vec![];
        let options = ArbitrageEngineOptions::default();
        
        let stream = ArbitrageOpportunityStream::new(
            rpc_client,
            config,
            arbitrage_config,
            tokens,
            options,
        );
        
        let stats = stream.get_health_stats();
        assert_eq!(stats.consecutive_failures, 0);
        assert!(!stats.circuit_breaker_active);
    }
}
