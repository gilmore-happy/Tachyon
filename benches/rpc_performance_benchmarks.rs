use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::VersionedTransaction,
    message::{v0, VersionedMessage},
    commitment_config::CommitmentConfig,
};
use std::time::Duration;
use std::str::FromStr;
use tokio::runtime::Runtime;

// Common Solana account addresses for testing
const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
const RAYDIUM_AMM_PROGRAM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

fn create_rpc_client() -> RpcClient {
    // Try to read RPC URL from environment, fallback to public endpoint
    let rpc_url = std::env::var("RPC_URL")
        .or_else(|_| std::env::var("MAINNET_RPC_URL"))
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    
    println!("🔗 Using RPC endpoint: {}", rpc_url);
    RpcClient::new(rpc_url)
}

fn bench_rpc_account_info(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_account_info");
    group.measurement_time(Duration::from_secs(10));
    
    // Test different account types
    let test_accounts = vec![
        ("USDC_mint", USDC_MINT),
        ("SOL_mint", SOL_MINT),
        ("Raydium_program", RAYDIUM_AMM_PROGRAM),
    ];
    
    for (name, address) in test_accounts {
        let pubkey = Pubkey::from_str(address).unwrap();
        
        group.bench_function(format!("get_account_info_{}", name), |b| {
            b.to_async(&rt).iter(|| async {
                black_box(client.get_account(&pubkey).await)
            })
        });
    }
    
    group.finish();
}

fn bench_rpc_multiple_accounts(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_multiple_accounts");
    
    let accounts: Vec<Pubkey> = vec![
        Pubkey::from_str(USDC_MINT).unwrap(),
        Pubkey::from_str(SOL_MINT).unwrap(),
        Pubkey::from_str(RAYDIUM_AMM_PROGRAM).unwrap(),
    ];
    
    group.bench_function("get_multiple_accounts", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(client.get_multiple_accounts(&accounts).await)
        })
    });
    
    group.finish();
}

fn bench_rpc_program_accounts(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_program_accounts");
    group.measurement_time(Duration::from_secs(15));
    
    let raydium_program = Pubkey::from_str(RAYDIUM_AMM_PROGRAM).unwrap();
    
    group.bench_function("get_program_accounts_raydium", |b| {
        b.to_async(&rt).iter(|| async {
            // Limit results to avoid overwhelming the benchmark
            black_box(client.get_program_accounts_with_config(
                &raydium_program,
                solana_client::rpc_config::RpcProgramAccountsConfig {
                    filters: None,
                    account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                        encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                        data_slice: Some(solana_client::rpc_config::RpcAccountInfoConfigDataSlice {
                            offset: 0,
                            length: 100,
                        }),
                        commitment: Some(CommitmentConfig::processed()),
                        min_context_slot: None,
                    },
                    with_context: Some(false),
                }
            ).await)
        })
    });
    
    group.finish();
}

fn bench_rpc_slot_info(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_slot_info");
    
    group.bench_function("get_slot", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(client.get_slot().await)
        })
    });
    
    group.bench_function("get_block_height", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(client.get_block_height().await)
        })
    });
    
    group.bench_function("get_latest_blockhash", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(client.get_latest_blockhash().await)
        })
    });
    
    group.finish();
}

fn bench_rpc_transaction_simulation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_transaction_simulation");
    group.measurement_time(Duration::from_secs(10));
    
    // Create a simple test transaction
    let keypair = Keypair::new();
    let recipient = Pubkey::new_unique();
    
    group.bench_function("simulate_simple_transfer", |b| {
        b.to_async(&rt).iter(|| async {
            // Get a recent blockhash
            let blockhash = match client.get_latest_blockhash().await {
                Ok(hash) => hash,
                Err(_) => return Ok(None), // Skip if can't get blockhash
            };
            
            // Create a simple transfer instruction
            let instruction = system_instruction::transfer(
                &keypair.pubkey(),
                &recipient,
                1_000_000, // 0.001 SOL
            );
            
            // Create transaction
            let message = v0::Message::try_compile(
                &keypair.pubkey(),
                &[instruction],
                &[],
                blockhash,
            ).unwrap();
            
            let versioned_message = VersionedMessage::V0(message);
            let transaction = VersionedTransaction::try_new(versioned_message, &[&keypair]).unwrap();
            
            black_box(client.simulate_transaction(&transaction).await)
        })
    });
    
    group.finish();
}

fn bench_rpc_parallel_requests(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_parallel_requests");
    
    for parallel_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("parallel_slot_requests", parallel_count),
            parallel_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let futures: Vec<_> = (0..count)
                        .map(|_| client.get_slot())
                        .collect();
                    
                    black_box(futures::future::join_all(futures).await)
                })
            },
        );
    }
    
    group.finish();
}

fn bench_rpc_rate_limits(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_rate_limits");
    group.measurement_time(Duration::from_secs(30));
    
    group.bench_function("sustained_requests", |b| {
        b.to_async(&rt).iter(|| async {
            let mut results = Vec::new();
            
            // Make 50 requests as fast as possible to test rate limiting
            for _ in 0..50 {
                let result = client.get_slot().await;
                results.push(result);
                
                // Small delay to avoid completely overwhelming the endpoint
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            black_box(results)
        })
    });
    
    group.finish();
}

fn bench_rpc_error_handling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let client = create_rpc_client();
    
    let mut group = c.benchmark_group("rpc_error_handling");
    
    // Test with non-existent account
    let fake_pubkey = Pubkey::new_unique();
    
    group.bench_function("handle_missing_account", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(client.get_account(&fake_pubkey).await)
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_rpc_account_info,
    bench_rpc_multiple_accounts,
    bench_rpc_program_accounts,
    bench_rpc_slot_info,
    bench_rpc_transaction_simulation,
    bench_rpc_parallel_requests,
    bench_rpc_rate_limits,
    bench_rpc_error_handling
);
criterion_main!(benches); 