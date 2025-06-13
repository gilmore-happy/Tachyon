//! src/execution/risk_engine.rs

use crate::common::config::RiskConfig;
// SimulationResult import removed
use crate::arbitrage::types::ArbOpportunity; // Added for should_execute
use anyhow::Result;
use log::warn; // Added for logging

pub struct RiskEngine {
    config: RiskConfig,
    portfolio_value_usd: f64,
    daily_loss: f64, // This should be persisted or managed more robustly
}

impl RiskEngine {
    pub fn new(config: RiskConfig, initial_portfolio_value_usd: f64) -> Self {
        Self {
            config,
            portfolio_value_usd: initial_portfolio_value_usd,
            daily_loss: 0.0,
        }
    }

    // Removed validate method as it used SimulationResult and was not called.
    // should_execute is called by strategies.rs

    pub async fn should_execute(&self, opportunity: &ArbOpportunity) -> Result<bool> {
        // Basic check: Don't execute if expected profit is zero or negative (if it can be)
        if opportunity.expected_profit_lamports == 0 {
            warn!("RiskEngine: Opportunity rejected due to zero expected profit.");
            return Ok(false);
        }

        // Daily drawdown check
        // Note: self.daily_loss needs to be updated elsewhere when actual losses occur.
        // This check prevents new trades if the limit is already hit.
        let max_daily_loss_allowed_usd = self.portfolio_value_usd * self.config.max_daily_drawdown;
        if self.daily_loss >= max_daily_loss_allowed_usd {
            warn!(
                "RiskEngine: Opportunity rejected. Daily loss limit of {:.2} USD reached or exceeded. Current daily loss: {:.2} USD.",
                max_daily_loss_allowed_usd, self.daily_loss
            );
            return Ok(false);
        }

        // Token whitelist check (simplified example)
        // A real implementation would iterate through opportunity.path.paths (Vec<Route>)
        // and check each token involved against self.config.token_whitelist.
        // For now, this is a placeholder.
        if !self.config.token_whitelist.is_empty() {
            let mut all_tokens_in_path: std::collections::HashSet<String> = std::collections::HashSet::new();
            for route in &opportunity.path.paths {
                all_tokens_in_path.insert(route.token_in.clone());
                all_tokens_in_path.insert(route.token_out.clone());
            }

            for token_address in all_tokens_in_path {
                if !self.config.token_whitelist.contains(&token_address) {
                    warn!(
                        "RiskEngine: Opportunity rejected. Path involves non-whitelisted token: {}",
                        token_address
                    );
                    return Ok(false);
                }
            }
        }
        
        // Profit sanity check (e.g., profit isn't absurdly high, which might indicate an error)
        // This requires converting lamports to USD and comparing against a percentage of portfolio.
        // For simplicity, this is omitted for now but is an important check.
        // Example:
        // let profit_usd = opportunity.expected_profit_lamports as f64 / 1_000_000_000.0 * current_sol_price_usd;
        // let max_sane_profit_usd = self.portfolio_value_usd * self.config.profit_sanity_check_percentage;
        // if profit_usd > max_sane_profit_usd {
        //     warn!("RiskEngine: Opportunity rejected. Profit {} USD seems too high (sanity check).", profit_usd);
        //     return Ok(false);
        // }


        // If all checks pass:
        Ok(true)
    }

    // TODO: Add a method like `record_trade_loss(&mut self, loss_usd: f64)`
    // to be called by the executor or a monitoring component when a trade results in a loss,
    // so that `self.daily_loss` can be accurately updated.
}
