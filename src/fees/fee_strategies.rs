use anyhow::Result;
use log::{info, debug};
use crate::fees::fee_cache::CachedFeeData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Urgency {
    Low,      // Can wait, use base fees
    Normal,   // Standard arbitrage opportunity
    High,     // High profit opportunity, compete aggressively
    Critical, // Extreme profit, must land transaction
}

/// Trait for different fee calculation strategies
pub trait FeeStrategy: Send + Sync + std::fmt::Debug {
    fn calculate_fee(
        &self,
        profit_lamports: u64,
        fee_data: &CachedFeeData,
        urgency: Urgency,
    ) -> u64;
    
    fn name(&self) -> &'static str;
}

/// Profit-based fee strategy with dynamic scaling
#[derive(Debug)]
pub struct ProfitBasedStrategy {
    /// Percentage of profit to use as fee (in basis points, 100 = 1%)
    pub profit_percentage_bps: u64,
    /// Minimum fee in microlamports
    pub min_fee: u64,
    /// Maximum fee in microlamports  
    pub max_fee: u64,
    /// Multiplier for high urgency (in basis points, 10000 = 1x)
    pub high_urgency_multiplier_bps: u64,
    /// Multiplier for critical urgency (in basis points, 10000 = 1x)
    pub critical_urgency_multiplier_bps: u64,
}

impl Default for ProfitBasedStrategy {
    fn default() -> Self {
        Self {
            profit_percentage_bps: 50,  // 0.5% of profit
            min_fee: 10_000,            // 0.00001 SOL minimum
            max_fee: 10_000_000,        // 0.01 SOL maximum
            high_urgency_multiplier_bps: 15_000,     // 1.5x for high urgency
            critical_urgency_multiplier_bps: 20_000, // 2x for critical urgency
        }
    }
}

impl ProfitBasedStrategy {
    pub fn new(
        profit_percentage_bps: u64,
        min_fee: u64,
        max_fee: u64,
    ) -> Self {
        Self {
            profit_percentage_bps,
            min_fee,
            max_fee,
            ..Default::default()
        }
    }

    /// Calculate fee based on profit tiers
    fn calculate_tiered_fee(&self, profit_lamports: u64) -> u64 {
        let profit_sol = profit_lamports as f64 / 1e9;
        
        // Tiered percentage based on profit size
        let percentage_bps = match profit_sol {
            p if p < 1.0 => 25,      // 0.25% for < 1 SOL
            p if p < 10.0 => 50,     // 0.5% for 1-10 SOL
            p if p < 50.0 => 100,    // 1% for 10-50 SOL
            p if p < 100.0 => 150,   // 1.5% for 50-100 SOL
            _ => 200,                // 2% for > 100 SOL
        };
        
        (profit_lamports * percentage_bps) / 10_000
    }
}

impl FeeStrategy for ProfitBasedStrategy {
    fn calculate_fee(
        &self,
        profit_lamports: u64,
        fee_data: &CachedFeeData,
        urgency: Urgency,
    ) -> u64 {
        // Start with base calculation
        let profit_based_fee = self.calculate_tiered_fee(profit_lamports);
        
        // Use percentile based on urgency
        let market_fee = match urgency {
            Urgency::Low => fee_data.base_fee,
            Urgency::Normal => fee_data.percentile_75,
            Urgency::High => fee_data.percentile_90,
            Urgency::Critical => fee_data.percentile_95,
        };
        
        // Take the higher of profit-based or market-based fee
        let mut fee = profit_based_fee.max(market_fee);
        
        // Apply urgency multiplier
        fee = match urgency {
            Urgency::High => (fee * self.high_urgency_multiplier_bps) / 10_000,
            Urgency::Critical => (fee * self.critical_urgency_multiplier_bps) / 10_000,
            _ => fee,
        };
        
        // Apply bounds
        fee = fee.max(self.min_fee).min(self.max_fee);
        
        debug!(
            "💰 Fee calculation: profit={:.3} SOL, urgency={:?}, base_fee={}, final_fee={} ({:.6} SOL)",
            profit_lamports as f64 / 1e9,
            urgency,
            market_fee,
            fee,
            fee as f64 / 1e9
        );
        
        fee
    }
    
    fn name(&self) -> &'static str {
        "ProfitBasedStrategy"
    }
}

/// Conservative strategy for testing
#[derive(Debug)]
pub struct ConservativeStrategy {
    pub base_multiplier_bps: u64,
}

impl Default for ConservativeStrategy {
    fn default() -> Self {
        Self {
            base_multiplier_bps: 12_000, // 1.2x base fee
        }
    }
}

impl FeeStrategy for ConservativeStrategy {
    fn calculate_fee(
        &self,
        _profit_lamports: u64,
        fee_data: &CachedFeeData,
        urgency: Urgency,
    ) -> u64 {
        let base = match urgency {
            Urgency::Low => fee_data.base_fee,
            Urgency::Normal => fee_data.percentile_75,
            Urgency::High => fee_data.percentile_90,
            Urgency::Critical => fee_data.percentile_95,
        };
        
        (base * self.base_multiplier_bps) / 10_000
    }
    
    fn name(&self) -> &'static str {
        "ConservativeStrategy"
    }
}

/// Aggressive strategy for high competition environments
#[derive(Debug)]
pub struct AggressiveStrategy {
    pub min_multiplier_bps: u64,
    pub max_multiplier_bps: u64,
}

impl Default for AggressiveStrategy {
    fn default() -> Self {
        Self {
            min_multiplier_bps: 15_000,  // 1.5x minimum
            max_multiplier_bps: 30_000,  // 3x maximum
        }
    }
}

impl FeeStrategy for AggressiveStrategy {
    fn calculate_fee(
        &self,
        profit_lamports: u64,
        fee_data: &CachedFeeData,
        urgency: Urgency,
    ) -> u64 {
        // Use high percentiles as base
        let base = match urgency {
            Urgency::Low => fee_data.percentile_75,
            Urgency::Normal => fee_data.percentile_90,
            Urgency::High => fee_data.percentile_95,
            Urgency::Critical => fee_data.max_recent_fee,
        };
        
        // Scale multiplier based on profit
        let profit_sol = profit_lamports as f64 / 1e9;
        let multiplier_bps = if profit_sol > 50.0 {
            self.max_multiplier_bps
        } else {
            let ratio = profit_sol / 50.0;
            self.min_multiplier_bps + 
                ((self.max_multiplier_bps - self.min_multiplier_bps) as f64 * ratio) as u64
        };
        
        (base * multiplier_bps) / 10_000
    }
    
    fn name(&self) -> &'static str {
        "AggressiveStrategy"
    }
}

/// Determine urgency based on profit amount
pub fn determine_urgency(profit_lamports: u64) -> Urgency {
    let profit_sol = profit_lamports as f64 / 1e9;
    
    match profit_sol {
        p if p < 0.1 => Urgency::Low,
        p if p < 1.0 => Urgency::Normal,
        p if p < 10.0 => Urgency::High,
        _ => Urgency::Critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profit_based_strategy() {
        let strategy = ProfitBasedStrategy::default();
        let fee_data = CachedFeeData {
            base_fee: 1_000,
            percentile_75: 5_000,
            percentile_90: 10_000,
            percentile_95: 20_000,
            max_recent_fee: 50_000,
            timestamp: std::time::Instant::now(),
        };
        
        // Test small profit
        let fee = strategy.calculate_fee(100_000_000, &fee_data, Urgency::Normal); // 0.1 SOL profit
        assert!(fee >= strategy.min_fee);
        
        // Test large profit
        let fee = strategy.calculate_fee(50_000_000_000, &fee_data, Urgency::Critical); // 50 SOL profit
        assert!(fee <= strategy.max_fee);
    }
}
