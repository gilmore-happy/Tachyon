use crate::common::constants::Env;
use crate::fees::{
    fee_cache::FeeCache,
    fee_strategies::{
        determine_urgency, AggressiveStrategy, ConservativeStrategy, FeeStrategy,
        ProfitBasedStrategy, Urgency,
    },
};
use anyhow::{anyhow, Context, Result};
use log::info;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use std::sync::Arc;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub enum FeeMode {
    Conservative,
    ProfitBased,
    Aggressive,
}

#[derive(Debug, Clone)]
pub struct PriorityFeeConfig {
    pub mode: FeeMode,
    pub cache_duration_secs: u64,
    pub custom_strategy: Option<Arc<dyn FeeStrategy>>,
}

impl Default for PriorityFeeConfig {
    fn default() -> Self {
        Self {
            mode: FeeMode::ProfitBased,
            cache_duration_secs: 2,
            custom_strategy: None,
        }
    }
}

pub struct PriorityFeeService {
    fee_cache: Arc<FeeCache>,
    strategy: Arc<dyn FeeStrategy>,
    config: PriorityFeeConfig,
}

impl PriorityFeeService {
    /// Create a new priority fee service
    pub fn new(rpc_client: Arc<RpcClient>, config: PriorityFeeConfig) -> Self {
        let fee_cache = Arc::new(FeeCache::new(rpc_client, config.cache_duration_secs));

        // Start background refresh task
        fee_cache.clone().start_background_refresh();

        // Select strategy based on config
        let strategy: Arc<dyn FeeStrategy> = if let Some(custom) = &config.custom_strategy {
            custom.clone()
        } else {
            match config.mode {
                FeeMode::Conservative => Arc::new(ConservativeStrategy::default()),
                FeeMode::ProfitBased => Arc::new(ProfitBasedStrategy::default()),
                FeeMode::Aggressive => Arc::new(AggressiveStrategy::default()),
            }
        };

        info!(
            "🚀 Priority fee service initialized with {} strategy",
            strategy.name()
        );

        Self {
            fee_cache,
            strategy,
            config,
        }
    }

    /// Create service from environment configuration
    pub fn from_env() -> Result<Self> {
        let env = Env::new();
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            env.rpc_url_tx.clone(),
            CommitmentConfig::processed(),
        ));

        // Determine mode from env or default to ProfitBased
        let mode = match std::env::var("FEE_STRATEGY").as_deref() {
            Ok("conservative") => FeeMode::Conservative,
            Ok("aggressive") => FeeMode::Aggressive,
            _ => FeeMode::ProfitBased,
        };

        let config = PriorityFeeConfig {
            mode,
            cache_duration_secs: std::env::var("FEE_CACHE_DURATION")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            custom_strategy: None,
        };

        Ok(Self::new(rpc_client, config))
    }

    /// Get priority fee for a given profit amount
    pub async fn get_priority_fee(&self, profit_lamports: u64) -> Result<u64> {
        let urgency = determine_urgency(profit_lamports);
        self.get_priority_fee_with_urgency(profit_lamports, urgency)
            .await
    }

    /// Get priority fee with explicit urgency
    pub async fn get_priority_fee_with_urgency(
        &self,
        profit_lamports: u64,
        urgency: Urgency,
    ) -> Result<u64> {
        let fee_data = self
            .fee_cache
            .get_fee_data()
            .await
            .context("Failed to get fee data")?;

        let fee = self
            .strategy
            .calculate_fee(profit_lamports, &fee_data, urgency);

        info!(
            "💸 Priority fee calculated: {} microlamports ({:.6} SOL) for {:.3} SOL profit",
            fee,
            fee as f64 / 1e9,
            profit_lamports as f64 / 1e9
        );

        Ok(fee)
    }

    /// Get base fee without profit consideration
    pub async fn get_base_fee(&self) -> Result<u64> {
        let fee_data = self.fee_cache.get_fee_data().await?;
        Ok(fee_data.percentile_75)
    }

    /// Force refresh fee cache
    pub async fn refresh_cache(&self) -> Result<()> {
        self.fee_cache.refresh_cache().await?;
        Ok(())
    }

    /// Get current fee statistics
    pub async fn get_fee_stats(&self) -> Result<FeeStats> {
        let fee_data = self.fee_cache.get_fee_data().await?;

        Ok(FeeStats {
            base_fee: fee_data.base_fee,
            percentile_75: fee_data.percentile_75,
            percentile_90: fee_data.percentile_90,
            percentile_95: fee_data.percentile_95,
            max_recent: fee_data.max_recent_fee,
            strategy_name: self.strategy.name().to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FeeStats {
    pub base_fee: u64,
    pub percentile_75: u64,
    pub percentile_90: u64,
    pub percentile_95: u64,
    pub max_recent: u64,
    pub strategy_name: String,
}

// Replace the dangerous mutable static with safe OnceLock
static GLOBAL_FEE_SERVICE: OnceLock<Arc<PriorityFeeService>> = OnceLock::new();

pub fn init_global_fee_service(
    rpc_client: Arc<RpcClient>,
    config: PriorityFeeConfig,
) -> Result<()> {
    let service = Arc::new(PriorityFeeService::new(rpc_client, config));
    GLOBAL_FEE_SERVICE.set(service)
        .map_err(|_| anyhow!("Global fee service already initialized"))?;
    Ok(())
}

pub fn get_global_fee_service() -> Result<Arc<PriorityFeeService>> {
    GLOBAL_FEE_SERVICE.get()
        .cloned()
        .ok_or_else(|| anyhow!("Global fee service not initialized"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_priority_fee_service() {
        // Test implementation would go here
    }
}
