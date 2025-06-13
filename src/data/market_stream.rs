//! src/data/market_stream.rs

use crate::common::config::{Config, DataMode};
use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{error, info, info_span, Instrument};

#[derive(Debug, Clone)]
pub struct MarketEvent {
    pub token_pair: String, // e.g., "SOL/USDC"
    pub price: f64,
    pub source: String, // e.g., "Pyth", "Raydium"
}

pub async fn init_market_data(config: &Config) -> Result<mpsc::Receiver<MarketEvent>> {
    let (tx, rx) = mpsc::channel(1000);

    match &config.data_mode {
        DataMode::WebSocket(url) => {
            tokio::spawn(
                ws_listener(url.clone(), tx)
                    .instrument(info_span!("ws_listener")),
            );
        }
        DataMode::Grpc(url) => {
            tokio::spawn(
                grpc_listener(url.clone(), tx)
                    .instrument(info_span!("grpc_listener")),
            );
        }
    }

    Ok(rx)
}

async fn ws_listener(url: String, tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    // Add connection retries for robustness
    let max_retries = 3;
    let mut retry_count = 0;
    let mut ws_stream = None;
    
    while retry_count < max_retries {
        match connect_async(&url).await {
            Ok((stream, _)) => {
                ws_stream = Some(stream);
                break;
            },
            Err(e) => {
                retry_count += 1;
                error!("WebSocket connection attempt {} failed: {}", retry_count, e);
                if retry_count < max_retries {
                    // Add exponential backoff
                    let delay = std::time::Duration::from_millis(500 * 2u64.pow(retry_count as u32));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    
    let ws_stream = match ws_stream {
        Some(stream) => stream,
        None => {
            error!("Failed to connect to WebSocket after {} attempts. Using fallback mode.", max_retries);
            // Return without error to allow the bot to continue with other data sources
            return Ok(());
        }
    };
    info!("Connected to WebSocket: {}", url);

    let (_, mut receiver) = ws_stream.split();
    while let Some(message) = receiver.next().await {
        match message {
            Ok(_msg) => {
                // Placeholder: Parse WebSocket message into MarketEvent
                let event = MarketEvent {
                    token_pair: "SOL/USDC".to_string(),
                    price: 150.0, // Mock price
                    source: "WebSocket".to_string(),
                };
                if tx.send(event).await.is_err() {
                    error!("Receiver dropped, closing WebSocket listener.");
                    break;
                }
            }
            Err(e) => error!("WebSocket error: {}", e),
        }
    }
    Ok(())
}

async fn grpc_listener(_url: String, _tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    // Placeholder: Implement gRPC client for data streaming
    info!("gRPC listener started (placeholder).");
    Ok(())
}
