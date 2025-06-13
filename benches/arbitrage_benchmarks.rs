use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mev_bot_solana::arbitrage::{
    calc_arb::calculate_arbitrage_paths_1_hop,
    types::{TokenInArb, Route, SwapPath},
};
use mev_bot_solana::markets::types::{Market, DexLabel};
use std::collections::HashMap;
use std::time::Duration;

fn create_test_markets(count: usize) -> HashMap<String, Vec<Market>> {
    let mut markets = HashMap::new();
    
    for i in 0..count {
        let pair = format!("SOL-USDC-{}", i);
        let market = Market {
            token_mint_a: format!("token_a_{}", i),
            token_vault_a: format!("vault_a_{}", i),
            token_mint_b: format!("token_b_{}", i),
            token_vault_b: format!("vault_b_{}", i),
            dex_label: DexLabel::Raydium,
            fee: 25, // 0.25%
            id: format!("market_{}", i),
            account_data: None,
            liquidity: Some(1_000_000),
        };
        markets.insert(pair, vec![market]);
    }
    
    markets
}

fn create_test_tokens(count: usize) -> Vec<TokenInArb> {
    (0..count)
        .map(|i| TokenInArb {
            token: format!("token_{}", i),
            symbol: format!("TOK{}", i),
            decimals: 9, // Standard SOL/SPL token decimals
        })
        .collect()
}

fn bench_path_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("arbitrage_path_calculation");
    group.measurement_time(Duration::from_secs(10));
    
    for market_count in [10, 50, 100, 500].iter() {
        let markets = create_test_markets(*market_count);
        let tokens = create_test_tokens(5); // Fixed token count
        
        group.bench_with_input(
            BenchmarkId::new("1_hop_paths", market_count),
            market_count,
            |b, _| {
                b.iter(|| {
                    calculate_arbitrage_paths_1_hop(
                        black_box(&tokens),
                        black_box(&markets),
                    )
                })
            },
        );
    }
    
    group.finish();
}

fn bench_path_evaluation(c: &mut Criterion) {
    let mut group = c.benchmark_group("path_evaluation");
    
    // Create test paths
    let paths: Vec<SwapPath> = (0..1000)
        .map(|i| SwapPath {
            id_paths: vec![i],
            hops: 1,
            paths: vec![Route {
                id: i,
                pool_address: format!("pool_{}", i),
                token_in: format!("token_in_{}", i),
                token_out: format!("token_out_{}", i),
                dex: DexLabel::Raydium,
                token_0to1: i % 2 == 0,
            }],
        })
        .collect();
    
    group.bench_function("evaluate_1000_paths", |b| {
        b.iter(|| {
            // Simulate path evaluation logic
            let mut best_profit = 0u64;
            for path in black_box(&paths) {
                let simulated_profit = path.id_paths[0] as u64 * 1000; // Mock calculation
                if simulated_profit > best_profit {
                    best_profit = simulated_profit;
                }
            }
            best_profit
        })
    });
    
    group.finish();
}

fn bench_memory_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocations");
    
    group.bench_function("path_creation_optimized", |b| {
        b.iter(|| {
            // Test optimized path creation (minimal allocations)
            let mut paths = Vec::with_capacity(100);
            for i in 0..100 {
                paths.push(SwapPath {
                    id_paths: vec![i],
                    hops: 1,
                    paths: vec![Route {
                        id: i,
                        pool_address: format!("pool_{}", i),
                        token_in: format!("token_in_{}", i),
                        token_out: format!("token_out_{}", i),
                        dex: DexLabel::Raydium,
                        token_0to1: i % 2 == 0,
                    }],
                });
            }
            black_box(paths)
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_path_calculation,
    bench_path_evaluation,
    bench_memory_allocations
);
criterion_main!(benches); 