//! src/arbitrage/strategies.rs - Updated for bounded channel & TokenInfos price_usd

use crate::arbitrage::path_evaluator::SmartPathEvaluator;
use crate::arbitrage::types::{ArbOpportunity, SwapPath, SwapPathSelected, TokenInArb, TokenInfos};
use crate::common::config::{Config, STRATEGY_MASSIVE, STRATEGY_BEST_PATH};
use crate::data::market_stream::MarketEvent;
use crate::execution::risk_engine::RiskEngine;
use crate::markets::pools::{Pool, PoolRegistry};
use crate::telemetry::Metrics; // Assuming this is Arc<Metrics> from telemetry.rs
use anyhow::Result;
use log::{error, warn, debug};
use moka::future::Cache;
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::TrySendError};
use tracing::{info, info_span, Instrument};

pub struct StrategyOrchestrator {
    config: Arc<Config>,
    pool_registry: Arc<PoolRegistry>,
    token_cache: Arc<Cache<String, TokenInfos>>,
    exec_tx: mpsc::Sender<ArbOpportunity>, // Bounded sender
    market_rx: mpsc::Receiver<MarketEvent>,
    risk_engine: Arc<RiskEngine>,
    metrics: Arc<Metrics>, // This should be the Arc<Metrics> from telemetry.rs
    evaluator: SmartPathEvaluator,
}

impl StrategyOrchestrator {
    pub fn new(
        config: Arc<Config>,
        pool_registry: Arc<PoolRegistry>,
        token_cache: Arc<Cache<String, TokenInfos>>,
        exec_tx: mpsc::Sender<ArbOpportunity>, // Bounded sender
        market_rx: mpsc::Receiver<MarketEvent>,
        risk_engine: Arc<RiskEngine>,
        metrics: Arc<Metrics>, // Pass Arc<Metrics>
    ) -> Self {
        Self {
            config,
            pool_registry,
            token_cache,
            exec_tx,
            market_rx,
            risk_engine,
            metrics,
            evaluator: SmartPathEvaluator::new(),
        }
    }

    pub async fn run(mut self) -> Result<()> {
        // Process market events in background
        let market_metrics = self.metrics.clone(); // Arc clone
        let exec_tx = self.exec_tx.clone(); // Sender clone
        let risk_engine = self.risk_engine.clone();
        let evaluator = SmartPathEvaluator::new();
        
        let (_, dummy_rx) = mpsc::channel(1);
        let mut market_rx = std::mem::replace(&mut self.market_rx, dummy_rx);
        
        let market_events_handle = tokio::spawn(
            async move {
                while let Some(event) = market_rx.recv().await {
                    market_metrics.inc_opportunities_discovered(); // Use helper method
                    debug!("Received market event: {:?}", event);
                    
                    if let Some(opportunity) = StrategyOrchestrator::process_market_event(event, &evaluator).await {
                        match risk_engine.should_execute(&opportunity).await {
                            Ok(true) => {
                                info!("✅ Risk check passed for opportunity: {} lamports profit", 
                                    opportunity.expected_profit_lamports);
                                
                                match exec_tx.try_send(opportunity) {
                                    Ok(()) => {
                                        market_metrics.inc_opportunities_sent(); // Use helper method
                                    }
                                    Err(TrySendError::Full(_)) => {
                                        warn!("Execution queue full, dropping opportunity from market event processor");
                                        market_metrics.inc_opportunities_dropped(); // Use helper method
                                    }
                                    Err(TrySendError::Closed(_)) => {
                                        error!("Execution channel closed from market event processor");
                                        break;
                                    }
                                }
                            }
                            Ok(false) => {
                                warn!("❌ Risk check failed for opportunity from market event");
                                market_metrics.inc_opportunities_rejected(); // Use helper method
                            }
                            Err(e) => {
                                error!("Risk check error from market event: {}", e);
                            }
                        }
                    }
                }
            }
            .instrument(info_span!("market_event_processor")),
        );

        if self.config.contains_strategy(STRATEGY_MASSIVE) {
            if let Err(e) = self.run_massive().await {
                error!("Massive strategy failed: {}", e);
            }
        }
        
        if self.config.contains_strategy(STRATEGY_BEST_PATH) {
            if let Err(e) = self.run_best_path().await {
                error!("Best path strategy failed: {}", e);
            }
        }
        
        market_events_handle.abort();
        info!("🏁 All strategies completed");
        Ok(())
    }

    async fn run_massive(&self) -> Result<()> {
        info!("🚀 Launching MASSIVE strategy");
        let pools = self.pool_registry.get_pools(false).await?;

        for input_config in &self.config.massive_strategy_inputs {
            for token_config in &input_config.tokens_to_arb {
                if !self.token_cache.contains_key(&token_config.address) { 
                    let token_info = TokenInfos {
                        address: token_config.address.clone(),
                        symbol: token_config.symbol.clone(),
                        decimals: token_config.decimals,
                        // price_usd is no longer part of TokenInfos as per the new types.rs
                        // Price information for execution will be part of the ArbOpportunity.execution_plan
                    };
                    self.token_cache.insert(token_config.address.clone(), token_info).await;
                    debug!("📝 Cached token info for {}", token_config.symbol);
                }
            }

            let tokens_to_arb: Vec<TokenInArb> = input_config
                .tokens_to_arb
                .iter()
                .map(|tc| TokenInArb::from(tc))
                .collect();

            let paths = self.find_arbitrage_paths(&tokens_to_arb, &pools).await?;
            info!("📊 Found {} potential arbitrage paths in MASSIVE strategy", paths.len());

            for path in paths {
                if !self.validate_path_tokens(&path).await {
                    warn!("Skipping path with invalid tokens in MASSIVE strategy");
                    continue;
                }
                
                match self.evaluator.evaluate(&path) {
                    Ok(Some(opportunity)) => {
                        self.metrics.inc_opportunities_discovered(); // Use helper method
                        
                        match self.risk_engine.should_execute(&opportunity).await {
                            Ok(true) => {
                                info!("✅ Sending opportunity from MASSIVE: {} lamports profit", 
                                    opportunity.expected_profit_lamports);
                                
                                match self.exec_tx.try_send(opportunity) {
                                    Ok(()) => {
                                        self.metrics.inc_opportunities_sent(); // Use helper method
                                    }
                                    Err(TrySendError::Full(_)) => {
                                        warn!("Execution queue full in MASSIVE strategy, dropping opportunity");
                                        self.metrics.inc_opportunities_dropped(); // Use helper method
                                    }
                                    Err(TrySendError::Closed(_)) => {
                                        error!("Execution channel closed in MASSIVE strategy");
                                        return Err(anyhow::anyhow!("Execution channel closed"));
                                    }
                                }
                            }
                            Ok(false) => {
                                self.metrics.inc_opportunities_rejected(); // Use helper method
                                warn!("❌ Opportunity rejected by risk engine in MASSIVE strategy");
                            }
                            Err(e) => {
                                error!("Risk engine error in MASSIVE strategy: {}", e);
                            }
                        }
                    }
                    Ok(None) => {
                        debug!("Path not profitable after evaluation in MASSIVE strategy");
                    }
                    Err(e) => {
                        error!("Path evaluation error in MASSIVE strategy: {}", e);
                    }
                }
            }
        }

        info!("✅ MASSIVE strategy cycle completed");
        Ok(())
    }

    async fn run_best_path(&self) -> Result<()> {
        info!("🚀 Launching BEST_PATH strategy");
        
        let cached_tokens: Vec<TokenInArb> = self.token_cache
            .iter()
            .map(|(_, token_info)| TokenInArb {
                token: token_info.address.clone(),
                symbol: token_info.symbol.clone(),
                decimals: token_info.decimals,
            })
            .collect();
        
        if cached_tokens.len() < 2 {
            warn!("Not enough tokens cached for BEST_PATH strategy");
            return Ok(());
        }
        
        let pools = self.pool_registry.get_pools(true).await?;
        let mut best_opportunity: Option<ArbOpportunity> = None;
        let mut current_best_profit = 0u64; // Renamed to avoid conflict if ArbOpportunity had 'best_profit'
        
        for i in 0..cached_tokens.len() {
            for j in i+1..cached_tokens.len() {
                let pair = vec![cached_tokens[i].clone(), cached_tokens[j].clone()];
                let paths = self.find_arbitrage_paths(&pair, &pools).await?;
                
                for path in paths {
                    if let Ok(Some(opportunity)) = self.evaluator.evaluate(&path) {
                        if opportunity.expected_profit_lamports > current_best_profit {
                            current_best_profit = opportunity.expected_profit_lamports;
                            best_opportunity = Some(opportunity);
                        }
                    }
                }
            }
        }
        
        if let Some(opportunity) = best_opportunity {
            info!("🎯 Found best path with {} lamports profit", current_best_profit);
            self.metrics.inc_opportunities_discovered(); // Discovered the best one

            if self.risk_engine.should_execute(&opportunity).await? {
                match self.exec_tx.try_send(opportunity) {
                    Ok(()) => {
                        self.metrics.inc_opportunities_sent(); // Use helper method
                    }
                    Err(TrySendError::Full(_)) => {
                        warn!("Execution queue full, dropping best opportunity from BEST_PATH strategy");
                        self.metrics.inc_opportunities_dropped(); // Use helper method
                    }
                    Err(TrySendError::Closed(_)) => {
                        error!("Execution channel closed in BEST_PATH strategy");
                        return Err(anyhow::anyhow!("Execution channel closed"));
                    }
                }
            } else {
                self.metrics.inc_opportunities_rejected(); // Use helper method
                warn!("❌ Best path opportunity rejected by risk engine");
            }
        } else {
            info!("No profitable paths found in BEST_PATH strategy");
        }
        
        Ok(())
    }
    
    async fn find_arbitrage_paths(&self, tokens: &[TokenInArb], pools: &[Pool]) -> Result<Vec<SwapPathSelected>> {
        if tokens.len() < 2 { return Ok(vec![]); }
        let mut paths = Vec::new();
        for i in 0..tokens.len() {
            for j in i+1..tokens.len() {
                let token_a = &tokens[i];
                let token_b = &tokens[j];
                let connecting_pools: Vec<&Pool> = pools.iter()
                    .filter(|pool| (pool.token_a == token_a.token && pool.token_b == token_b.token) || (pool.token_a == token_b.token && pool.token_b == token_a.token))
                    .collect();
                if connecting_pools.len() >= 2 { // Placeholder: needs real path finding
                    paths.push(SwapPathSelected {
                        path: SwapPath { id_paths: vec![1, 2], hops: 2, paths: vec![] },
                        expected_profit_usd: 0.0, markets: vec![],
                    });
                }
            }
        }
        Ok(paths)
    }
    
    async fn validate_path_tokens(&self, path: &SwapPathSelected) -> bool {
        if path.expected_profit_usd < 0.0 {
            warn!("Path has negative expected_profit_usd ({}) during token validation step.", path.expected_profit_usd);
            return false;
        }
        // TODO: Actual token validation using self.token_cache and config whitelist
        true
    }
    
    async fn process_market_event(event: MarketEvent, _evaluator: &SmartPathEvaluator) -> Option<ArbOpportunity> {
        // event is a struct: MarketEvent { token_pair, price, source }
        // The previous logic assumed an enum variant MarketEvent::PriceUpdate { pool_id, price_change, .. }
        // which is incorrect based on the actual MarketEvent struct definition.
        //
        // This function needs to be implemented with logic that translates a MarketEvent
        // (e.g., a new price for a token_pair) into a potential ArbOpportunity.
        // This might involve:
        // 1. Identifying related pools from self.pool_registry.
        // 2. Comparing the new event.price with existing prices in pools to find discrepancies.
        // 3. Constructing a SwapPath if an arbitrage is detected.
        //
        // For now, providing a very basic placeholder to fix the compile error.
        // This placeholder creates an opportunity if the price is non-zero, which is not realistic.
        
        // Example placeholder:
        if event.price > 0.0 { // This condition is arbitrary and needs real logic
            // info!("Processing MarketEvent for {}: price {} from {}", event.token_pair, event.price, event.source);
            
            // Creating a highly simplified ArbOpportunity.
            // Real logic would need to construct a valid SwapPath based on the event.
            // We don't have pool_id or a direct price_change from the current MarketEvent struct.
            // The id_paths would typically come from identified pools in an arbitrage route.
            let placeholder_path = SwapPath {
                id_paths: vec![0], // Placeholder ID, e.g., representing an abstract market event
                hops: 1,           // Placeholder
                paths: vec![],     // Placeholder, actual Route objects would be here
            };

            // Placeholder profit calculation. This is very arbitrary.
            // A real calculation would depend on the arbitrage path found and amounts.
            // E.g., if event.price is for SOL/USDC, and it's $150,
            // this calculates 1% of 1 SOL notional value in lamports.
            let placeholder_profit_lamports = (event.price * 0.01 * 1_000_000_000.0) as u64; 

            if placeholder_profit_lamports > 0 {
                let now_nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                return Some(ArbOpportunity {
                    path: placeholder_path,
                    expected_profit_lamports: placeholder_profit_lamports,
                    timestamp_unix_nanos: now_nanos,
                    execution_plan: vec![], // Added
                    metadata: crate::arbitrage::types::OpportunityMetadata { // Added
                        estimated_gas_cost: 0,
                        net_profit_lamports: placeholder_profit_lamports as i64,
                        profit_percentage_bps: 100, // 1%
                        risk_score: 0,
                        source: crate::arbitrage::types::OpportunitySource::MarketEvent { 
                            pool_id: 0, // Placeholder
                            event_type: "price_update".to_string() 
                        },
                        max_latency_ms: 500,
                    },
                });
            }
        }
        None
    }
}
