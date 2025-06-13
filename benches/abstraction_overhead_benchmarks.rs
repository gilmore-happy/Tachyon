use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::hint::black_box as hint_black_box;

// Direct implementation (current approach)
struct DirectSolanaClient {
    rpc_url: String,
}

impl DirectSolanaClient {
    #[inline(always)]
    fn get_account_direct(&self, address: &str) -> u64 {
        // Simulate account fetching work
        address.len() as u64 * 42
    }
    
    #[inline(always)]
    fn calculate_swap_direct(&self, amount: u64) -> u64 {
        // Simulate swap calculation
        amount * 997 / 1000 // 0.3% fee simulation
    }
}

// Static dispatch abstraction (recommended)
trait StaticAdapter {
    fn get_account(&self, address: &str) -> u64;
    fn calculate_swap(&self, amount: u64) -> u64;
}

struct StaticSolanaAdapter {
    rpc_url: String,
}

impl StaticAdapter for StaticSolanaAdapter {
    #[inline(always)]
    fn get_account(&self, address: &str) -> u64 {
        address.len() as u64 * 42
    }
    
    #[inline(always)]
    fn calculate_swap(&self, amount: u64) -> u64 {
        amount * 997 / 1000
    }
}

// Dynamic dispatch (what to avoid)
struct DynamicSolanaAdapter {
    rpc_url: String,
}

impl StaticAdapter for DynamicSolanaAdapter {
    fn get_account(&self, address: &str) -> u64 {
        address.len() as u64 * 42
    }
    
    fn calculate_swap(&self, amount: u64) -> u64 {
        amount * 997 / 1000
    }
}

// Generic function using static dispatch
#[inline(always)]
fn execute_arbitrage_static<T: StaticAdapter>(adapter: &T) -> u64 {
    let account_data = adapter.get_account("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    let swap_result = adapter.calculate_swap(account_data);
    hint_black_box(swap_result)
}

// Function using dynamic dispatch
fn execute_arbitrage_dynamic(adapter: &dyn StaticAdapter) -> u64 {
    let account_data = adapter.get_account("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    let swap_result = adapter.calculate_swap(account_data);
    hint_black_box(swap_result)
}

fn benchmark_abstraction_overhead(c: &mut Criterion) {
    let direct_client = DirectSolanaClient {
        rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
    };
    
    let static_adapter = StaticSolanaAdapter {
        rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
    };
    
    let dynamic_adapter = DynamicSolanaAdapter {
        rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
    };

    let mut group = c.benchmark_group("abstraction_overhead");
    
    // Direct implementation (baseline)
    group.bench_function("direct_calls", |b| {
        b.iter(|| {
            let account_data = direct_client.get_account_direct(black_box("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"));
            let swap_result = direct_client.calculate_swap_direct(black_box(account_data));
            hint_black_box(swap_result)
        })
    });
    
    // Static dispatch (recommended abstraction)
    group.bench_function("static_dispatch", |b| {
        b.iter(|| {
            execute_arbitrage_static(black_box(&static_adapter))
        })
    });
    
    // Dynamic dispatch (what to avoid in hot paths)
    group.bench_function("dynamic_dispatch", |b| {
        b.iter(|| {
            execute_arbitrage_dynamic(black_box(&dynamic_adapter))
        })
    });
    
    // Function call overhead test
    group.bench_function("function_call_layers", |b| {
        b.iter(|| {
            // Simulate 3 layers of function calls
            fn layer1(x: u64) -> u64 { layer2(x * 2) }
            fn layer2(x: u64) -> u64 { layer3(x + 1) }
            #[inline(always)]
            fn layer3(x: u64) -> u64 { x * 997 / 1000 }
            
            hint_black_box(layer1(black_box(1000000)))
        })
    });
    
    group.finish();
}

criterion_group!(benches, benchmark_abstraction_overhead);
criterion_main!(benches); 