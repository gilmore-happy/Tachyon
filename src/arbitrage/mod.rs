pub mod calc_arb;
pub mod config;
pub mod path_evaluator;
pub mod path_statistics;
pub mod simulate;
pub mod streams;
pub mod strategies;
pub mod types;

// Re-export main types for convenience
pub use calc_arb::ArbitragePool;
pub use config::{ArbitrageConfig, PairId};
pub use types::SwapPath;

// For backwards compatibility if needed
pub type ArbitragePath = SwapPath;
