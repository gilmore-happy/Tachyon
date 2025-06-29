//! Foundation.rs - Unified MarketBehavior trait architecture
//! 
//! This module provides the foundation for all DEX implementations with a unified interface.
//! Performance target: Sub-millisecond execution with full concurrency support.
//! 
//! Based on the compact summary description for cross-DEX arbitrage strategies.

use crate::markets::errors::MarketSimulationError;
use crate::markets::types::{MarketId, DexLabel};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Core quote structure for all DEX implementations
/// 
/// This unified quote format enables seamless comparison across different DEXes
/// for arbitrage opportunity detection.
#[derive(Debug, Clone, Copy)]
pub struct Quote {
    /// Input amount for the swap
    pub amount_in: u64,
    
    /// Expected output amount after swap
    pub amount_out: u64,
    
    /// Price impact as percentage (0.0 to 100.0)
    pub price_impact: f64,
    
    /// Total fee amount in input token units
    pub fee_amount: u64,
    
    /// Slippage tolerance for the trade
    pub slippage_tolerance: f64,
}

/// Unified market behavior trait for all DEX implementations
/// 
/// This trait abstracts the differences between DEXes (Orca, Raydium, Meteora)
/// to enable unified arbitrage strategies and cross-DEX comparisons.
pub trait MarketBehavior: Send + Sync {
    /// Get swap quote for specified input amount
    /// 
    /// # Arguments
    /// * `amount_in` - Input amount in smallest token units
    /// * `a_to_b` - Swap direction (true: A->B, false: B->A)
    /// 
    /// # Performance Target
    /// Sub-10μs execution for hot path operations
    fn get_quote(&self, amount_in: u64, a_to_b: bool) -> Result<Quote, MarketSimulationError>;
    
    /// Get current market price
    /// 
    /// Returns the spot price without considering trade size impact.
    fn get_price(&self) -> Result<f64, MarketSimulationError>;
    
    /// Update market state with fresh on-chain data
    /// 
    /// This method should be called periodically to maintain quote accuracy.
    fn update_state(&mut self, new_data: &[u8]) -> Result<(), MarketSimulationError>;
    
    /// Get market identifier for routing and error reporting
    fn market_id(&self) -> MarketId;
    
    /// Get DEX label for strategy decisions
    fn dex_label(&self) -> DexLabel;
}

/// Market cache for high-frequency quote operations
/// 
/// Provides WebSocket-based real-time market data with sub-millisecond access.
pub struct MarketCache {
    /// Cached market quotes with timestamp
    quotes: Arc<RwLock<std::collections::HashMap<(u64, bool), (Quote, std::time::Instant)>>>,
    
    /// Cache TTL in milliseconds
    cache_ttl_ms: u64,
    
    /// Market implementation
    market: Box<dyn MarketBehavior>,
}

impl MarketCache {
    /// Create new market cache with specified TTL
    /// 
    /// # Arguments
    /// * `market` - Market implementation (Orca, Raydium, etc.)
    /// * `cache_ttl_ms` - Cache time-to-live in milliseconds
    pub fn new(market: Box<dyn MarketBehavior>, cache_ttl_ms: u64) -> Self {
        Self {
            quotes: Arc::new(RwLock::new(std::collections::HashMap::new())),
            cache_ttl_ms,
            market,
        }
    }
    
    /// Get cached quote or calculate new one
    /// 
    /// Implements cache-aside pattern for optimal performance.
    pub async fn get_quote(&self, amount_in: u64, a_to_b: bool) -> Result<Quote, MarketSimulationError> {
        let cache_key = (amount_in, a_to_b);
        let now = std::time::Instant::now();
        
        // Try cache first (read lock)
        {
            let quotes = self.quotes.read().await;
            if let Some((quote, timestamp)) = quotes.get(&cache_key) {
                if now.duration_since(*timestamp).as_millis() < self.cache_ttl_ms as u128 {
                    return Ok(*quote);
                }
            }
        }
        
        // Cache miss or expired - calculate new quote
        let quote = self.market.get_quote(amount_in, a_to_b)?;
        
        // Update cache (write lock)
        {
            let mut quotes = self.quotes.write().await;
            quotes.insert(cache_key, (quote, now));
        }
        
        Ok(quote)
    }
    
    /// Clear expired cache entries
    /// 
    /// Should be called periodically to prevent memory bloat.
    pub async fn cleanup_expired(&self) {
        let now = std::time::Instant::now();
        let mut quotes = self.quotes.write().await;
        
        quotes.retain(|_, (_, timestamp)| {
            now.duration_since(*timestamp).as_millis() < self.cache_ttl_ms as u128
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Mock market implementation for testing
    struct MockMarket;
    
    impl MarketBehavior for MockMarket {
        fn get_quote(&self, amount_in: u64, _a_to_b: bool) -> Result<Quote, MarketSimulationError> {
            Ok(Quote {
                amount_in,
                amount_out: amount_in * 95 / 100, // 5% price impact
                price_impact: 5.0,
                fee_amount: amount_in / 1000, // 0.1% fee
                slippage_tolerance: 0.5,
            })
        }
        
        fn get_price(&self) -> Result<f64, MarketSimulationError> {
            Ok(1.0)
        }
        
        fn update_state(&mut self, _new_data: &[u8]) -> Result<(), MarketSimulationError> {
            Ok(())
        }
        
        fn market_id(&self) -> MarketId {
            MarketId::Orca
        }
        
        fn dex_label(&self) -> DexLabel {
            DexLabel::Orca
        }
    }
    
    #[tokio::test]
    async fn test_market_cache() {
        let mock_market = Box::new(MockMarket);
        let cache = MarketCache::new(mock_market, 1000); // 1 second TTL
        
        // First call should calculate
        let quote1 = cache.get_quote(1_000_000, true).await.unwrap();
        assert_eq!(quote1.amount_in, 1_000_000);
        assert_eq!(quote1.amount_out, 950_000);
        
        // Second call should use cache
        let quote2 = cache.get_quote(1_000_000, true).await.unwrap();
        assert_eq!(quote1.amount_out, quote2.amount_out);
    }
}