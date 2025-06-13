//! src/execution/executor.rs - FIXED VERSION
//! This implements actual transaction execution logic

use crate::arbitrage::types::ArbOpportunity; // Keep Route if opportunity.path.paths is used, not needed for execution_plan
use crate::common::config::Config;
use crate::execution::paper_trading::PaperTrader;
use crate::fees::priority_fees::{get_global_fee_service, PriorityFeeService};
// Removed create_swap_instructions import
use crate::markets::types::DexLabel; // Added for new build_swap_instructions
use crate::transactions::{ // Added for new build_swap_instructions
    create_transaction::InstructionDetails,
    meteoradlmm_swap::{construct_meteora_instructions, SwapParametersMeteora},
    raydium_swap::{construct_raydium_instructions, SwapParametersRaydium},
    orca_whirpools_swap::{construct_orca_whirpools_instructions, SwapParametersOrcaWhirpools},
    // TODO: Add imports for RaydiumClmm and Orca swap constructors when they are implemented
};
use anyhow::Result;
use log::{error, info, warn};
use priority_queue::PriorityQueue;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::tpu_client::{TpuClient, TpuClientConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::cmp::Reverse;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::time::{sleep, Instant};
use uuid::Uuid;

// Basic Metrics Structure (Ideally in its own module: src/common/metrics.rs)
#[derive(Debug)]
pub struct Metrics {
    pub execution_attempts: AtomicU64,
    pub execution_successes: AtomicU64,
    pub execution_failures: AtomicU64,
    pub total_profit_lamports: AtomicI64, // Use I64 for profit as it can be negative
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            execution_attempts: AtomicU64::new(0),
            execution_successes: AtomicU64::new(0),
            execution_failures: AtomicU64::new(0),
            total_profit_lamports: AtomicI64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub signature: Option<String>,
    pub error: Option<String>,
    pub profit_lamports: i64,
    pub gas_cost: u64,
    pub execution_time_ms: u64,
}

// SimulationResult might be useful if detailed simulation outcomes are needed elsewhere.
// For now, execute_simulation directly populates ExecutionResult.
// #[derive(Debug, Clone)]
// pub struct SimulationResult {
//     pub success: bool,
//     pub expected_profit: f64,
//     pub error: Option<String>,
//     pub priority_fee_lamports: u64,
//     pub compute_units_consumed: u64,
// }

pub struct TransactionExecutor {
    keypair: Arc<Keypair>,
    rpc_client: Arc<RpcClient>,
    tpu_client: Arc<TpuClient<solana_quic_client::QuicPool, solana_quic_client::QuicConnectionManager, solana_quic_client::QuicConfig>>,
    execution_queue: Receiver<ArbOpportunity>,
    fee_service: Arc<PriorityFeeService>,
    execution_mode: String,
    paper_trader: Option<PaperTrader>,
    priority_queue: PriorityQueue<ArbOpportunity, Reverse<u64>>,
    config: Arc<Config>,
    metrics: Arc<Metrics>,
}

impl TransactionExecutor {
    pub async fn new(
        keypair: Arc<Keypair>,
        rpc_url: String,
        wss_url: String,
        rx: Receiver<ArbOpportunity>,
        config: Arc<Config>,
        metrics: Arc<Metrics>, // Pass in metrics
    ) -> Result<Self> {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));
        
        let blocking_rpc_client_for_tpu = Arc::new(solana_client::rpc_client::RpcClient::new(rpc_url));
        
        let tpu_client = match TpuClient::new(
            blocking_rpc_client_for_tpu,
            &wss_url,
            TpuClientConfig::default(),
        ) {
            Ok(client) => Arc::new(client),
            Err(e) => return Err(anyhow::anyhow!("Failed to create TPU client: {}", e)),
        };
        
        let fee_service = get_global_fee_service()
            .map_err(|e| anyhow::anyhow!("Failed to get fee service: {}", e))?;
        
        let paper_trader = if config.execution_mode == "Paper" {
            Some(PaperTrader::new().await)
        } else {
            None
        };
        
        Ok(Self {
            keypair,
            rpc_client,
            tpu_client,
            execution_queue: rx,
            fee_service,
            execution_mode: config.execution_mode.clone(),
            paper_trader,
            priority_queue: PriorityQueue::new(),
            config,
            metrics,
        })
    }
    
    pub async fn run(mut self) {
        info!("🚀 Transaction Executor started in {} mode", self.execution_mode);

        loop {
            tokio::select! {
                biased;

                _ = async {}, if !self.priority_queue.is_empty() => {
                    if let Some((opportunity, _priority)) = self.priority_queue.pop() {
                        if let Err(e) = self.process_opportunity_internal(opportunity).await {
                             error!("Failed to process opportunity: {}", e);
                        }
                    }
                },

                maybe_opportunity = self.execution_queue.recv() => {
                    match maybe_opportunity {
                        Some(opportunity) => {
                            let priority = Reverse(opportunity.expected_profit_lamports);
                            let max_queue_size = self.config.max_queue_size.unwrap_or(1000);
                            let new_opportunity_profit = opportunity.expected_profit_lamports; // For logging after move

                            if self.priority_queue.len() >= max_queue_size {
                                let mut should_replace = false;
                                let mut worst_profit_in_queue = 0;

                                if let Some((_, worst_p_val_reverse)) = self.priority_queue.peek() {
                                    worst_profit_in_queue = worst_p_val_reverse.0; // Get the actual profit value
                                    if priority < *worst_p_val_reverse {
                                        should_replace = true;
                                    }
                                } else {
                                    // Queue is full but peek failed (empty? should not happen if len >= max_queue_size > 0)
                                    // Or if max_queue_size is 0, this branch is complex. Assume max_queue_size >= 1
                                    should_replace = true; // If queue is notionally full but empty, add.
                                }
                                
                                if should_replace {
                                    let _ = self.priority_queue.pop(); 
                                    self.priority_queue.push(opportunity, priority);
                                    info!(
                                        "Queue full (size {}). New opportunity (profit: {}) is better than worst (profit: {}). Replaced.",
                                        self.priority_queue.len(), // Length after pop and push is same as max_queue_size
                                        new_opportunity_profit,
                                        worst_profit_in_queue 
                                    );
                                } else {
                                    warn!(
                                        "Queue full (size {}). Discarding new opportunity (profit: {}) as it's not better than the worst in queue (worst profit: {}).",
                                        self.priority_queue.len(),
                                        new_opportunity_profit,
                                        worst_profit_in_queue
                                    );
                                }
                            } else {
                                self.priority_queue.push(opportunity, priority);
                            }
                        }
                        None => {
                            info!("Execution channel closed. Processing remaining opportunities.");
                            break;
                        }
                    }
                }
            }
        }

        info!("Draining remaining {} opportunities from priority queue...", self.priority_queue.len());
        while let Some((opportunity, _)) = self.priority_queue.pop() {
            if let Err(e) = self.process_opportunity_internal(opportunity).await {
                error!("Failed to process opportunity from drained queue: {}", e);
            }
        }
        info!("Transaction Executor finished.");
    }

    async fn process_opportunity_internal(&self, opportunity: ArbOpportunity) -> Result<()> {
        self.metrics.execution_attempts.fetch_add(1, Ordering::Relaxed);
        match self.execute_opportunity(opportunity).await {
            Ok(result) => {
                if result.success {
                    self.metrics.execution_successes.fetch_add(1, Ordering::Relaxed);
                    self.metrics.total_profit_lamports.fetch_add(result.profit_lamports, Ordering::Relaxed);
                    info!("✅ Executed trade: profit {} lamports, sig: {:?}, gas_cost: {}, time_ms: {}",
                          result.profit_lamports, result.signature, result.gas_cost, result.execution_time_ms);
                } else {
                    self.metrics.execution_failures.fetch_add(1, Ordering::Relaxed);
                    warn!("❌ Trade failed: {:?}, sig: {:?}, gas_cost: {}, time_ms: {}",
                          result.error, result.signature, result.gas_cost, result.execution_time_ms);
                }
                Ok(())
            }
            Err(e) => {
                self.metrics.execution_failures.fetch_add(1, Ordering::Relaxed); 
                Err(anyhow::anyhow!("Error in execution/simulation framework: {}", e))
            }
        }
    }
    
    async fn execute_opportunity(&self, opportunity: ArbOpportunity) -> Result<ExecutionResult> {
        match self.execution_mode.as_str() {
            "Live" => self.execute_live(opportunity).await,
            "Paper" => self.execute_paper(opportunity).await,
            "Simulate" => self.execute_simulation(opportunity).await,
            _ => Err(anyhow::anyhow!("Unknown execution mode: {}", self.execution_mode)),
        }
    }
    
    async fn execute_live(&self, opportunity: ArbOpportunity) -> Result<ExecutionResult> {
        info!("🔥 Executing LIVE trade for {} lamports profit", opportunity.expected_profit_lamports);
        let measurement_start = Instant::now();

        let priority_fee = self.fee_service
            .get_priority_fee(opportunity.expected_profit_lamports)
            .await?;
        
        let mut instructions = self.build_swap_instructions(&opportunity).await?;
        
        let compute_unit_limit = self.config.compute_unit_limit.unwrap_or(400_000);
        instructions.insert(0, ComputeBudgetInstruction::set_compute_unit_limit(compute_unit_limit));
        instructions.insert(1, ComputeBudgetInstruction::set_compute_unit_price(priority_fee));
        
        let recent_blockhash = self.rpc_client.get_latest_blockhash().await?;
        
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.keypair.pubkey()),
            &[&*self.keypair],
            recent_blockhash,
        );
        
        let mut actual_transaction_signature: Option<solana_sdk::signature::Signature> = None;
        let max_retries = self.config.max_send_retries.unwrap_or(3);

        for attempt in 0..max_retries {
            if self.tpu_client.send_transaction(&transaction) {
                // If send_transaction returns true, the transaction was accepted by the TPU.
                // The signature is part of the transaction object itself.
                if let Some(sig) = transaction.signatures.first() {
                    actual_transaction_signature = Some(*sig);
                    info!("Transaction sent to TPU successfully on attempt {}, signature: {}", attempt + 1, sig);
                    break; 
                } else {
                    // This should not happen for a properly signed transaction
                    error!("TPU accepted transaction, but no signature found in transaction object on attempt {}.", attempt + 1);
                    if attempt >= max_retries - 1 {
                        return Ok(ExecutionResult {
                            success: false,
                            signature: None,
                            error: Some("TPU accepted transaction, but failed to retrieve signature.".to_string()),
                            profit_lamports: 0,
                            gas_cost: priority_fee,
                            execution_time_ms: measurement_start.elapsed().as_millis() as u64,
                        });
                    }
                }
            } else {
                // send_transaction returned false, meaning TPU did not accept it.
                if attempt < max_retries - 1 {
                    warn!("Attempt {}/{} to send transaction via TPU failed (TPU did not accept). Retrying in 100ms...",
                          attempt + 1, max_retries);
                    sleep(Duration::from_millis(100)).await;
                } else {
                    error!("Failed to send transaction via TPU after {} attempts (TPU did not accept).", max_retries);
                    return Ok(ExecutionResult { 
                        success: false,
                        signature: None,
                        error: Some(format!("Failed to send transaction via TPU after {} attempts", max_retries)),
                        profit_lamports: 0,
                        gas_cost: priority_fee, 
                        execution_time_ms: measurement_start.elapsed().as_millis() as u64,
                    });
                }
            }
        }

        let signature = match actual_transaction_signature {
            Some(s) => s,
            None => {
                // This case implies all retries failed to get the transaction accepted by TPU or retrieve signature
                return Ok(ExecutionResult {
                    success: false,
                    signature: None,
                    error: Some("Transaction not successfully sent to TPU after all retries.".to_string()),
                    profit_lamports: 0,
                    gas_cost: priority_fee,
                    execution_time_ms: measurement_start.elapsed().as_millis() as u64,
                });
            }
        };
        
        let timeout = Duration::from_secs(self.config.transaction_confirmation_timeout_secs.unwrap_or(30));
        let poll_interval = Duration::from_millis(self.config.transaction_poll_interval_ms.unwrap_or(500));
        let confirmation_start_time = Instant::now();

        loop {
            if confirmation_start_time.elapsed() > timeout {
                return Ok(ExecutionResult {
                    success: false,
                    signature: Some(signature.to_string()),
                    error: Some("Transaction confirmation timeout".to_string()),
                    profit_lamports: 0,
                    gas_cost: priority_fee,
                    execution_time_ms: measurement_start.elapsed().as_millis() as u64,
                });
            }

            match self.rpc_client.get_signature_status(&signature).await {
                Ok(Some(status)) => {
                    if status.is_ok() {
                        return Ok(ExecutionResult {
                            success: true,
                            signature: Some(signature.to_string()),
                            error: None,
                            profit_lamports: opportunity.expected_profit_lamports as i64,
                            gas_cost: priority_fee,
                            execution_time_ms: measurement_start.elapsed().as_millis() as u64,
                        });
                    } else if status.is_err() {
                        return Ok(ExecutionResult {
                            success: false,
                            signature: Some(signature.to_string()),
                            error: Some(format!("Transaction failed on-chain: {:?}", status)),
                            profit_lamports: 0,
                            gas_cost: priority_fee,
                            execution_time_ms: measurement_start.elapsed().as_millis() as u64,
                        });
                    }
                }
                Ok(None) => { /* Signature not found yet, continue polling. */ }
                Err(e) => {
                    warn!("Error fetching signature status for {}: {}. Retrying.", signature, e);
                }
            }
            
            sleep(poll_interval).await;
        }
    }
    
    async fn execute_paper(&self, opportunity: ArbOpportunity) -> Result<ExecutionResult> {
        info!("📝 Executing PAPER trade for {} lamports profit", opportunity.expected_profit_lamports);
        let measurement_start = Instant::now();
        
        if let Some(paper_trader) = &self.paper_trader {
            // Construct a placeholder SwapPathResult for the paper trader
            // This is simplified; a real version might need more accurate data
            let (first_token_in, first_token_in_symbol) = opportunity.path.paths.first().map_or(
                ("UNKNOWN_IN_ADDR".to_string(), "UNK_IN".to_string()), 
                |r| (r.token_in.clone(), "SYM_IN".to_string()) // Placeholder symbol
            );
            let (last_token_out, last_token_out_symbol) = opportunity.path.paths.last().map_or(
                ("UNKNOWN_OUT_ADDR".to_string(), "UNK_OUT".to_string()), 
                |r| (r.token_out.clone(), "SYM_OUT".to_string()) // Placeholder symbol
            );
            let tokens_path_str = opportunity.path.paths.iter()
                .map(|r| r.dex.to_string()) // Simplified path string
                .collect::<Vec<String>>().join("-");

            let placeholder_swap_path_result = crate::arbitrage::types::SwapPathResult {
                path_id: opportunity.path.id_paths.first().cloned().unwrap_or(0), // Use first id_path or 0
                hops: opportunity.path.hops as u32, // Cast usize to u32
                tokens_path: tokens_path_str,
                route_simulations: vec![], // Paper trader might not need detailed route simulations
                token_in: first_token_in,
                token_in_symbol: first_token_in_symbol,
                token_out: last_token_out,
                token_out_symbol: last_token_out_symbol,
                amount_in: self.config.simulation_amount, // Amount used for the trade
                estimated_amount_out: opportunity.expected_profit_lamports.to_string(), // Approximation
                estimated_min_amount_out: opportunity.expected_profit_lamports.to_string(), // Approximation
                result: opportunity.expected_profit_lamports as f64, // Expected profit
            };

            let paper_trade_outcome = paper_trader.execute_trade(placeholder_swap_path_result).await?;
            
            Ok(ExecutionResult {
                success: paper_trade_outcome.success,
                signature: Some(format!("PAPER-{}", Uuid::new_v4())),
                error: paper_trade_outcome.error,
                profit_lamports: paper_trade_outcome.profit_lamports,
                gas_cost: self.config.paper_trade_mock_gas_cost.unwrap_or(5000),
                execution_time_ms: measurement_start.elapsed().as_millis() as u64,
            })
        } else {
            Err(anyhow::anyhow!("Paper trader not initialized"))
        }
    }
    
    async fn execute_simulation(&self, opportunity: ArbOpportunity) -> Result<ExecutionResult> {
        info!("🔬 Simulating trade for {} lamports profit", opportunity.expected_profit_lamports);
        let measurement_start = Instant::now();
        
        let priority_fee = self.fee_service
            .get_priority_fee(opportunity.expected_profit_lamports)
            .await?;
        
        let instructions = self.build_swap_instructions(&opportunity).await?;
        
        let _latest_blockhash = self.rpc_client.get_latest_blockhash().await?; // Keep for simulation if needed by RPC
        
        // Corrected: Transaction::new_with_payer takes 2 arguments
        // For simulation, we typically don't need to sign it or provide blockhash to this constructor.
        // The simulate_transaction RPC call handles the context.
        let tx_to_simulate = Transaction::new_with_payer(
            &instructions,
            Some(&self.keypair.pubkey()),
            // No signers array or blockhash for this specific constructor
        );
        
        let simulation_response = self.rpc_client
            .simulate_transaction(&tx_to_simulate)
            .await?;
        
        let execution_time_ms = measurement_start.elapsed().as_millis() as u64;
        
        // Corrected handling of simulation_response.value.err
        let err_option = simulation_response.value.err; // Take ownership
        let sim_success = err_option.is_none();
        let error_string = err_option.map(|e| format!("{:?}", e));
        
        Ok(ExecutionResult {
            success: sim_success,
            signature: Some(format!("SIM-{}", Uuid::new_v4())),
            error: error_string,
            profit_lamports: if sim_success { 
                opportunity.expected_profit_lamports as i64 
            } else { 
                0 
            },
            gas_cost: priority_fee,
            execution_time_ms,
        })
    }
    
    /// Build swap instructions from pre-calculated execution plan
    /// NO MATH, NO ESTIMATES - just translate plan to instructions
    async fn build_swap_instructions(&self, opportunity: &ArbOpportunity) -> Result<Vec<Instruction>> {
        // Note: Imports for DEX-specific constructors are now at the top of the file.
        
        let mut all_instructions_details: Vec<InstructionDetails> = Vec::new();
        
        // Pre-flight checks (fast integer comparisons only)
        if opportunity.execution_plan.is_empty() {
            warn!("Executor received opportunity with empty execution plan for path_ids: {:?}, profit: {}", opportunity.path.id_paths, opportunity.expected_profit_lamports);
            return Err(anyhow::anyhow!("Empty execution plan received by executor"));
        }
        
        // Check profitability (already includes estimated gas from metadata)
        if !opportunity.is_profitable() { // Uses helper method from new types.rs
            warn!("Executor received non-profitable opportunity after gas. Net profit: {} lamports. Path_ids: {:?}", 
                opportunity.metadata.net_profit_lamports, opportunity.path.id_paths);
            return Err(anyhow::anyhow!(
                "Opportunity no longer profitable: {} lamports net", 
                opportunity.metadata.net_profit_lamports
            ));
        }
        
        // Validate slippage tolerance (configurable)
        // max_slippage_bps should be in self.config (we added it to config.rs)
        let max_slippage_bps = self.config.max_slippage_bps.unwrap_or(100); // Default 1% (100 bps)
        if !opportunity.validate_slippage(max_slippage_bps) { // Uses helper method
            warn!("Executor: Opportunity slippage exceeds tolerance of {} bps. Path_ids: {:?}", max_slippage_bps, opportunity.path.id_paths);
            return Err(anyhow::anyhow!("Slippage exceeds tolerance"));
        }
        
        // Build instructions from pre-calculated plan
        for (index, leg) in opportunity.execution_plan.iter().enumerate() {
            // let leg_construction_start = Instant::now(); // For detailed timing if needed
            
            let leg_specific_instruction_details: Vec<InstructionDetails> = match leg.dex {
                DexLabel::Meteora => {
                    let params = SwapParametersMeteora {
                        lb_pair: leg.pool_address,
                        amount_in: leg.amount_in,
                        swap_for_y: leg.swap_direction, // Assumes SwapLeg.swap_direction maps to Meteora's swap_for_y
                        input_token: leg.token_in,
                        output_token: leg.token_out,
                        minimum_amount_out: leg.minimum_amount_out,
                    };
                    construct_meteora_instructions(params).await
                }
                
                DexLabel::Raydium => {
                    let params = SwapParametersRaydium {
                        pool: leg.pool_address,
                        input_token_mint: leg.token_in,
                        output_token_mint: leg.token_out,
                        amount_in: leg.amount_in,
                        swap_for_y: leg.swap_direction, // Assumes SwapLeg.swap_direction maps to Raydium's concept
                        min_amount_out: leg.minimum_amount_out,
                    };
                    construct_raydium_instructions(params).await
                }
                
                DexLabel::OrcaWhirlpools => {
                    let params = SwapParametersOrcaWhirpools {
                        whirpools: leg.pool_address, // This is the Whirlpool pubkey
                        input_token: leg.token_in,
                        output_token: leg.token_out,
                        amount_in: leg.amount_in,
                        minimum_amount_out: leg.minimum_amount_out,
                    };
                    construct_orca_whirpools_instructions(params).await
                }
                
                // TODO: Implement for RaydiumClmm and Orca once their constructors are ready
                DexLabel::RaydiumClmm | DexLabel::Orca => {
                    warn!("build_swap_instructions: Skipping currently unsupported DEX: {:?} in leg {}", leg.dex, index + 1);
                    // It's important to decide if an unsupported DEX in a plan is an error or skippable.
                    // For now, returning an error as the plan is pre-calculated and should be executable.
                    return Err(anyhow::anyhow!("Unsupported DEX {:?} in execution plan at leg {}", leg.dex, index + 1));
                    // continue; // Or, if some legs can be skipped (less likely for arbitrage)
                }
            };
            
            // let leg_construction_duration = leg_construction_start.elapsed();
            // if leg_construction_duration.as_millis() > 5 { // Example threshold
            //     warn!("Slow instruction building for leg {}: {}ms", index + 1, leg_construction_duration.as_millis());
            // }
            
            if leg_specific_instruction_details.is_empty() {
                 // This check is important if construct_XYZ_instructions can return empty for valid params
                 // but for an HFT plan, each leg should yield instructions.
                error!("Failed to build instructions for {:?} leg {}. Constructor returned empty.", leg.dex, index + 1);
                return Err(anyhow::anyhow!(
                    "Instruction construction returned empty for {:?} leg {}", 
                    leg.dex, 
                    index + 1
                ));
            }
            
            all_instructions_details.extend(leg_specific_instruction_details);
        }
        
        if all_instructions_details.is_empty() {
            // This implies the execution_plan was not empty, but no instructions were generated.
            warn!("No instructions generated from a non-empty execution plan ({} legs). Path_ids: {:?}",
                opportunity.execution_plan.len(), opportunity.path.id_paths);
            return Err(anyhow::anyhow!("Execution plan yielded no instructions"));
        }
        
        // Extract just the Instruction objects
        Ok(all_instructions_details.into_iter()
            .map(|detail| detail.instruction)
            .collect())
    }
}

/*
Reminder: Ensure the following fields are present in your `Config` struct (e.g., in `src/common/config.rs`):
pub struct Config {
    // ... other fields ...
    pub execution_mode: String,
    pub simulation_amount: u64,
    pub compute_unit_limit: Option<u32>,                 // Default: 400_000
    pub transaction_confirmation_timeout_secs: Option<u64>, // Default: 30
    pub transaction_poll_interval_ms: Option<u64>,        // Default: 500
    pub paper_trade_mock_gas_cost: Option<u64>,           // Default: 5000
    pub max_send_retries: Option<u32>,                    // Default: 3
    pub max_queue_size: Option<usize>,                    // Default: 1000
    // ... other fields ...
}

And ensure `uuid` crate is in `Cargo.toml` dependencies:
uuid = { version = "1.0", features = ["v4"] } // Or your current version

Consider moving the Metrics struct to a dedicated module like `src/common/metrics.rs`
and pass `Arc<Metrics>` when creating `TransactionExecutor`.
Example main.rs or setup:
// In main.rs or where you initialize components:
// use crate::execution::executor::Metrics; // Adjust path if Metrics is moved
// let metrics = Arc::new(Metrics::new());
// let executor = TransactionExecutor::new(..., config, metrics.clone()).await?;
*/
