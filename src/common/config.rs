use serde::{Deserialize, Serialize};
use std::fs;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub tokens_to_trade: Vec<TokenConfig>,
    pub min_profit_threshold: f64,
    pub max_slippage: f64,
    pub enable_paper_trading: bool,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tokens_to_trade: vec![],
            min_profit_threshold: 20.0,
            max_slippage: 0.02,
            enable_paper_trading: false,
        }
    }
}
