//! src/arbitrage/types.rs - HFT-Optimized Types
//! All calculations done during discovery, execution just builds instructions

use crate::markets::types::DexLabel;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
// use std::time::SystemTime; // Removed unused import

/// Pre-calculated swap execution details for one leg of arbitrage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SwapLeg {
    /// DEX where this swap executes
    pub dex: DexLabel,
    
    /// Pool/Market address for this swap
    pub pool_address: Pubkey,
    
    /// Input token mint
    pub token_in: Pubkey,
    
    /// Output token mint  
    pub token_out: Pubkey,
    
    /// Exact amount of tokens to swap (in token_in decimals)
    pub amount_in: u64,
    
    /// Minimum acceptable output (includes slippage tolerance)
    pub minimum_amount_out: u64,
    
    /// Expected output based on current pool state (for monitoring)
    pub expected_amount_out: u64,
    
    /// Direction flag for DEXs that need it (e.g. swap_for_y)
    pub swap_direction: bool,
    
    /// Pool-specific data that might be needed
    pub pool_data: PoolExecutionData,
}

/// Pool-specific execution data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PoolExecutionData {
    /// Meteora DLMM specific data
    Meteora {
        bin_id: Option<i32>,
        /// Pre-calculated price impact
        price_impact_bps: u16,
    },
    /// Raydium specific data
    Raydium {
        /// AMM program variant
        amm_version: u8,
    },
    /// Orca Whirlpools specific data
    OrcaWhirlpools {
        /// Tick spacing for the pool
        tick_spacing: u16,
        /// Current tick (if needed for calculation)
        current_tick: Option<i32>,
    },
    /// Generic for other DEXs
    Generic,
}

/// Enhanced arbitrage opportunity with pre-calculated execution plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ArbOpportunity {
    /// Original path structure (for compatibility)
    pub path: SwapPath,
    
    /// Expected profit in lamports (SOL smallest unit)
    pub expected_profit_lamports: u64,
    
    /// When this opportunity was discovered (Unix nanoseconds)
    pub timestamp_unix_nanos: u128,
    
    /// Pre-calculated execution plan with all swap details
    pub execution_plan: Vec<SwapLeg>,
    
    /// Metadata for monitoring and analysis
    pub metadata: OpportunityMetadata,
}

/// Additional metadata for opportunity tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OpportunityMetadata {
    /// Total gas cost estimate in lamports
    pub estimated_gas_cost: u64,
    
    /// Net profit after gas (expected_profit_lamports - estimated_gas_cost)
    pub net_profit_lamports: i64,
    
    /// Profit percentage (net_profit / initial_amount * 100)
    pub profit_percentage_bps: u16, // Basis points (100 = 1%)
    
    /// Risk score (0-100, higher = riskier)
    pub risk_score: u8,
    
    /// Source of opportunity discovery
    pub source: OpportunitySource,
    
    /// Maximum acceptable latency for execution (milliseconds)
    pub max_latency_ms: u16,
}

/// Source of arbitrage opportunity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OpportunitySource {
    /// Real-time market event (e.g., large trade)
    MarketEvent { pool_id: u64, event_type: String },
    
    /// Periodic strategy scan
    StrategyScan { strategy_name: String },
    
    /// Cross-DEX price discrepancy
    PriceDiscrepancy { dex_a: DexLabel, dex_b: DexLabel },
    
    /// External signal (e.g., oracle price update)
    ExternalSignal { source: String },
}

// Keep existing types for compatibility or if used by other parts not yet refactored

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)] // Added PartialEq, Eq, Hash back if path is used as key
pub struct SwapPath {
    pub id_paths: Vec<u32>,
    pub hops: usize, // Changed from u32 to usize to match some existing uses, verify consistency
    pub paths: Vec<Route>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)] // Added PartialEq, Eq, Hash back
pub struct Route {
    pub id: u32,
    pub dex: DexLabel,
    pub pool_address: String, // Kept as String, new SwapLeg uses Pubkey. Conversion needed.
    pub token_in: String,     // Kept as String
    pub token_out: String,    // Kept as String
    pub token_0to1: bool,     // Corresponds to swap_direction in SwapLeg
    // Removed fee, decimals_in, decimals_out as they might be part of pool data or fetched differently
}

#[derive(Debug, Clone, Serialize, Deserialize)] // Removed PartialEq, Eq, Hash due to f64 fields
pub struct SwapPathSelected { // This might become less relevant if ArbOpportunity is fully adopted
    pub path: SwapPath,
    pub expected_profit_usd: f64,
    pub markets: Vec<Market>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)] // Added Serialize, Deserialize
pub struct Market { // This might be simplified or absorbed into pool data
    pub id: String,
    pub dex_label: crate::markets::types::DexLabel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)] // Added PartialEq, Eq, Hash
pub struct TokenInArb {
    pub token: String, // Address as String
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfos {
    pub address: String,
    pub symbol: String,
    pub decimals: u8,
    // price_usd is commented out as per your provided types.rs.
    // If strategies.rs or other parts need it, it must be uncommented or handled.
    // pub price_usd: f64, 
}

// Swap simulation result (used by create_transaction.rs, may need update or replacement)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapPathResult {
    pub path_id: u32, // Changed from usize to match SwapPath.id_paths element type
    pub hops: u32,    // Changed from usize
    pub tokens_path: String, // Added from earlier version of types.rs
    pub route_simulations: Vec<SwapRouteSimulation>,
    pub token_in: String, // Added
    pub token_in_symbol: String, // Added
    pub token_out: String, // Added
    pub token_out_symbol: String, // Added
    pub amount_in: u64, // Renamed from begin_amount
    pub estimated_amount_out: String, // Added
    pub estimated_min_amount_out: String, // Added
    pub result: f64, // Profit/loss from simulation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRouteSimulation {
    pub id_route: u32,
    pub pool_address: String,
    pub dex_label: DexLabel,
    pub token_in: String,
    pub token_out: String,
    pub token_0to1: bool,
    pub amount_in: u64,
    pub estimated_amount_out: String, // Kept as String, parsing done elsewhere
    pub minimum_amount_out: u64, // This was u64 in your example
}

// Helper functions for HFT calculations (no floating point in hot path!)

impl SwapLeg {
    /// Calculate slippage in basis points (100 = 1%)
    pub fn slippage_bps(&self) -> u16 {
        if self.expected_amount_out == 0 {
            return 0; // Avoid division by zero
        }
        
        // Ensure minimum_amount_out is not greater than expected_amount_out
        let diff = self.expected_amount_out.saturating_sub(self.minimum_amount_out);
        // bps = (difference / expected_out) * 10000
        // To avoid floating point, multiply by 10000 first, then divide.
        // Ensure intermediate multiplication doesn't overflow u64.
        // If diff is small, (diff * 10000) might be fine.
        // If diff can be large, consider u128 for intermediate.
        // For now, assuming diff * 10000 fits in u64.
        let bps = (diff as u128 * 10000 / self.expected_amount_out as u128) as u64;
        bps.min(10000) as u16 // Cap at 100% slippage (10000 bps)
    }
    
    /// Check if slippage is within acceptable range
    pub fn is_slippage_acceptable(&self, max_slippage_bps: u16) -> bool {
        self.slippage_bps() <= max_slippage_bps
    }
}

impl ArbOpportunity {
    /// Total amount in for the arbitrage (first leg input)
    pub fn initial_amount(&self) -> u64 {
        self.execution_plan.first()
            .map(|leg| leg.amount_in)
            .unwrap_or(0)
    }
    
    /// Final expected output (last leg output)
    pub fn final_expected_output(&self) -> u64 {
        self.execution_plan.last()
            .map(|leg| leg.expected_amount_out)
            .unwrap_or(0)
    }
    
    /// Check if opportunity is still profitable after gas
    pub fn is_profitable(&self) -> bool {
        self.metadata.net_profit_lamports > 0
    }
    
    /// Check if all legs have acceptable slippage
    pub fn validate_slippage(&self, max_slippage_bps: u16) -> bool {
        self.execution_plan.iter()
            .all(|leg| leg.is_slippage_acceptable(max_slippage_bps))
    }
    
    /// Get total number of swaps
    pub fn swap_count(&self) -> usize {
        self.execution_plan.len()
    }
}

/// Enhanced arbitrage engine options for production use
#[derive(Debug, Clone)]
pub struct ArbitrageEngineOptions {
    pub fetch_interval_ms: u64,
    pub max_opportunities_per_cycle: usize,
    pub enable_circuit_breaker: bool,
    pub backoff_multiplier: f64,
    pub max_backoff_ms: u64,
}

impl Default for ArbitrageEngineOptions {
    fn default() -> Self {
        Self {
            fetch_interval_ms: 100,
            max_opportunities_per_cycle: 20,
            enable_circuit_breaker: true,
            backoff_multiplier: 2.0,
            max_backoff_ms: 30000, // 30 seconds
        }
    }
}
