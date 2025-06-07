use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use log::{info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::{arbitrage::types::SwapPathResult, execution::executor::ExecutionResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradeRecord {
    pub timestamp: DateTime<Utc>,
    pub swap_path: SwapPathResult,
    pub simulated_profit: i64,
    pub simulated_gas: u64,
    pub net_profit: i64,
    pub success: bool,
    pub failure_reason: Option<String>,
    pub market_conditions: MarketConditions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConditions {
    pub network_congested: bool,
    pub slippage_applied: f64,
    pub priority_fee: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradingState {
    pub starting_balance: u64,
    pub current_balance: u64,
    pub total_trades: u64,
    pub successful_trades: u64,
    pub failed_trades: u64,
    pub total_profit: i64,
    pub total_gas_spent: u64,
    pub trades: Vec<PaperTradeRecord>,
}

pub struct PaperTrader {
    state: Arc<Mutex<PaperTradingState>>,
    config: PaperTradingConfig,
}

#[derive(Debug, Clone)]
pub struct PaperTradingConfig {
    pub starting_balance: u64,
    pub max_position_size: u64,
    pub slippage_factor: f64,
    pub failure_rate: f64, // Simulate random failures
    pub state_file: String,
}

impl Default for PaperTradingConfig {
    fn default() -> Self {
        Self {
            starting_balance: 10_000_000_000, // 10 SOL
            max_position_size: 5_000_000_000, // 5 SOL max per trade
            slippage_factor: 0.005,           // 0.5% slippage
            failure_rate: 0.05,               // 5% random failure rate
            state_file: "paper_trading_state.json".to_string(),
        }
    }
}

impl PaperTrader {
    pub fn new() -> Self {
        Self::with_config(PaperTradingConfig::default())
    }

    pub fn with_config(config: PaperTradingConfig) -> Self {
        // Load existing state or create new
        let state = if Path::new(&config.state_file).exists() {
            match Self::load_state(&config.state_file) {
                Ok(state) => state,
                Err(e) => {
                    warn!("Failed to load paper trading state: {:?}", e);
                    Self::create_new_state(&config)
                }
            }
        } else {
            Self::create_new_state(&config)
        };

        Self {
            state: Arc::new(Mutex::new(state)),
            config,
        }
    }

    fn create_new_state(config: &PaperTradingConfig) -> PaperTradingState {
        PaperTradingState {
            starting_balance: config.starting_balance,
            current_balance: config.starting_balance,
            total_trades: 0,
            successful_trades: 0,
            failed_trades: 0,
            total_profit: 0,
            total_gas_spent: 0,
            trades: Vec::new(),
        }
    }

    fn load_state(path: &str) -> Result<PaperTradingState> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let state = serde_json::from_reader(reader)?;
        Ok(state)
    }

    fn save_state(&self) -> Result<()> {
        let state = self.state.lock().unwrap();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.config.state_file)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &*state)?;
        writer.flush()?;
        Ok(())
    }

    pub async fn execute_trade(&self, swap_path: SwapPathResult) -> Result<ExecutionResult> {
        let mut state = self.state.lock().unwrap();

        // Check if we have enough balance
        let amount_in = swap_path.amount_in;
        if amount_in > self.config.max_position_size {
            return Ok(ExecutionResult {
                success: false,
                signature: None,
                error: Some("Position size exceeds maximum allowed".to_string()),
                profit: 0,
                gas_cost: 0,
            });
        }

        if amount_in > state.current_balance {
            return Ok(ExecutionResult {
                success: false,
                signature: None,
                error: Some("Insufficient balance".to_string()),
                profit: 0,
                gas_cost: 0,
            });
        }

        // Simulate market conditions
        let market_conditions = self.simulate_market_conditions();

        // Apply slippage to the expected output
        let slippage = 1.0 - (market_conditions.slippage_applied * self.config.slippage_factor);
        let actual_output =
            (swap_path.estimated_amount_out.parse::<f64>().unwrap_or(0.0) * slippage) as u64;

        // Calculate profit/loss
        let gross_profit = actual_output as i64 - amount_in as i64;
        let gas_cost = self.calculate_gas_cost(&market_conditions);
        let net_profit = gross_profit - gas_cost as i64;

        // Simulate random failures
        let success = !self.should_fail_randomly();

        let failure_reason = if !success {
            Some(self.generate_failure_reason())
        } else if net_profit < 0 {
            Some("Unprofitable after gas costs".to_string())
        } else {
            None
        };

        // Update state
        state.total_trades += 1;
        if success && net_profit > 0 {
            state.successful_trades += 1;
            state.current_balance = (state.current_balance as i64 + net_profit) as u64;
            state.total_profit += net_profit;
        } else {
            state.failed_trades += 1;
            // Deduct gas even on failure
            state.current_balance = state.current_balance.saturating_sub(gas_cost);
        }
        state.total_gas_spent += gas_cost;

        // Record the trade
        let trade_record = PaperTradeRecord {
            timestamp: Utc::now(),
            swap_path: swap_path.clone(),
            simulated_profit: gross_profit,
            simulated_gas: gas_cost,
            net_profit,
            success: success && net_profit > 0,
            failure_reason: failure_reason.clone(),
            market_conditions,
        };
        state.trades.push(trade_record);

        // Keep only last 1000 trades
        if state.trades.len() > 1000 {
            state.trades.remove(0);
        }

        drop(state); // Release lock before saving

        // Save state to disk
        let _ = self.save_state();

        // Log the result
        if success && net_profit > 0 {
            info!(
                "📝 Paper trade successful: {} -> Profit: {} SOL (after {} gas)",
                swap_path.tokens_path,
                net_profit as f64 / 1e9,
                gas_cost as f64 / 1e9
            );
        } else {
            info!(
                "📝 Paper trade failed: {} -> Reason: {}",
                swap_path.tokens_path,
                failure_reason.as_ref().unwrap_or(&"Unknown".to_string())
            );
        }

        Ok(ExecutionResult {
            success: success && net_profit > 0,
            signature: Some(format!("PAPER-{}", uuid::Uuid::new_v4())),
            error: failure_reason,
            profit: net_profit,
            gas_cost,
        })
    }

    fn simulate_market_conditions(&self) -> MarketConditions {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        MarketConditions {
            network_congested: rng.gen_bool(0.3), // 30% chance of congestion
            slippage_applied: rng.gen_range(0.1..2.0), // 0.1% to 2% slippage
            priority_fee: rng.gen_range(1_000..100_000), // Variable priority fees
        }
    }

    fn calculate_gas_cost(&self, conditions: &MarketConditions) -> u64 {
        let base_cost = 5_000; // 0.000005 SOL base
        let priority = conditions.priority_fee;
        let congestion_multiplier = if conditions.network_congested { 3 } else { 1 };

        (base_cost + priority) * congestion_multiplier
    }

    fn should_fail_randomly(&self) -> bool {
        use rand::Rng;
        rand::thread_rng().gen_bool(self.config.failure_rate)
    }

    fn generate_failure_reason(&self) -> String {
        use rand::seq::SliceRandom;
        let reasons = vec![
            "Transaction simulation failed",
            "Slippage tolerance exceeded",
            "Pool state changed",
            "Insufficient liquidity",
            "Network congestion timeout",
            "Priority fee too low",
        ];

        reasons
            .choose(&mut rand::thread_rng())
            .unwrap_or(&"Unknown error")
            .to_string()
    }

    pub fn get_statistics(&self) -> PaperTradingStatistics {
        let state = self.state.lock().unwrap();

        let win_rate = if state.total_trades > 0 {
            (state.successful_trades as f64 / state.total_trades as f64) * 100.0
        } else {
            0.0
        };

        let avg_profit_per_trade = if state.successful_trades > 0 {
            state.total_profit as f64 / state.successful_trades as f64
        } else {
            0.0
        };

        PaperTradingStatistics {
            total_trades: state.total_trades,
            successful_trades: state.successful_trades,
            failed_trades: state.failed_trades,
            win_rate,
            total_profit: state.total_profit,
            total_gas_spent: state.total_gas_spent,
            net_profit: state.total_profit - state.total_gas_spent as i64,
            current_balance: state.current_balance,
            starting_balance: state.starting_balance,
            roi: ((state.current_balance as f64 / state.starting_balance as f64) - 1.0) * 100.0,
            avg_profit_per_trade,
        }
    }

    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        *state = Self::create_new_state(&self.config);
        let _ = self.save_state();
        info!("📝 Paper trading state reset");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradingStatistics {
    pub total_trades: u64,
    pub successful_trades: u64,
    pub failed_trades: u64,
    pub win_rate: f64,
    pub total_profit: i64,
    pub total_gas_spent: u64,
    pub net_profit: i64,
    pub current_balance: u64,
    pub starting_balance: u64,
    pub roi: f64,
    pub avg_profit_per_trade: f64,
}

impl std::fmt::Display for PaperTradingStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "📊 Paper Trading Statistics")?;
        writeln!(f, "═══════════════════════════════════════")?;
        writeln!(f, "Total Trades: {}", self.total_trades)?;
        writeln!(
            f,
            "Successful: {} ({:.2}% win rate)",
            self.successful_trades, self.win_rate
        )?;
        writeln!(f, "Failed: {}", self.failed_trades)?;
        writeln!(f, "Total Profit: {:.6} SOL", self.total_profit as f64 / 1e9)?;
        writeln!(f, "Gas Spent: {:.6} SOL", self.total_gas_spent as f64 / 1e9)?;
        writeln!(f, "Net Profit: {:.6} SOL", self.net_profit as f64 / 1e9)?;
        writeln!(
            f,
            "Current Balance: {:.6} SOL",
            self.current_balance as f64 / 1e9
        )?;
        writeln!(f, "ROI: {:.2}%", self.roi)?;
        writeln!(
            f,
            "Avg Profit/Trade: {:.6} SOL",
            self.avg_profit_per_trade / 1e9
        )?;
        Ok(())
    }
}
