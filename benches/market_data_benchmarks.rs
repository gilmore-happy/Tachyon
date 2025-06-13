use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mev_bot_solana::markets::{
    lockless_cache::LocklessMarketCache,
    types::{Market, DexLabel},
};
use std::time::Duration;

fn create_test_market(id: usize) -> Market {
    Market {
        token_mint_a: format!("mint_a_{}", id),
        token_vault_a: format!("vault_a_{}", id),
        token_mint_b: format!("mint_b_{}", id),
        token_vault_b: format!("vault_b_{}", id),
        dex_label: DexLabel::Raydium,
        fee: 25,
        id: format!("market_{}", id),
        account_data: Some(vec![0u8; 1024]), // 1KB of mock data
        liquidity: Some(1_000_000),
    }
}

fn bench_cache_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_operations");
    group.measurement_time(Duration::from_secs(5));
    
    let cache = LocklessMarketCache::new();
    
    // Pre-populate cache
    for i in 0..1000 {
        cache.insert(create_test_market(i));
    }
    
    group.bench_function("cache_insert", |b| {
        let mut counter = 1000;
        b.iter(|| {
            cache.insert(black_box(create_test_market(counter)));
            counter += 1;
        })
    });
    
    group.bench_function("cache_get_hit", |b| {
        b.iter(|| {
            let id = format!("market_{}", black_box(500));
            cache.get(&id)
        })
    });
    
    group.bench_function("cache_get_miss", |b| {
        b.iter(|| {
            let id = format!("market_nonexistent_{}", black_box(9999));
            cache.get(&id)
        })
    });
    
    group.finish();
}

fn bench_concurrent_cache_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_cache");
    
    let cache = LocklessMarketCache::new();
    
    // Pre-populate
    for i in 0..10000 {
        cache.insert(create_test_market(i));
    }
    
    group.bench_function("concurrent_reads", |b| {
        b.iter(|| {
            use std::sync::Arc;
            use std::thread;
            
            let cache_ref = Arc::new(cache.clone());
            let handles: Vec<_> = (0..4)
                .map(|thread_id| {
                    let cache_clone = cache_ref.clone();
                    thread::spawn(move || {
                        for i in 0..100 {
                            let id = format!("market_{}", (thread_id * 100 + i) % 1000);
                            black_box(cache_clone.get(&id));
                        }
                    })
                })
                .collect();
            
            for handle in handles {
                handle.join().unwrap();
            }
        })
    });
    
    group.finish();
}

fn bench_market_data_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("market_data_parsing");
    
    // Mock account data for different DEXes
    let raydium_data = vec![0u8; 752]; // Typical Raydium AMM size
    let orca_data = vec![0u8; 324];    // Typical Orca pool size
    let _meteora_data = vec![0u8; 1024]; // Typical Meteora DLMM size
    
    group.bench_function("parse_raydium_account", |b| {
        b.iter(|| {
            // Simulate parsing Raydium account data
            let data = black_box(&raydium_data);
            let _parsed_fee = u64::from_le_bytes([
                data[100], data[101], data[102], data[103],
                data[104], data[105], data[106], data[107],
            ]);
            // More parsing simulation...
        })
    });
    
    group.bench_function("parse_orca_account", |b| {
        b.iter(|| {
            let data = black_box(&orca_data);
            let _parsed_fee = u64::from_le_bytes([
                data[50], data[51], data[52], data[53],
                data[54], data[55], data[56], data[57],
            ]);
        })
    });
    
    group.finish();
}

fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");
    
    for batch_size in [10, 50, 100, 500].iter() {
        let markets: Vec<Market> = (0..*batch_size)
            .map(create_test_market)
            .collect();
        
        group.bench_with_input(
            BenchmarkId::new("process_market_batch", batch_size),
            batch_size,
            |b, _| {
                b.iter(|| {
                    let mut total_liquidity = 0u64;
                    for market in black_box(&markets) {
                        total_liquidity += market.liquidity.unwrap_or(0);
                        // Simulate additional processing
                        let _fee_calc = market.fee * 1000;
                    }
                    total_liquidity
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_cache_operations,
    bench_concurrent_cache_access,
    bench_market_data_parsing,
    bench_batch_processing
);
criterion_main!(benches); 