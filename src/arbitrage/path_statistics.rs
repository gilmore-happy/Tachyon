//! src/arbitrage/path_statistics.rs
//
// This module provides a high-throughput, self-tuning feedback loop for the
// PathEvaluator. It is architected to be non-blocking and persistent, ensuring
// it can handle a high volume of trade results without slowing down the core
// arbitrage logic, while also making the bot smarter over time.

use crate::arbitrage::path_evaluator::PathEvaluator;
use crate::arbitrage::types::SwapPath;
use crate::markets::types::DexLabel;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

// --- Core Data Structures ---

/// The definitive outcome of a trade attempt, sent to the manager for recording.
/// This is the "source of truth" for all statistical analysis.
#[derive(Debug, Clone)]
pub struct TradeOutcome {
    pub path: SwapPath,
    pub result: TradeResult,
}

/// A detailed breakdown of a trade's success or failure, based on on-chain data.
#[derive(Debug, Clone)]
pub enum TradeResult {
    /// The transaction was successful.
    Success {
        /// The output amount expected from the pre-flight simulation.
        expected_amount_out: u64,
        /// The actual output amount received, parsed from the transaction logs.
        actual_amount_out: u64,
    },
    /// The transaction failed on-chain for a specific reason.
    Failure,
}

/// Aggregated statistics for a single token pair.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TokenPairStats {
    success_count: u64,
    failure_count: u64,
    /// Exponential Moving Average of slippage percentage.
    avg_slippage_pct: f64,
}

impl TokenPairStats {
    /// Updates the stats with a new trade outcome using an Exponential Moving Average (EMA).
    /// EMA gives more weight to recent trades, allowing quick adaptation.
    fn record_outcome(&mut self, is_success: bool, slippage_pct: f64) {
        const ALPHA: f64 = 0.1; // Weight for new observations.
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
            0.75 // Assume a reasonably high success rate for new, unseen pairs.
        } else {
            self.success_count as f64 / total as f64
        }
    }
}

/// Aggregated statistics for a single DEX.
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
            0.8 // Assume a high success rate for new, unseen DEXs.
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
    fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
    }
}

// --- The Statistics Manager ---

/// Manages the collection, persistence, and application of path statistics.
/// This is the public interface for the feedback loop system.
pub struct PathStatisticsManager {
    outcome_sender: mpsc::Sender<TradeOutcome>,
    stats: Arc<RwLock<AllPathStats>>,
    stats_file_path: String,
}

impl PathStatisticsManager {
    /// Creates a new manager, loads existing stats, and spawns background tasks.
    pub async fn new(stats_file_path: String, save_interval_secs: u64) -> Arc<Self> {
        let (outcome_sender, mut outcome_receiver) = mpsc::channel(1000);

        let stats = match AllPathStats::load_from_file(&stats_file_path) {
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

        // Spawn a single-threaded writer to process outcomes. This avoids lock contention.
        tokio::spawn(async move {
            while let Some(outcome) = outcome_receiver.recv().await {
                let mut stats = stats_arc.write().await;
                
                let is_success = matches!(outcome.result, TradeResult::Success { .. });

                let slippage_pct = if let TradeResult::Success { expected_amount_out, actual_amount_out } = outcome.result {
                    if expected_amount_out > 0 {
                        (expected_amount_out as f64 - actual_amount_out as f64).abs() / expected_amount_out as f64 * 100.0
                    } else { 0.0 }
                } else { 100.0 }; // Assume 100% slippage for failures to heavily penalize them.

                for route in outcome.path.paths {
                    // Update stats for the token pair
                    let (token_a, token_b) = if route.token_in < route.token_out {
                        (route.token_in.clone(), route.token_out.clone())
                    } else {
                        (route.token_out.clone(), route.token_in.clone())
                    };
                    let pair_key = format!("{}-{}", token_a, token_b);
                    stats.token_pairs.entry(pair_key).or_default().record_outcome(is_success, slippage_pct);

                    // Update stats for the DEX
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
    /// This is the primary entry point for the feedback loop.
    pub fn get_outcome_sender(&self) -> mpsc::Sender<TradeOutcome> {
        self.outcome_sender.clone()
    }

    /// Applies the current statistics to a PathEvaluator instance. This is called
    /// periodically to make the path scoring heuristics smarter.
    pub async fn apply_to_evaluator(&self, evaluator: &mut PathEvaluator) {
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

    /// Triggers a manual save of the current statistics.
    async fn save_stats(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats_clone = self.stats.read().await.clone();
        let path_clone = self.stats_file_path.clone();
        
        // Blocking file I/O is offloaded to a dedicated thread to avoid starving the async runtime.
        tokio::task::spawn_blocking(move || {
            match stats_clone.save_to_file(&path_clone) {
                Ok(_) => info!("Path statistics saved to {}", path_clone),
                Err(e) => error!("Error saving path statistics to {}: {}", path_clone, e),
            }
        }).await?;

        Ok(())
    }
}
