//! src/arbitrage/config.rs
//! Arbitrage-specific configuration - extracted from calc_arb.rs

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArbitrageConfig {
    pub min_profit_usd: f64,
    pub max_slippage_bps: u16,
    pub max_cycle_detection_us: u64,
    pub breaker_threshold: u32,
    pub gas_cost_lamports: u64,
    pub jito_tip_lamports: u64,
    pub min_liquidity_usd: f64,
}

impl Default for ArbitrageConfig {
    fn default() -> Self {
        Self {
            min_profit_usd: 10.0,           // Realistic minimum for Solana
            max_slippage_bps: 30,           // 0.3% - tight for MEV
            max_cycle_detection_us: 1500,   // 1.5ms - realistic for 500 tokens
            breaker_threshold: 5,           // Circuit breaker after 5 failures
            gas_cost_lamports: 15_000,      // ~3 instructions on Solana
            jito_tip_lamports: 100_000,     // ~0.0001 SOL tip for Jito
            min_liquidity_usd: 1000.0,      // $1k minimum liquidity
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PositionSizer {
    pub max_position_usd: f64,
    pub max_percent_of_liquidity: f64,
    pub kelly_fraction: f64,
    pub historical_win_rate: f64,
    pub historical_win_loss_ratio: f64,
}

impl Default for PositionSizer {
    fn default() -> Self {
        Self {
            max_position_usd: 10_000.0,      // Maximum position size
            max_percent_of_liquidity: 0.02,  // 2% - conservative for HFT
            kelly_fraction: 0.25,            // 25% Kelly - very conservative
            historical_win_rate: 0.45,       // Realistic for arbitrage
            historical_win_loss_ratio: 1.3,  // Small but consistent profits
        }
    }
}

impl PositionSizer {
    /// Calculate optimal position size based on Kelly criterion and liquidity constraints
    pub fn calculate_position_size(&self, min_liquidity: f64) -> f64 {
        // Apply liquidity constraint
        let liquidity_constrained = min_liquidity * self.max_percent_of_liquidity;
        
        // Apply maximum position constraint
        let position = liquidity_constrained.min(self.max_position_usd);
        
        // Apply Kelly criterion for risk management
        let p = self.historical_win_rate;
        let b = self.historical_win_loss_ratio;
        let q = 1.0 - p;
        let kelly_optimal = (p * b - q) / b;
        
        // Only apply Kelly if positive expectancy
        if kelly_optimal > 0.0 {
            position * (kelly_optimal * self.kelly_fraction).min(1.0).max(0.0)
        } else {
            0.0 // Don't trade if negative expectancy
        }
    }
    
    /// Update historical performance data
    pub fn update_performance(&mut self, trade_won: bool, profit_ratio: f64) {
        // Simple exponential moving average update
        const ALPHA: f64 = 0.1;
        
        if trade_won {
            self.historical_win_rate = self.historical_win_rate * (1.0 - ALPHA) + ALPHA;
            self.historical_win_loss_ratio = self.historical_win_loss_ratio * (1.0 - ALPHA) + profit_ratio * ALPHA;
        } else {
            self.historical_win_rate = self.historical_win_rate * (1.0 - ALPHA);
        }
        
        // Keep within reasonable bounds
        self.historical_win_rate = self.historical_win_rate.clamp(0.1, 0.9);
        self.historical_win_loss_ratio = self.historical_win_loss_ratio.clamp(1.0, 5.0);
    }
}

// Error types for arbitrage operations
#[derive(Debug, thiserror::Error)]
pub enum ArbitrageError {
    #[error("Cycle detection timed out after {0} µs")]
    DetectionTimeout(u64),

    #[error("Insufficient liquidity: ${available:.2} < ${required:.2}")]
    InsufficientLiquidity { available: f64, required: f64 },

    #[error("Slippage exceeded: {actual_bps} bps > {max_bps} bps")]
    SlippageExceeded { actual_bps: u16, max_bps: u16 },

    #[error("MEV frontrun detected: expected ${expected:.2}, got ${actual:.2}")]
    MevFrontrun { expected: f64, actual: f64 },

    #[error("Circuit breaker open: {failures} consecutive failures")]
    CircuitOpen { failures: u32 },

    #[error("No profitable arbitrage found")]
    NoProfitableArbitrage,

    #[error("Invalid pool data: {0}")]
    InvalidPoolData(String),

    #[error("Position size too small: ${size:.2} < minimum")]
    PositionTooSmall { size: f64 },
}

// Fast canonical pair identifier
#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct PairId {
    pub lower: u32,
    pub higher: u32,
}

impl PairId {
    #[inline(always)]
    pub fn new(token_a_idx: u32, token_b_idx: u32) -> Self {
        if token_a_idx < token_b_idx {
            Self { lower: token_a_idx, higher: token_b_idx }
        } else {
            Self { lower: token_b_idx, higher: token_a_idx }
        }
    }

    #[inline(always)]
    pub fn hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        use ahash::AHasher;
        
        let mut hasher = AHasher::default();
        self.lower.hash(&mut hasher);
        self.higher.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pair_id_canonical() {
        let id1 = PairId::new(5, 10);
        let id2 = PairId::new(10, 5);
        assert_eq!(id1, id2);
        assert_eq!(id1.hash(), id2.hash());
    }

    #[test]
    fn test_position_sizing() {
        let sizer = PositionSizer::default();
        
        let position = sizer.calculate_position_size(100_000.0);
        
        // Should be limited by liquidity percentage
        let expected_max = 100_000.0 * 0.02; // 2% of 100k
        assert!(position <= expected_max);
        assert!(position <= sizer.max_position_usd);
        assert!(position > 0.0);
    }

    #[test]
    fn test_performance_update() {
        let mut sizer = PositionSizer::default();
        let initial_win_rate = sizer.historical_win_rate;
        
        // Simulate a winning trade
        sizer.update_performance(true, 1.5);
        assert!(sizer.historical_win_rate > initial_win_rate);
        
        // Simulate a losing trade
        sizer.update_performance(false, 0.0);
        // Win rate should decrease but not go below 0.1
        assert!(sizer.historical_win_rate >= 0.1);
    }

    #[test]
    fn test_default_configs() {
        let arb_config = ArbitrageConfig::default();
        assert!(arb_config.min_profit_usd > 0.0);
        assert!(arb_config.max_slippage_bps > 0);
        assert!(arb_config.gas_cost_lamports > 0);
        
        let position_config = PositionSizer::default();
        assert!(position_config.max_position_usd > 0.0);
        assert!(position_config.historical_win_rate > 0.0);
        assert!(position_config.historical_win_rate < 1.0);
    }
} 