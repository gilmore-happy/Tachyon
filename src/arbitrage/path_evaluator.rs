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
    
    /// Evaluate arbitrage potential based on historical success rates and market conditions
    pub fn evaluate_arbitrage_potential(&self, pair_key: &str, price: f64, source: &str) -> f64 {
        // Base score starts at 0.5 (neutral)
        let mut score = 0.5;
        
        // Adjust score based on historical token pair success rate
        if let Some(&success_rate) = self.token_pair_success_rates.get(pair_key) {
            score += (success_rate - 0.5) * 0.3; // +/- 0.15 based on historical performance
        }
        
        // Adjust score based on DEX source reliability
        if let Ok(dex_label) = source.parse::<crate::markets::types::DexLabel>() {
            if let Some(&dex_success_rate) = self.dex_success_rates.get(&dex_label) {
                score += (dex_success_rate - 0.5) * 0.2; // +/- 0.10 based on DEX performance
            }
        }
        
        // Adjust score based on price volatility (higher volatility = better arbitrage potential)
        let price_volatility_bonus = if price > 0.0 {
            // Calculate volatility based on price deviation from typical ranges
            let volatility = match pair_key {
                s if s.contains("SOL") => {
                    let sol_base = 200.0;
                    ((price - sol_base).abs() / sol_base).min(0.1) // Cap at 10% deviation
                },
                s if s.contains("USDC") => {
                    let usdc_base = 1.0;
                    ((price - usdc_base).abs() / usdc_base).min(0.05) // Cap at 5% deviation
                },
                _ => 0.01 // Default small bonus for other pairs
            };
            volatility * 2.0 // Convert to bonus (max +0.2 for SOL, +0.1 for USDC)
        } else {
            0.0
        };
        
        score += price_volatility_bonus;
        
        // Ensure score stays within [0.0, 1.0] bounds
        score.max(0.0).min(1.0)
    }
}
