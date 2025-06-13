use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mev_bot_solana::arbitrage::types::{ArbOpportunity, SwapPath, Route, OpportunityMetadata, OpportunitySource};
use mev_bot_solana::markets::types::DexLabel;
use std::time::Duration;

fn create_test_opportunity(hops: usize, profit: u64) -> ArbOpportunity {
    let routes: Vec<Route> = (0..hops)
        .map(|i| Route {
            id: i as u32,
            pool_address: format!("pool_{}", i),
            token_in: format!("token_in_{}", i),
            token_out: format!("token_out_{}", i % 2), // Circular for arbitrage
            dex: match i % 3 {
                0 => DexLabel::Raydium,
                1 => DexLabel::Orca,
                _ => DexLabel::Meteora,
            },
            token_0to1: i % 2 == 0,
        })
        .collect();
    
    ArbOpportunity {
        path: SwapPath {
            id_paths: (0..hops).map(|i| i as u32).collect(),
            hops,
            paths: routes,
        },
        expected_profit_lamports: profit,
        timestamp_unix_nanos: 1000000000,
        execution_plan: vec![], // Empty for benchmarking
        metadata: OpportunityMetadata {
            estimated_gas_cost: 5000,
            net_profit_lamports: profit as i64 - 5000,
            profit_percentage_bps: 250, // 2.5%
            risk_score: 30,
            source: OpportunitySource::StrategyScan { 
                strategy_name: "triangular_arb".to_string() 
            },
            max_latency_ms: 100,
        },
    }
}

fn bench_opportunity_evaluation(c: &mut Criterion) {
    let mut group = c.benchmark_group("opportunity_evaluation");
    group.measurement_time(Duration::from_secs(5));
    
    for hop_count in [1, 2, 3, 4].iter() {
        let opportunities: Vec<ArbOpportunity> = (0..100)
            .map(|i| create_test_opportunity(*hop_count, (i * 1000) as u64))
            .collect();
        
        group.bench_with_input(
            BenchmarkId::new("evaluate_opportunities", hop_count),
            hop_count,
            |b, _| {
                b.iter(|| {
                    let mut best_opportunity: Option<&ArbOpportunity> = None;
                    let mut best_profit = 0u64;
                    
                    for opp in black_box(&opportunities) {
                        // Simulate profit calculation with gas costs
                        let net_profit = opp.expected_profit_lamports
                            .saturating_sub(opp.metadata.estimated_gas_cost);
                        
                        if net_profit > best_profit {
                            best_profit = net_profit;
                            best_opportunity = Some(opp);
                        }
                    }
                    
                    best_opportunity
                })
            },
        );
    }
    
    group.finish();
}

fn bench_priority_queue_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_queue");
    
    use priority_queue::PriorityQueue;
    use std::cmp::Reverse;
    
    group.bench_function("queue_operations", |b| {
        b.iter(|| {
            let mut queue: PriorityQueue<ArbOpportunity, Reverse<u64>> = PriorityQueue::new();
            
            // Insert 1000 opportunities
            for i in 0..1000 {
                let opp = create_test_opportunity(2, i * 100);
                let priority = Reverse(opp.expected_profit_lamports);
                queue.push(black_box(opp), priority);
            }
            
            // Pop top 100
            for _ in 0..100 {
                black_box(queue.pop());
            }
            
            queue.len()
        })
    });
    
    group.finish();
}

fn bench_transaction_building_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("transaction_building");
    
    let opportunity = create_test_opportunity(3, 50000);
    
    group.bench_function("build_swap_instructions", |b| {
        b.iter(|| {
            // Simulate instruction building for each hop
            let mut instructions = Vec::with_capacity(opportunity.path.hops * 2);
            
            for route in black_box(&opportunity.path.paths) {
                // Simulate creating swap instruction
                let instruction_data = format!(
                    "swap_{}_{}_{}",
                    route.pool_address,
                    route.token_in,
                    route.token_out
                );
                
                // Simulate instruction creation overhead
                let _accounts = vec![
                    route.pool_address.clone(),
                    route.token_in.clone(),
                    route.token_out.clone(),
                ];
                
                instructions.push(instruction_data);
            }
            
            black_box(instructions)
        })
    });
    
    group.bench_function("calculate_priority_fee", |b| {
        b.iter(|| {
            let profit = black_box(opportunity.expected_profit_lamports);
            
            // Simulate priority fee calculation
            let base_fee = 1000u64;
            let profit_percentage = 5; // 5% of profit
            let calculated_fee = std::cmp::min(
                base_fee + (profit * profit_percentage / 100),
                profit / 10, // Max 10% of profit
            );
            
            black_box(calculated_fee)
        })
    });
    
    group.finish();
}

fn bench_concurrent_execution_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_execution");
    
    group.bench_function("parallel_opportunity_processing", |b| {
        b.iter(|| {
            use std::sync::Arc;
            use std::thread;
            
            let opportunities: Vec<ArbOpportunity> = (0..1000)
                .map(|i| create_test_opportunity(2, i * 100))
                .collect();
            
            let opportunities_ref = Arc::new(opportunities);
            
            let handles: Vec<_> = (0..4)
                .map(|thread_id| {
                    let opps = opportunities_ref.clone();
                    thread::spawn(move || {
                        let start = thread_id * 250;
                        let end = start + 250;
                        let mut processed = 0;
                        
                        for opp in &opps[start..end] {
                            // Simulate processing
                            let _net_profit = opp.expected_profit_lamports
                                .saturating_sub(opp.metadata.estimated_gas_cost);
                            processed += 1;
                        }
                        
                        processed
                    })
                })
                .collect();
            
            let mut total_processed = 0;
            for handle in handles {
                total_processed += handle.join().unwrap();
            }
            
            black_box(total_processed)
        })
    });
    
    group.finish();
}

fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");
    
    group.bench_function("opportunity_creation_optimized", |b| {
        b.iter(|| {
            // Test memory-efficient opportunity creation
            let mut opportunities = Vec::with_capacity(1000);
            
            for i in 0..1000 {
                opportunities.push(create_test_opportunity(2, i * 100));
            }
            
            // Simulate processing without additional allocations
            let mut total_profit = 0u64;
            for opp in &opportunities {
                total_profit += opp.expected_profit_lamports;
            }
            
            black_box(total_profit)
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_opportunity_evaluation,
    bench_priority_queue_operations,
    bench_transaction_building_simulation,
    bench_concurrent_execution_simulation,
    bench_memory_efficiency
);
criterion_main!(benches); 