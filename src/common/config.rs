//! src/common/config.rs - Updated with missing fields

use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub initial_portfolio_value_usd: Option<f64>,
    pub max_daily_drawdown: f64,
    pub max_trade_size_percentage: f64,
    pub profit_sanity_check_percentage: f64,
    pub token_whitelist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataMode {
    WebSocket(String),
    Grpc(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInputConfig {
    pub tokens_to_arb: Vec<TokenConfig>,
    pub get_fresh_pools_bool: Option<bool>,
    pub include_1hop: Option<bool>,
    pub include_2hop: Option<bool>,
    pub numbers_of_best_paths: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    pub address: String,
    pub symbol: String,
    pub decimals: u8,
}

// From implementation for TokenInArb conversion
impl From<&TokenConfig> for crate::arbitrage::types::TokenInArb {
    fn from(tc: &TokenConfig) -> Self {
        crate::arbitrage::types::TokenInArb {
            token: tc.address.clone(),
            symbol: tc.symbol.clone(),
            decimals: tc.decimals,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // Simple RPC configuration
    pub rpc_url: String,
    pub websocket_url: String,
    
    // Core settings
    pub vault_url: Option<String>,
    pub execution_mode: String, // "Live", "Paper", "Simulate"
    pub simulation_amount: u64,

    // Strategy settings
    pub active_strategies: Vec<String>,
    pub massive_strategy_inputs: Vec<StrategyInputConfig>,
    pub path_best_strategy: String,
    pub top_n_ultra_paths: Option<usize>,

    // Performance and management
    pub executor_queue_size: Option<usize>,
    pub fee_multiplier: Option<f64>,
    pub fetch_new_pools: Option<bool>,
    pub restrict_sol_usdc: Option<bool>,
    pub output_dir: Option<String>,
    pub statistics_file_path: Option<String>,
    pub statistics_save_interval_secs: Option<u64>,

    // Data sources
    pub data_mode: DataMode,

    // Module configurations
    pub risk_management: RiskConfig,
    
    // Transaction execution settings
    pub compute_unit_limit: Option<u32>,              // Default: 400_000
    pub transaction_confirmation_timeout_secs: Option<u64>, // Default: 30
    pub transaction_poll_interval_ms: Option<u64>,     // Default: 500
    pub max_send_retries: Option<u32>,                // Default: 3
    
    // Paper trading settings
    pub paper_trade_mock_gas_cost: Option<u64>,       // Default: 5000
    pub paper_trade_mock_execution_time_ms: Option<u64>, // Default: 100
    
    // Priority fee settings
    pub fee_cache_duration_secs: Option<u64>,         // Default: 2
    
    // Queue management
    pub max_queue_size: Option<usize>,                // Default: 1000
    
    // Slippage settings
    pub max_slippage_bps: Option<u16>,                // Default: 100 (1%)
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_str = fs::read_to_string("config.json")?;
        
        // Expand environment variables in the config string
        let expanded_config_str = expand_env_vars(&config_str);
        
        let mut config: Config = serde_json::from_str(&expanded_config_str)?;
        
        // Validate URLs
        config.validate_urls()?;
        
        Ok(config)
    }

    /// Validate URLs and provide fallbacks if needed
    fn validate_urls(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Validate RPC URL
        if self.rpc_url.is_empty() || self.rpc_url.starts_with("${") {
            return Err("QUICKNODE_RPC_URL environment variable not set or invalid".into());
        }
        
        // Validate WebSocket URL
        if self.websocket_url.is_empty() || self.websocket_url.starts_with("${") {
            return Err("QUICKNODE_WSS_URL environment variable not set or invalid".into());
        }
        
        Ok(())
    }

    pub fn contains_strategy(&self, strategy_name: &str) -> bool {
        self.active_strategies.contains(&strategy_name.to_string())
    }
    
    /// Get RPC URL
    pub fn get_rpc_url(&self) -> String {
        self.rpc_url.clone()
    }
    
    /// Get WebSocket URL
    pub fn get_websocket_url(&self) -> String {
        self.websocket_url.clone()
    }
}

/// Expand environment variables in a string
/// Supports ${VAR_NAME} syntax
fn expand_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    
    // Simple regex-like replacement for ${VAR_NAME}
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_else(|_| {
                tracing::warn!("Environment variable {} not found, keeping placeholder", var_name);
                format!("${{{}}}", var_name)
            });
            result.replace_range(start..start + end + 1, &replacement);
        } else {
            break;
        }
    }
    
    result
}

// Strategy constants
pub const STRATEGY_MASSIVE: &str = "Massive";
pub const STRATEGY_BEST_PATH: &str = "BestPath";

// Execution mode constants
pub const EXECUTION_MODE_LIVE: &str = "Live";
pub const EXECUTION_MODE_PAPER: &str = "Paper";
pub const EXECUTION_MODE_SIMULATE: &str = "Simulate";
