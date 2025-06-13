//! src/main.rs

use anyhow::Result;
use moka::future::Cache;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::bs58; // Ensure bs58 is in Cargo.toml
use std::sync::Arc;
use std::sync::atomic::Ordering; // Added for fetch_add
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, info_span, Instrument};
use log::warn; // Added for warn! macro

use crate::arbitrage::strategies::StrategyOrchestrator;
use crate::arbitrage::types::{ArbOpportunity, TokenInfos}; // Removed SwapPathSelected
use crate::common::config::{Config, EXECUTION_MODE_SIMULATE}; // Added EXECUTION_MODE_SIMULATE
use crate::data::market_stream::init_market_data;
use crate::execution::executor::TransactionExecutor;
use crate::execution::risk_engine::RiskEngine;
use crate::fees::priority_fees::{init_global_fee_service, PriorityFeeConfig, FeeMode}; // Added fee imports
use crate::markets::pools::PoolRegistry;
use crate::telemetry::init_telemetry;


// Module declarations
mod arbitrage;
mod common;
mod data;
mod execution;
mod fees; // Ensured
mod markets;
mod telemetry;
mod transactions; // Ensured

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry (tracing + Prometheus)
    let metrics = init_telemetry();
    let main_span = info_span!("main");
    let _main_span_guard = main_span.enter();

    info!("🚀 Bot starting up...");

    // Load configuration
    let config = Arc::new(Config::load().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?);
    info!("✅ Configuration loaded.");

    // Load keypair securely from Vault
    let keypair = Arc::new(load_keypair(&config).await?);
    info!("✅ Keypair loaded securely for wallet: {}", keypair.pubkey());

    // Initialize concurrent state caches
    let token_cache: Arc<Cache<String, TokenInfos>> = Arc::new(
        Cache::builder()
            .time_to_live(Duration::from_secs(3600))
            .build(),
    );
    let pool_registry = Arc::new(PoolRegistry::new(&config).await?);
    metrics.pools_loaded.fetch_add(pool_registry.len() as u64, Ordering::Relaxed); // Changed to fetch_add
    info!("✅ Caches and pool registry initialized with {} pools.", pool_registry.len());

    // Initialize RPC client (shared for fee service, executor will take URL string)
    let rpc_url = config.rpc_url.clone().ok_or_else(|| anyhow::anyhow!("rpc_url not configured in config.json"))?;
    let wss_rpc_url = config.wss_rpc_url.clone().ok_or_else(|| anyhow::anyhow!("wss_rpc_url not configured in config.json"))?;
    
    let rpc_client_for_fees = Arc::new(RpcClient::new_with_commitment(
        rpc_url.clone(),
        solana_sdk::commitment_config::CommitmentConfig::confirmed(),
    ));
    info!("✅ RPC client for Fee Service initialized using URL: {}", rpc_url);

    // Initialize priority fee service globally (used by advanced executor)
    let fee_config_mode = match config.execution_mode.as_str() {
        "Live" => FeeMode::Aggressive,
        "Paper" => FeeMode::ProfitBased,
        _ => FeeMode::Conservative,
    };
    let fee_service_config = PriorityFeeConfig {
        mode: fee_config_mode,
        cache_duration_secs: config.fee_cache_duration_secs.unwrap_or(2), // Assumes fee_cache_duration_secs in Config
        custom_strategy: None,
    };
    init_global_fee_service(rpc_client_for_fees, fee_service_config)?;
    info!("✅ Global Priority Fee Service initialized.");

    // Channel for StrategyOrchestrator to send ArbOpportunity to TransactionExecutor
    let (exec_tx_for_orchestrator, exec_rx_for_executor) = mpsc::channel::<ArbOpportunity>(
        config.executor_queue_size.unwrap_or(100) // Bounded channel
    );
    
    // Initialize risk engine
    let risk_engine = Arc::new(RiskEngine::new(
        config.risk_management.clone(),
        config.risk_management.initial_portfolio_value_usd.unwrap_or(10000.0),
    ));
    info!("✅ Risk engine initialized.");
    
    // Initialize and spawn "advanced" transaction executor
    let executor = TransactionExecutor::new(
        keypair.clone(), // Arc<Keypair>
        rpc_url.clone(), // String
        wss_rpc_url.clone(), // String
        exec_rx_for_executor, // Receiver<ArbOpportunity>
        config.clone(),   // Arc<Config>
        Arc::new(crate::execution::executor::Metrics::new()),  // Changed to use executor::Metrics
    ).await?;
    let executor_handle = tokio::spawn( // Capture handle
        async move {
            executor.run().await; // Advanced executor.run() is `async fn run(mut self)`
        }
        .instrument(info_span!("transaction_executor")),
    );
    info!("✅ Advanced Transaction Executor started in {} mode.", config.execution_mode);

    // Initialize market data pipeline
    let market_rx = init_market_data(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize market data: {}", e))?;
    info!("✅ Market data pipeline initialized.");
    
    // Initialize strategy orchestrator
    let orchestrator = StrategyOrchestrator::new(
        config.clone(),
        pool_registry,
        token_cache,
        exec_tx_for_orchestrator, // Pass the bounded sender for ArbOpportunity
        market_rx,
        risk_engine,
        metrics.clone(),
    );
    info!("✅ Strategy orchestrator initialized.");

    // Log active strategies
    info!("📋 Active strategies: {:?}", config.active_strategies);

    // Run strategies
    let orchestrator_handle = tokio::spawn( // Capture handle
        async move {
            if let Err(e) = orchestrator.run().await {
                error!("Strategy orchestrator failed: {}", e);
            }
        }
        .instrument(info_span!("strategy_orchestrator")),
    );

    // Set up graceful shutdown
    let shutdown_signal = tokio::signal::ctrl_c();
    
    tokio::select! {
        _ = shutdown_signal => {
            info!("🛑 Shutdown signal received");
            // Optionally, could abort tasks here if they don't handle shutdown internally
            // orchestrator_handle.abort();
            // converter_handle.abort();
            // executor_handle.abort();
        }
        res = orchestrator_handle => {
            info!("🏁 Orchestrator completed: {:?}", res);
        }
        // Converter_handle removed
        res = executor_handle => {
            error!("❌ Executor terminated unexpectedly: {:?}", res);
        }
    }

    info!("👋 MEV Bot shutting down gracefully");
    Ok(())
}

async fn load_keypair(config: &Config) -> Result<Keypair> {
    // Check for keypair in environment first (Base58 encoded string)
    if let Ok(keypair_bs58) = std::env::var("SOLANA_KEYPAIR") {
        info!("Loading keypair from SOLANA_KEYPAIR environment variable.");
        let keypair_bytes = bs58::decode(&keypair_bs58)
            .into_vec()
            .map_err(|e| anyhow::anyhow!("Failed to decode base58 keypair from SOLANA_KEYPAIR: {}", e))?;
        
        return Keypair::from_bytes(&keypair_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to create keypair from SOLANA_KEYPAIR bytes: {}", e));
    }
    
    // Check for keypair file path in environment
    if let Ok(keypair_path) = std::env::var("SOLANA_KEYPAIR_PATH") {
        info!("Loading keypair from file specified by SOLANA_KEYPAIR_PATH: {}", keypair_path);
        let keypair_bytes = std::fs::read(&keypair_path)
            .map_err(|e| anyhow::anyhow!("Failed to read keypair file from {}: {}", keypair_path, e))?;
        
        return Keypair::from_bytes(&keypair_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to create keypair from file bytes ({}): {}", keypair_path, e));
    }
    
    // For development/testing in Simulate mode - generate random keypair if no ENV vars found
    if config.execution_mode == EXECUTION_MODE_SIMULATE {
        warn!("⚠️  No keypair found via SOLANA_KEYPAIR or SOLANA_KEYPAIR_PATH.");
        warn!("⚠️  Generating random keypair for SIMULATION mode.");
        return Ok(Keypair::new());
    }
    
    // In non-Simulate modes (Live, Paper), require an explicit keypair via ENV vars.
    Err(anyhow::anyhow!(
        "CRITICAL: No keypair found for {} mode. \
        Set SOLANA_KEYPAIR (base58 string) or SOLANA_KEYPAIR_PATH (file path) environment variable.",
        config.execution_mode
    ))
    
    // Production Vault integration (example, commented out)
    /*
    info!("Attempting to load keypair from Vault...");
    let vault_url = config.vault_url.clone()
        .ok_or_else(|| anyhow::anyhow!("Vault URL (vault_url) not configured in config.json"))?;
    let vault_token = std::env::var("VAULT_TOKEN")
        .map_err(|_| anyhow::anyhow!("VAULT_TOKEN environment variable not set for Vault access"))?;
    
    // Assuming vaultrs crate or similar
    // let settings = vaultrs::client::VaultClientSettingsBuilder::default()
    //     .address(vault_url)
    //     .token(vault_token)
    //     .build()?;
    // let client = vaultrs::client::VaultClient::new(settings)?;
    
    // Example: Read a secret from kv2 engine, path "secret/data/solana/mev_bot_keypair", field "value"
    // let secret_path = "secret/data/solana/mev_bot_keypair";
    // let secret_data = vaultrs::kv2::read(&client, secret_path).await
    //     .map_err(|e| anyhow::anyhow!("Failed to read secret from Vault at {}: {}", secret_path, e))?;
    
    // let keypair_bs58 = secret_data
    //     .get("value") // Assuming the keypair string is stored in a field named "value"
    //     .and_then(|v| v.as_str())
    //     .ok_or_else(|| anyhow::anyhow!("Keypair 'value' field not found or not a string in Vault secret at {}", secret_path))?;
    
    // info!("Successfully retrieved keypair string from Vault.");
    // let keypair_bytes = bs58::decode(keypair_bs58)
    //     .into_vec()
    //     .map_err(|e| anyhow::anyhow!("Failed to decode base58 keypair from Vault: {}", e))?;
        
    // Keypair::from_bytes(&keypair_bytes)
    //     .map_err(|e| anyhow::anyhow!("Failed to create keypair from Vault bytes: {}", e))
    */
}
