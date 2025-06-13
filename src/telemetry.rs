//! src/telemetry.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering}; // Added AtomicU64 and Ordering
// Removed prometheus imports and SocketAddr
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone, Debug)] // Added Debug
pub struct Metrics {
    pub opportunities_discovered: Arc<AtomicU64>,
    // For txs_executed, we'll need separate counters if we remove IntCounterVec
    pub txs_executed_success: Arc<AtomicU64>,
    pub txs_executed_failure: Arc<AtomicU64>,
    pub pools_loaded: Arc<AtomicU64>,
    // New fields
    pub opportunities_sent: Arc<AtomicU64>,
    pub opportunities_dropped: Arc<AtomicU64>,
    pub opportunities_rejected: Arc<AtomicU64>,
}

impl Metrics {
    // new() no longer returns a Result as Prometheus errors are removed
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            opportunities_discovered: Arc::new(AtomicU64::new(0)),
            txs_executed_success: Arc::new(AtomicU64::new(0)),
            txs_executed_failure: Arc::new(AtomicU64::new(0)),
            pools_loaded: Arc::new(AtomicU64::new(0)),
            opportunities_sent: Arc::new(AtomicU64::new(0)),
            opportunities_dropped: Arc::new(AtomicU64::new(0)),
            opportunities_rejected: Arc::new(AtomicU64::new(0)),
        })
    }

    // Example helper methods for incrementing (optional, but good practice)
    // These would be called instead of direct fetch_add elsewhere to encapsulate Ordering.
    pub fn inc_opportunities_discovered(&self) {
        self.opportunities_discovered.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_txs_executed_success(&self) {
        self.txs_executed_success.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_txs_executed_failure(&self) {
        self.txs_executed_failure.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn add_pools_loaded(&self, count: u64) {
        self.pools_loaded.fetch_add(count, Ordering::Relaxed);
    }

    pub fn inc_opportunities_sent(&self) {
        self.opportunities_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_opportunities_dropped(&self) {
        self.opportunities_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_opportunities_rejected(&self) {
        self.opportunities_rejected.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn init_telemetry() -> Arc<Metrics> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let formatting_layer = fmt::layer().pretty();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(formatting_layer)
        .init();

    // Prometheus exporter removed
    // tokio::spawn(async {
    //     let addr: SocketAddr = "0.0.0.0:9090".parse().expect("Failed to parse socket address");
    //     prometheus_exporter::start(addr).expect("Failed to start Prometheus exporter");
    // });

    Metrics::new() // No longer expect(), as new() doesn't return Result
}
