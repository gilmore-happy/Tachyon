//! src/arbitrage/path_evaluator.rs

use crate::arbitrage::types::{ArbOpportunity, SwapPathSelected};
use anyhow::Result;

#[derive(Clone)]
pub struct SmartPathEvaluator {
    slippage_factor: f64,
    token_pair_success_rates: std::collections::HashMap<String, f64>,
    dex_success_rates: std::collections::HashMap<crate::markets::types::DexLabel, f64>,
}

impl SmartPathEvaluator {
    pub fn new() -> Self {
        Self {
            slippage_factor: 0.005, // 0.5% default slippage
            token_pair_success_rates: std::collections::HashMap::new(),
            dex_success_rates: std::collections::HashMap::new(),
        }
    }

    /// Updates the success rate for a token pair
    pub fn update_token_pair_success(&mut self, token_a: String, token_b: String, success_rate: f64) {
        let pair_key = if token_a < token_b {
            format!("{}-{}", token_a, token_b)
        } else {
            format!("{}-{}", token_b, token_a)
        };
        self.token_pair_success_rates.insert(pair_key, success_rate);
    }

    /// Updates the success rate for a DEX
    pub fn update_dex_success_rate(&mut self, dex_label: crate::markets::types::DexLabel, success_rate: f64) {
        self.dex_success_rates.insert(dex_label, success_rate);
    }

    /// Evaluates a potential arbitrage path for profitability and safety.
    pub fn evaluate(&self, path_selected: &SwapPathSelected) -> Result<Option<ArbOpportunity>> {
        // Use expected_profit_usd instead of result
        let base_profit_usd = path_selected.expected_profit_usd;
        if base_profit_usd <= 0.0 {
            return Ok(None);
        }

        // Apply a slippage model to get a more realistic profit estimate
        let adjusted_profit_usd = base_profit_usd * (1.0 - self.slippage_factor);

        // No empty path check
        if path_selected.path.paths.is_empty() {
             return Ok(None); // Cannot validate an empty path
        }

        // Convert the final USD profit to lamports for the priority queue
        // (Assuming 1B lamports per dollar for this calculation, should be refined with SOL price)
        let profit_lamports = (adjusted_profit_usd * 1_000_000_000.0) as u64;

        // Placeholder for execution_plan and metadata
        // In a real scenario, these would be derived from path_selected.path and other factors
        let execution_plan = vec![]; // TODO: Populate this based on path_selected.path.paths
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let metadata = crate::arbitrage::types::OpportunityMetadata {
            estimated_gas_cost: 5000, // Placeholder
            net_profit_lamports: profit_lamports.saturating_sub(5000) as i64,
            profit_percentage_bps: if path_selected.expected_profit_usd > 0.0 { // Avoid division by zero if initial amount is tied to profit
                (profit_lamports as f64 / (path_selected.expected_profit_usd * 1_000_000_000.0) * 10000.0) as u16
            } else {
                0
            },
            risk_score: 10, // Placeholder
            source: crate::arbitrage::types::OpportunitySource::StrategyScan { strategy_name: "SmartPathEvaluator".to_string() },
            max_latency_ms: 500, // Placeholder
        };

        Ok(Some(ArbOpportunity {
            path: path_selected.path.clone(),
            expected_profit_lamports: profit_lamports,
            timestamp_unix_nanos: now_nanos,
            execution_plan,
            metadata,
        }))
    }
}
