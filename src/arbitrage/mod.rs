pub mod calc_arb;
pub mod config;
pub mod path_evaluator;
pub mod path_statistics;
pub mod simulate;
pub mod streams;
pub mod strategies;
pub mod types;

// Re-export main types for convenience
pub use calc_arb::{ArbitragePool, ArbitrageStats, SimpleArbitrageCalculator, SimpleArbitrageOpportunity};
pub use config::{ArbitrageConfig, ArbitrageError, PairId, PositionSizer};
pub use types::{ArbOpportunity, SwapPath, TokenInArb};

// For backwards compatibility if needed
pub type ArbitragePath = SwapPath;
