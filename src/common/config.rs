use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::execution::executor::ExecutionMode;
use crate::fees::priority_fees::FeeMode;

// Define the Strategy enum for more organized strategy selection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Strategy {
    Massive,
    BestPath,
    Optimism,
    All,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub tokens_to_arb: Vec<TokenConfig>,
    pub include_1hop: bool,
    pub include_2hop: bool,
    pub numbers_of_best_paths: usize,
    pub get_fresh_pools_bool: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    // Token configuration
    pub tokens_to_trade: Vec<TokenConfig>,
    pub input_vectors: Vec<InputConfig>,

    // Strategy parameters
    pub active_strategies: Vec<Strategy>,
    pub simulation_amount: u64, // Example: 3500000000 (3.5 SOL in lamports)
    pub min_profit_threshold_lamports: u64, // Minimum profit threshold in lamports (e.g. 20000000 = 0.02 SOL)

    // Profit and slippage thresholds
    pub min_profit_threshold: f64, // Example: 20.0 (USD or equivalent value)
    pub max_slippage: f64,         // Example: 0.02 (2%)

    // Execution configuration
    #[serde(with = "execution_mode_serde")]
    pub execution_mode: ExecutionMode,

    // Massive strategy options
    pub fetch_new_pools: bool,
    pub restrict_sol_usdc: bool,

    // Path configuration
    pub path_best_strategie: String, // Example: "best_paths_selected/ultra_strategies/0-SOL-SOLLY-1-SOL-SPIKE-2-SOL-AMC-GME.json"
    pub optimism_path: String, // Example: "optimism_transactions/11-6-2024-SOL-SOLLY-SOL-0.json"

    // Fee configuration
    #[serde(with = "fee_mode_serde")]
    pub fee_mode: FeeMode,
    pub fee_cache_duration_secs: u64,

    // Optional URL configuration (can be overridden by environment variables)
    pub rpc_url: Option<String>,
    pub rpc_url_tx: Option<String>,
    pub wss_rpc_url: Option<String>,

    // Cache directories
    pub cache_dir: String,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    pub address: String,
    pub symbol: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Try to load from config file, otherwise use defaults
        if let Ok(contents) = fs::read_to_string("config.json") {
            Ok(serde_json::from_str(&contents)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        // Save the configuration to a file
        let contents = serde_json::to_string_pretty(self)?;
        fs::write("config.json", contents)?;
        Ok(())
    }

    pub fn contains_strategy(&self, strategy_name: &str) -> bool {
        // Map the string strategy name to our Strategy enum
        let strategy = match strategy_name {
            STRATEGY_MASSIVE => Strategy::Massive,
            STRATEGY_BEST_PATH => Strategy::BestPath,
            STRATEGY_OPTIMISM => Strategy::Optimism,
            STRATEGY_ALL => Strategy::All,
            _ => Strategy::None,
        };

        // Check if the strategy is in the active strategies or if "All" is active
        self.active_strategies.contains(&strategy) || self.active_strategies.contains(&Strategy::All)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tokens_to_trade: vec![TokenConfig {
                address: "So11111111111111111111111111111111111111112".to_string(),
                symbol: "SOL".to_string(),
            }],
            input_vectors: vec![InputConfig {
                tokens_to_arb: vec![TokenConfig {
                    address: "So11111111111111111111111111111111111111112".to_string(),
                    symbol: "SOL".to_string(),
                }],
                include_1hop: false,
                include_2hop: false,
                numbers_of_best_paths: 1,
                get_fresh_pools_bool: false,
            }],
            active_strategies: vec![Strategy::Massive, Strategy::BestPath],
            simulation_amount: 3_500_000_000, // 3.5 SOL in lamports
            min_profit_threshold_lamports: 20_000_000, // 0.02 SOL minimum profit threshold
            min_profit_threshold: 20.0,       // Default profit threshold (USD or equivalent)
            max_slippage: 0.02,               // Default max slippage 2%
            execution_mode: ExecutionMode::Simulate, // Default to simulation mode for safety
            fetch_new_pools: false,
            restrict_sol_usdc: true,
            path_best_strategie:
                "best_paths_selected/ultra_strategies/0-SOL-SOLLY-1-SOL-SPIKE-2-SOL-AMC-GME.json"
                    .to_string(),
            optimism_path: "optimism_transactions/11-6-2024-SOL-SOLLY-SOL-0.json".to_string(),
            fee_mode: FeeMode::ProfitBased,
            fee_cache_duration_secs: 2,
            rpc_url: None, // Will use environment variables by default
            rpc_url_tx: None,
            wss_rpc_url: None,
            cache_dir: "src/markets/cache".to_string(),
            output_dir: "best_paths_selected".to_string(),
        }
    }
}

// Constants for strategy names
pub const STRATEGY_MASSIVE: &str = "Massive";
pub const STRATEGY_BEST_PATH: &str = "BestPath";
pub const STRATEGY_OPTIMISM: &str = "Optimism";
pub const STRATEGY_ALL: &str = "All";
pub const STRATEGY_NONE: &str = "None";

// Serde helper modules for enum serialization/deserialization
mod execution_mode_serde {
    use crate::execution::executor::ExecutionMode;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(mode: &ExecutionMode, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mode_str = match mode {
            ExecutionMode::Live => "Live",
            ExecutionMode::Paper => "Paper",
            ExecutionMode::Simulate => "Simulate",
        };
        serializer.serialize_str(mode_str)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ExecutionMode, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mode_str = String::deserialize(deserializer)?;
        match mode_str.as_str() {
            "Live" => Ok(ExecutionMode::Live),
            "Paper" => Ok(ExecutionMode::Paper),
            _ => Ok(ExecutionMode::Simulate), // Default to Simulate
        }
    }
}

mod fee_mode_serde {
    use crate::fees::priority_fees::FeeMode;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(mode: &FeeMode, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mode_str = match mode {
            FeeMode::Conservative => "Conservative",
            FeeMode::Aggressive => "Aggressive",
            FeeMode::ProfitBased => "ProfitBased",
        };
        serializer.serialize_str(mode_str)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<FeeMode, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mode_str = String::deserialize(deserializer)?;
        match mode_str.as_str() {
            "Conservative" => Ok(FeeMode::Conservative),
            "Aggressive" => Ok(FeeMode::Aggressive),
            _ => Ok(FeeMode::ProfitBased), // Default to ProfitBased
        }
    }
}
