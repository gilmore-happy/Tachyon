use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Real-time performance metrics for the MEV bot
#[derive(Debug)]
pub struct PerformanceMetrics {
    // Arbitrage metrics
    pub paths_calculated_per_second: AtomicU64,
    pub opportunities_found_per_second: AtomicU64,
    pub avg_path_calculation_time_ns: AtomicU64,
    
    // Execution metrics
    pub transactions_sent_per_second: AtomicU64,
    pub avg_transaction_build_time_ns: AtomicU64,
    pub success_rate_bps: AtomicU32, // basis points (10000 = 100%)
    
    // Memory metrics
    pub peak_memory_usage_bytes: AtomicU64,
    pub current_memory_usage_bytes: AtomicU64,
    pub allocations_per_second: AtomicU64,
    
    // Network metrics
    pub rpc_calls_per_second: AtomicU64,
    pub avg_rpc_latency_ms: AtomicU64,
    pub websocket_messages_per_second: AtomicU64,
    
    // Profit metrics
    pub total_profit_lamports: AtomicU64,
    pub avg_profit_per_opportunity: AtomicU64,
    pub opportunities_missed: AtomicU64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            paths_calculated_per_second: AtomicU64::new(0),
            opportunities_found_per_second: AtomicU64::new(0),
            avg_path_calculation_time_ns: AtomicU64::new(0),
            transactions_sent_per_second: AtomicU64::new(0),
            avg_transaction_build_time_ns: AtomicU64::new(0),
            success_rate_bps: AtomicU32::new(0),
            peak_memory_usage_bytes: AtomicU64::new(0),
            current_memory_usage_bytes: AtomicU64::new(0),
            allocations_per_second: AtomicU64::new(0),
            rpc_calls_per_second: AtomicU64::new(0),
            avg_rpc_latency_ms: AtomicU64::new(0),
            websocket_messages_per_second: AtomicU64::new(0),
            total_profit_lamports: AtomicU64::new(0),
            avg_profit_per_opportunity: AtomicU64::new(0),
            opportunities_missed: AtomicU64::new(0),
        }
    }
    
    /// Record path calculation performance
    pub fn record_path_calculation(&self, duration: Duration, paths_found: u64) {
        let duration_ns = duration.as_nanos() as u64;
        self.avg_path_calculation_time_ns.store(duration_ns, Ordering::Relaxed);
        self.paths_calculated_per_second.fetch_add(paths_found, Ordering::Relaxed);
    }
    
    /// Record opportunity discovery
    pub fn record_opportunity_found(&self, profit_lamports: u64) {
        self.opportunities_found_per_second.fetch_add(1, Ordering::Relaxed);
        self.total_profit_lamports.fetch_add(profit_lamports, Ordering::Relaxed);
        
        // Update average profit (simple moving average)
        let current_avg = self.avg_profit_per_opportunity.load(Ordering::Relaxed);
        let new_avg = (current_avg + profit_lamports) / 2;
        self.avg_profit_per_opportunity.store(new_avg, Ordering::Relaxed);
    }
    
    /// Record transaction execution
    pub fn record_transaction_execution(&self, build_time: Duration, success: bool) {
        let build_time_ns = build_time.as_nanos() as u64;
        self.avg_transaction_build_time_ns.store(build_time_ns, Ordering::Relaxed);
        self.transactions_sent_per_second.fetch_add(1, Ordering::Relaxed);
        
        // Update success rate
        if success {
            let current_rate = self.success_rate_bps.load(Ordering::Relaxed);
            let new_rate = std::cmp::min(10000, current_rate + 1);
            self.success_rate_bps.store(new_rate, Ordering::Relaxed);
        }
    }
    
    /// Record RPC call performance
    pub fn record_rpc_call(&self, latency: Duration) {
        let latency_ms = latency.as_millis() as u64;
        self.avg_rpc_latency_ms.store(latency_ms, Ordering::Relaxed);
        self.rpc_calls_per_second.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record memory usage
    pub fn record_memory_usage(&self, current_bytes: u64) {
        self.current_memory_usage_bytes.store(current_bytes, Ordering::Relaxed);
        
        // Update peak if necessary
        let current_peak = self.peak_memory_usage_bytes.load(Ordering::Relaxed);
        if current_bytes > current_peak {
            self.peak_memory_usage_bytes.store(current_bytes, Ordering::Relaxed);
        }
    }
    
    /// Get performance summary
    pub fn get_summary(&self) -> PerformanceSummary {
        PerformanceSummary {
            paths_per_second: self.paths_calculated_per_second.load(Ordering::Relaxed),
            opportunities_per_second: self.opportunities_found_per_second.load(Ordering::Relaxed),
            avg_path_calc_time_ms: self.avg_path_calculation_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            transactions_per_second: self.transactions_sent_per_second.load(Ordering::Relaxed),
            avg_tx_build_time_ms: self.avg_transaction_build_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            success_rate_percent: self.success_rate_bps.load(Ordering::Relaxed) as f64 / 100.0,
            memory_usage_mb: self.current_memory_usage_bytes.load(Ordering::Relaxed) as f64 / 1_048_576.0,
            peak_memory_mb: self.peak_memory_usage_bytes.load(Ordering::Relaxed) as f64 / 1_048_576.0,
            avg_rpc_latency_ms: self.avg_rpc_latency_ms.load(Ordering::Relaxed),
            total_profit_sol: self.total_profit_lamports.load(Ordering::Relaxed) as f64 / 1_000_000_000.0,
            avg_profit_per_opp_sol: self.avg_profit_per_opportunity.load(Ordering::Relaxed) as f64 / 1_000_000_000.0,
        }
    }
    
    /// Reset per-second counters (call every second)
    pub fn reset_per_second_counters(&self) {
        self.paths_calculated_per_second.store(0, Ordering::Relaxed);
        self.opportunities_found_per_second.store(0, Ordering::Relaxed);
        self.transactions_sent_per_second.store(0, Ordering::Relaxed);
        self.rpc_calls_per_second.store(0, Ordering::Relaxed);
        self.websocket_messages_per_second.store(0, Ordering::Relaxed);
        self.allocations_per_second.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub paths_per_second: u64,
    pub opportunities_per_second: u64,
    pub avg_path_calc_time_ms: f64,
    pub transactions_per_second: u64,
    pub avg_tx_build_time_ms: f64,
    pub success_rate_percent: f64,
    pub memory_usage_mb: f64,
    pub peak_memory_mb: f64,
    pub avg_rpc_latency_ms: u64,
    pub total_profit_sol: f64,
    pub avg_profit_per_opp_sol: f64,
}

/// Performance monitor that tracks and reports metrics
pub struct PerformanceMonitor {
    metrics: Arc<PerformanceMetrics>,
    start_time: Instant,
    last_report: RwLock<Instant>,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(PerformanceMetrics::new()),
            start_time: Instant::now(),
            last_report: RwLock::new(Instant::now()),
        }
    }
    
    pub fn metrics(&self) -> Arc<PerformanceMetrics> {
        self.metrics.clone()
    }
    
    /// Start the performance monitoring task
    pub async fn start_monitoring(&self, report_interval: Duration) {
        let metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(report_interval);
            
            loop {
                interval.tick().await;
                
                // Reset per-second counters
                metrics.reset_per_second_counters();
                
                // Generate and log performance report
                let summary = metrics.get_summary();
                Self::log_performance_report(&summary).await;
            }
        });
    }
    
    async fn log_performance_report(summary: &PerformanceSummary) {
        info!(
            "🚀 PERFORMANCE REPORT 🚀\n\
            ┌─ Arbitrage Performance ─────────────────┐\n\
            │ Paths/sec: {:>8}                    │\n\
            │ Opportunities/sec: {:>8}            │\n\
            │ Avg path calc: {:>8.2}ms            │\n\
            ├─ Execution Performance ─────────────────┤\n\
            │ Transactions/sec: {:>8}             │\n\
            │ Avg tx build: {:>8.2}ms             │\n\
            │ Success rate: {:>8.1}%              │\n\
            ├─ System Performance ────────────────────┤\n\
            │ Memory usage: {:>8.1}MB             │\n\
            │ Peak memory: {:>8.1}MB              │\n\
            │ RPC latency: {:>8}ms                │\n\
            ├─ Profit Metrics ────────────────────────┤\n\
            │ Total profit: {:>8.3} SOL          │\n\
            │ Avg per opp: {:>8.6} SOL           │\n\
            └─────────────────────────────────────────┘",
            summary.paths_per_second,
            summary.opportunities_per_second,
            summary.avg_path_calc_time_ms,
            summary.transactions_per_second,
            summary.avg_tx_build_time_ms,
            summary.success_rate_percent,
            summary.memory_usage_mb,
            summary.peak_memory_mb,
            summary.avg_rpc_latency_ms,
            summary.total_profit_sol,
            summary.avg_profit_per_opp_sol
        );
        
        // Warn about performance issues
        if summary.avg_path_calc_time_ms > 10.0 {
            warn!("⚠️  Path calculation time is high: {:.2}ms", summary.avg_path_calc_time_ms);
        }
        
        if summary.success_rate_percent < 80.0 {
            warn!("⚠️  Transaction success rate is low: {:.1}%", summary.success_rate_percent);
        }
        
        if summary.memory_usage_mb > 500.0 {
            warn!("⚠️  Memory usage is high: {:.1}MB", summary.memory_usage_mb);
        }
        
        if summary.avg_rpc_latency_ms > 100 {
            warn!("⚠️  RPC latency is high: {}ms", summary.avg_rpc_latency_ms);
        }
    }
    
    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for timing operations
#[macro_export]
macro_rules! time_operation {
    ($metrics:expr, $operation:expr, $record_fn:ident) => {{
        let start = std::time::Instant::now();
        let result = $operation;
        let duration = start.elapsed();
        $metrics.$record_fn(duration);
        result
    }};
}

/// Macro for timing operations with additional parameters
#[macro_export]
macro_rules! time_operation_with_params {
    ($metrics:expr, $operation:expr, $record_fn:ident, $($param:expr),*) => {{
        let start = std::time::Instant::now();
        let result = $operation;
        let duration = start.elapsed();
        $metrics.$record_fn(duration, $($param),*);
        result
    }};
} 