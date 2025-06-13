//! src/arbitrage/path_statistics.rs
//
// This module provides a high-throughput, self-tuning feedback loop for the
// PathEvaluator. It is architected to be non-blocking and persistent, ensuring
// it can handle a high volume of trade results without slowing down the core
// arbitrage logic, while also making the bot smarter over time.

use crate::arbitrage::path_evaluator::SmartPathEvaluator;
use crate::arbitrage::types::SwapPath;
use crate::markets::types::DexLabel;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

// --- Core Data Structures ---

/// The definitive outcome of a trade attempt, sent to the manager for recording.
#[derive(Debug, Clone)]
pub struct TradeOutcome {
    pub path: SwapPath,
    pub result: TradeResult,
}

/// A detailed breakdown of a trade's success or failure, based on on-chain data.
#[derive(Debug, Clone)]
pub enum TradeResult {
    Success {
        expected_amount_out: u64,
        actual_amount_out: u64,
    },
    Failure,
}

// --- Internal Statistics Structs (Serializable) ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TokenPairStats {
    success_count: u64,
    failure_count: u64,
    avg_slippage_pct: f64,
}

impl TokenPairStats {
    fn record_outcome(&mut self, is_success: bool, slippage_pct: f64) {
        const ALPHA: f64 = 0.1; // EMA weight for new observations.
        if is_success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.avg_slippage_pct = ALPHA * slippage_pct + (1.0 - ALPHA) * self.avg_slippage_pct;
    }

    fn get_success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.75 // Assume a reasonably high success rate for new pairs.
        } else {
            self.success_count as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DexStats {
    success_count: u64,
    failure_count: u64,
}

impl DexStats {
    fn record_outcome(&mut self, is_success: bool) {
        if is_success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
    }

    fn get_success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.8 // Assume a high success rate for new DEXs.
        } else {
            self.success_count as f64 / total as f64
        }
    }
}

/// The main serializable struct holding all historical statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AllPathStats {
    token_pairs: HashMap<String, TokenPairStats>,
    dexes: HashMap<DexLabel, DexStats>,
}

impl AllPathStats {
    /// Asynchronously saves the current statistics to a JSON file.
    async fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    /// Asynchronously loads statistics from a JSON file.
    async fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = tokio::fs::read_to_string(path).await?;
        let stats: Self = serde_json::from_str(&content)?;
        Ok(stats)
    }
}

// --- The Statistics Manager ---

/// Manages the collection, persistence, and application of path statistics.
pub struct PathStatisticsManager {
    outcome_sender: mpsc::Sender<TradeOutcome>,
    stats: Arc<RwLock<AllPathStats>>,
    stats_file_path: String,
}

impl PathStatisticsManager {
    /// Creates a new manager, loads existing stats, and spawns background tasks.
    pub async fn new(stats_file_path: String, save_interval_secs: u64) -> Arc<Self> {
        let (outcome_sender, mut outcome_receiver) = mpsc::channel(1000);

        let stats = match AllPathStats::load_from_file(&stats_file_path).await {
            Ok(s) => {
                info!("Loaded path statistics from {}", stats_file_path);
                s
            }
            Err(e) => {
                warn!(
                    "Failed to load path statistics from '{}': {}. Starting with fresh stats.",
                    stats_file_path, e
                );
                AllPathStats::default()
            }
        };
        let stats_arc = Arc::new(RwLock::new(stats));

        let manager = Arc::new(Self {
            outcome_sender,
            stats: Arc::clone(&stats_arc),
            stats_file_path: stats_file_path.clone(),
        });

        // Spawn the single-threaded writer to process outcomes, avoiding lock contention.
        tokio::spawn(async move {
            while let Some(outcome) = outcome_receiver.recv().await {
                let mut stats = stats_arc.write().await;
                
                let is_success = matches!(outcome.result, TradeResult::Success { .. });

                let slippage_pct = if let TradeResult::Success { expected_amount_out, actual_amount_out } = outcome.result {
                    if expected_amount_out > 0 {
                        (expected_amount_out as f64 - actual_amount_out as f64).abs() / expected_amount_out as f64 * 100.0
                    } else { 0.0 }
                } else { 100.0 };

                for route in outcome.path.paths {
                    let (token_a, token_b) = if route.token_in < route.token_out {
                        (route.token_in.clone(), route.token_out.clone())
                    } else {
                        (route.token_out.clone(), route.token_in.clone())
                    };
                    let pair_key = format!("{}-{}", token_a, token_b);
                    stats.token_pairs.entry(pair_key).or_default().record_outcome(is_success, slippage_pct);
                    stats.dexes.entry(route.dex).or_default().record_outcome(is_success);
                }
            }
        });

        // Spawn the periodic saving task.
        let manager_clone = Arc::clone(&manager);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(save_interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = manager_clone.save_stats().await {
                    error!("Failed to periodically save path statistics: {}", e);
                }
            }
        });

        manager
    }

    /// Returns a cloneable sender to record trade outcomes from any concurrent task.
    pub fn get_outcome_sender(&self) -> mpsc::Sender<TradeOutcome> {
        self.outcome_sender.clone()
    }

    /// Applies the current statistics to a PathEvaluator instance.
    pub async fn apply_to_evaluator(&self, evaluator: &mut SmartPathEvaluator) {
        let stats = self.stats.read().await;
        
        for (pair_key, pair_stats) in &stats.token_pairs {
            let tokens: Vec<&str> = pair_key.split('-').collect();
            if tokens.len() == 2 {
                evaluator.update_token_pair_success(tokens[0].to_string(), tokens[1].to_string(), pair_stats.get_success_rate());
            }
        }
        
        for (dex_label, dex_stats) in &stats.dexes {
            evaluator.update_dex_success_rate(dex_label.clone(), dex_stats.get_success_rate());
        }
    }

    /// Asynchronously triggers a save of the current statistics.
    async fn save_stats(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats_clone = self.stats.read().await.clone();
        stats_clone.save_to_file(&self.stats_file_path).await?;
        info!("Path statistics saved to {}", self.stats_file_path);
        Ok(())
    }
}
