use anyhow::{Context, Result};
use log::{info, error, warn};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use solana_transaction_status::UiTransactionEncoding;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::{
    arbitrage::types::SwapPathResult,
    common::constants::Env,
    transactions::create_transaction::{
        create_and_send_swap_transaction_with_fee, SendOrSimulate, ChainType,
    },
    fees::priority_fees::{get_global_fee_service, PriorityFeeService},
};

#[derive(Debug, Clone)]
pub enum ExecutionMode {
    Live,
    Paper,
    Simulate,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub swap_path: SwapPathResult,
    pub mode: ExecutionMode,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub signature: Option<String>,
    pub error: Option<String>,
    pub profit: i64,
    pub gas_cost: u64,
}

pub struct TransactionExecutor {
    rpc_client: Arc<RpcClient>,
    keypair: Arc<Keypair>,
    fee_service: Arc<PriorityFeeService>,
    mode: ExecutionMode,
    tx_sender: mpsc::Sender<ExecutionRequest>,
    tx_receiver: mpsc::Receiver<ExecutionRequest>,
}

impl TransactionExecutor {
    pub fn new(mode: ExecutionMode) -> Result<Self> {
        let env = Env::new();
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            env.rpc_url_tx.clone(),
            CommitmentConfig::processed(),
        ));
        
        let keypair = Arc::new(
            solana_sdk::signature::read_keypair_file(&env.payer_keypair_path)
                .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?
        );
        
        // Get the global fee service
        let fee_service = get_global_fee_service()
            .context("Failed to get global fee service")?;
        
        let (tx_sender, tx_receiver) = mpsc::channel::<ExecutionRequest>(100);
        
        Ok(Self {
            rpc_client,
            keypair,
            fee_service,
            mode,
            tx_sender,
            tx_receiver,
        })
    }
    
    /// Get the sender channel for submitting execution requests
    pub fn get_sender(&self) -> mpsc::Sender<ExecutionRequest> {
        self.tx_sender.clone()
    }
    
    /// Start the executor loop
    pub async fn run(mut self) -> Result<()> {
        info!("🚀 Transaction executor started in {:?} mode", self.mode);
        
        while let Some(request) = self.tx_receiver.recv().await {
            match request.mode {
                ExecutionMode::Live => {
                    self.execute_live(request.swap_path).await;
                }
                ExecutionMode::Paper => {
                    self.execute_paper(request.swap_path).await;
                }
                ExecutionMode::Simulate => {
                    self.execute_simulation(request.swap_path).await;
                }
            }
        }
        
        Ok(())
    }
    
    /// Execute a live transaction
    async fn execute_live(&self, swap_path: SwapPathResult) {
        info!("💸 Executing LIVE transaction for path: {}", swap_path.tokens_path);
        
        // Calculate priority fee based on profit
        let profit_lamports = swap_path.result as u64;
        let priority_fee = match self.fee_service.get_priority_fee(profit_lamports).await {
            Ok(fee) => fee,
            Err(e) => {
                error!("Failed to calculate priority fee: {:?}, using default", e);
                10_000 // Fallback
            }
        };
        
        info!("📊 Using priority fee: {} microlamports ({:.6} SOL) for {:.3} SOL profit",
            priority_fee,
            priority_fee as f64 / 1e9,
            profit_lamports as f64 / 1e9
        );
        
        match create_and_send_swap_transaction_with_fee(
            SendOrSimulate::Send,
            ChainType::Mainnet,
            swap_path.clone(),
            priority_fee,
        ).await {
            Ok(_) => {
                info!("✅ Transaction executed successfully!");
            }
            Err(e) => {
                error!("❌ Transaction failed: {:?}", e);
            }
        }
    }
    
    /// Execute a paper trade (no real transaction)
    async fn execute_paper(&self, swap_path: SwapPathResult) {
        info!("📝 Executing PAPER trade for path: {}", swap_path.tokens_path);
        
        // For paper trading, still calculate fee to track costs
        let profit_lamports = swap_path.result as u64;
        let priority_fee = match self.fee_service.get_priority_fee(profit_lamports).await {
            Ok(fee) => fee,
            Err(_) => 10_000,
        };
        
        info!("📝 Paper trade would use {} microlamports priority fee", priority_fee);
        
        // Paper trading logic will be in paper_trading.rs
        use crate::execution::paper_trading::PaperTrader;
        
        let paper_trader = PaperTrader::new();
        match paper_trader.execute_trade(swap_path).await {
            Ok(result) => {
                info!("📝 Paper trade result: Profit = {} SOL (after {} SOL fee)", 
                    result.profit as f64 / 1e9,
                    priority_fee as f64 / 1e9
                );
            }
            Err(e) => {
                error!("❌ Paper trade failed: {:?}", e);
            }
        }
    }
    
    /// Simulate a transaction (RPC simulation only)
    async fn execute_simulation(&self, swap_path: SwapPathResult) {
        info!("🧪 Simulating transaction for path: {}", swap_path.tokens_path);
        
        // Calculate priority fee for simulation
        let profit_lamports = swap_path.result as u64;
        let priority_fee = match self.fee_service.get_priority_fee(profit_lamports).await {
            Ok(fee) => fee,
            Err(_) => 10_000,
        };
        
        match create_and_send_swap_transaction_with_fee(
            SendOrSimulate::Simulate,
            ChainType::Mainnet,
            swap_path,
            priority_fee,
        ).await {
            Ok(_) => {
                info!("✅ Simulation successful with {} microlamports fee!", priority_fee);
            }
            Err(e) => {
                error!("❌ Simulation failed: {:?}", e);
            }
        }
    }
}

/// Queue for managing execution requests
pub struct ExecutionQueue {
    sender: mpsc::Sender<ExecutionRequest>,
}

impl ExecutionQueue {
    pub fn new(sender: mpsc::Sender<ExecutionRequest>) -> Self {
        Self { sender }
    }
    
    pub async fn submit(&self, swap_path: SwapPathResult, mode: ExecutionMode) -> Result<()> {
        let request = ExecutionRequest {
            swap_path,
            mode,
        };
        
        self.sender.send(request).await
            .context("Failed to submit execution request")?;
        
        Ok(())
    }
}

/// Replace the old TCP-based execution with this direct executor
pub async fn execute_profitable_swap(
    swap_path: SwapPathResult,
    execution_queue: &ExecutionQueue,
    mode: ExecutionMode,
) -> Result<()> {
    // Check profit threshold
    let profit_threshold = match mode {
        ExecutionMode::Paper => 0.0, // Execute all paper trades for testing
        _ => 20_000_000.0, // 0.02 SOL for live/simulate
    };
    
    if swap_path.result > profit_threshold {
        info!("💰 Profitable swap detected: {} SOL profit", 
            swap_path.result / 1e9);
        
        execution_queue.submit(swap_path, mode).await?;
    }
    
    Ok(())
}
