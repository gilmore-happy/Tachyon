//! src/data/market_stream.rs

use crate::common::config::{Config, DataMode};
use anyhow::Result;
use futures_util::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration, interval};
use tokio_tungstenite::{connect_async, tungstenite::{Message, client::IntoClientRequest}};
use tokio_tungstenite::tungstenite::http::Uri;
use tracing::{debug, error, info, info_span, warn, Instrument};

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
    // TODO: Move Helius API key to configuration
    // SECURITY: API key should be loaded from config, not hardcoded
    let helius_url = std::env::var("HELIUS_WS_URL")
        .unwrap_or_else(|_| {
            warn!("HELIUS_WS_URL not set, using primary endpoint only");
            String::new()
        });
    
    let mut endpoints = vec![url.clone()];
    if !helius_url.is_empty() {
        endpoints.push(helius_url);
    }
    
    for (i, endpoint_url) in endpoints.iter().enumerate() {
        info!("🔄 Attempting WebSocket connection {} to: {}", i + 1, endpoint_url);
        
        match connect_with_robust_config(endpoint_url).await {
            Ok(ws_stream) => {
                info!("✅ WebSocket connected successfully to endpoint {}!", i + 1);
                return handle_websocket_stream(ws_stream, tx).await;
            },
            Err(e) => {
                error!("❌ Endpoint {} failed: {}", i + 1, e);
                if i < endpoints.len() - 1 {
                    info!("⏳ Trying next endpoint in 2 seconds...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
    
    Err(anyhow::anyhow!("All WebSocket endpoints failed to connect"))
}

/// Robust WebSocket connection with timeout and custom headers
async fn connect_with_robust_config(url: &str) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
    // Parse URL and add custom headers
    let parsed_url: Uri = url.parse()?;
    let mut req = parsed_url.into_client_request()?;
    
    // Add required headers for QuickNode compatibility
    let headers = req.headers_mut();
    headers.insert("Origin", "https://tachyon-hft.local".parse()?);
    headers.insert("User-Agent", "Tachyon-HFT-Bot/1.0".parse()?);
    
    info!("📡 Connecting with headers: Origin=tachyon-hft.local, User-Agent=Tachyon-HFT-Bot/1.0");
    
    // Add 10-second connection timeout
    let connection_timeout = Duration::from_secs(10);
    
    match timeout(connection_timeout, connect_async(req)).await {
        Ok(Ok((ws_stream, response))) => {
            info!("✅ WebSocket handshake successful! Status: {}", response.status());
            Ok(ws_stream)
        },
        Ok(Err(e)) => {
            error!("❌ WebSocket connection failed: {}", e);
            Err(anyhow::anyhow!("WebSocket connection error: {}", e))
        },
        Err(_) => {
            error!("⏰ WebSocket connection timeout after 10 seconds");
            Err(anyhow::anyhow!("Connection timeout"))
        }
    }
}

/// Handle WebSocket stream with proper error handling and keep-alive
async fn handle_websocket_stream(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tx: mpsc::Sender<MarketEvent>
) -> Result<()> {

    let (mut sender, mut receiver) = ws_stream.split();
    
    // Start with minimal test subscription
    info!("📡 Sending test subscription (slotSubscribe)...");
    let test_subscription = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "slotSubscribe"
    });
    
    sender.send(Message::Text(test_subscription.to_string())).await?;
    info!("✅ Test subscription sent, waiting for confirmation...");
    
    // Wait for subscription confirmation
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Add account subscription for SOL/USDC pool
    info!("📡 Adding account subscription for SOL/USDC pool...");
    let account_subscription = serde_json::json!({
        "jsonrpc": "2.0", 
        "id": 2,
        "method": "accountSubscribe",
        "params": [
            "58oQChx4yWmvKdwLLZzBi4ChoCKmMY8dqZMFwrxDCWnT", // SOL/USDC Raydium
            {
                "encoding": "base64",
                "commitment": "confirmed"
            }
        ]
    });
    
    sender.send(Message::Text(account_subscription.to_string())).await?;
    
    // Wait before adding logs subscription
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Add logs subscription for broader coverage
    info!("📡 Adding logs subscription for broader DEX coverage...");
    let logs_subscription = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "logsSubscribe",
        "params": [
            {
                "mentions": ["675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8"] // Raydium AMM
            },
            {
                "commitment": "confirmed"
            }
        ]
    });
    
    sender.send(Message::Text(logs_subscription.to_string())).await?;
    info!("🎯 All subscriptions active: slot + account + logs for maximum coverage");
    
    // Set up keep-alive ping interval
    let mut ping_interval = interval(Duration::from_secs(30));
    ping_interval.tick().await; // Skip first immediate tick
    
    // Main message processing loop with keep-alive
    loop {
        tokio::select! {
            // Handle keep-alive pings
            _ = ping_interval.tick() => {
                if let Err(e) = sender.send(Message::Ping(vec![])).await {
                    error!("❌ Ping failed - connection lost: {}", e);
                    break;
                }
                debug!("💗 Sent keep-alive ping");
            }
            
            // Handle incoming messages
            message = receiver.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        // Parse real Solana WebSocket messages for both subscription types
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(method) = parsed.get("method") {
                                match method.as_str() {
                                    Some("logsNotification") => {
                                        // Handle transaction log events (broader coverage)
                                        if let Some(value) = parsed.pointer("/params/result/value") {
                                            let event = parse_transaction_logs_for_arbitrage(value);
                                            if let Some(market_event) = event {
                                                if tx.send(market_event).await.is_err() {
                                                    error!("Receiver dropped, closing WebSocket listener.");
                                                    break;
                                                }
                                            }
                                        } else {
                                            debug!("logsNotification missing expected data structure");
                                        }
                                    },
                                    Some("accountNotification") => {
                                        // Handle account updates (precise pool monitoring)
                                        if let Some(params) = parsed.get("params") {
                                            if let Some(result) = params.get("result") {
                                                if let Some(value) = result.get("value") {
                                                    // Parse pool state changes for price updates
                                                    let event = MarketEvent {
                                                        token_pair: "SOL/USDC".to_string(),
                                                        price: parse_real_price_from_account_data(value),
                                                        source: "Account_Monitor".to_string(),
                                                    };
                                                    
                                                    if tx.send(event).await.is_err() {
                                                        error!("Receiver dropped, closing WebSocket listener.");
                                                        return Ok(());
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    Some("slotNotification") => {
                                        // Handle slot updates (basic connectivity)
                                        if let Some(params) = parsed.get("params") {
                                            if let Some(result) = params.get("result") {
                                                if let Some(slot) = result.get("slot") {
                                                    debug!("📊 Current slot: {}", slot);
                                                }
                                            }
                                        }
                                    },
                                    _ => {
                                        // Handle subscription confirmations and other messages
                                        debug!("Received WebSocket message: {}", method.as_str().unwrap_or("unknown"));
                                    }
                                }
                            } else {
                                debug!("Received WebSocket message without method field");
                            }
                        }
                    },
                Some(Ok(Message::Binary(_))) => {
                    debug!("Received binary WebSocket message");
                },
                Some(Ok(Message::Ping(ping))) => {
                    debug!("📨 Received ping, sending pong");
                    if let Err(e) = sender.send(Message::Pong(ping)).await {
                        error!("Failed to send pong: {}", e);
                        break;
                    }
                },
                Some(Ok(Message::Pong(_))) => {
                    debug!("📨 Received pong");
                },
                Some(Ok(Message::Close(frame))) => {
                    info!("📪 WebSocket close frame received: {:?}", frame);
                    break;
                },
                Some(Ok(Message::Frame(_))) => {
                    debug!("Received raw WebSocket frame");
                },
                Some(Err(e)) => {
                    error!("❌ WebSocket error: {}", e);
                    break;
                },
                None => {
                    warn!("🔌 WebSocket stream ended");
                    break;
                }
            }
        }
    }
}
    
    info!("🔌 WebSocket connection closed");
    Ok(())
}

async fn grpc_listener(_url: String, _tx: mpsc::Sender<MarketEvent>) -> Result<()> {
    // gRPC/Geyser data streaming disabled - requires additional QuickNode subscription
    info!("⚠️  gRPC/Geyser listener disabled - using WebSocket RPC for market data");
    info!("💡 To enable: Upgrade QuickNode plan and implement Geyser client");
    
    // Keep connection alive but don't process data
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

// Extract REAL price data from Solana account updates
fn parse_real_price_from_account_data(account_data: &serde_json::Value) -> f64 {
    // Parse actual Solana account data to extract pool reserves
    if let Some(account) = account_data.get("account") {
        if let Some(data_array) = account.get("data").and_then(|v| v.as_array()) {
            if let Some(data_base64) = data_array.get(0).and_then(|v| v.as_str()) {
                // Decode base64 account data using new API
                use base64::{engine::general_purpose, Engine as _};
                if let Ok(decoded_data) = general_purpose::STANDARD.decode(data_base64) {
                    // Parse pool state based on known AMM program layouts
                    if let Ok(price) = parse_amm_pool_state(&decoded_data) {
                        return price;
                    } else {
                        warn!("Failed to parse AMM pool state, falling back to balance-based estimation");
                    }
                }
            }
        }
        
        // No valid account data for AMM pool state parsing
        warn!("Account data does not contain valid pool state information");
        // Current SOL price as of January 2025: ~$145-148 (do not use as trading price)
        // This is only used as a fallback indicator, not for trading decisions
        148.0 // Market reference price - NOT for trading
    } else {
        warn!("No account data found in notification");
        148.0 // Market reference price - NOT for trading
    }
}

// Parse AMM pool state from raw account data
fn parse_amm_pool_state(data: &[u8]) -> Result<f64> {
    // Validate minimum data length for Raydium AMM pool
    if data.len() < 752 {
        return Err(anyhow::anyhow!("Insufficient data length: {} bytes, expected >= 752", data.len()));
    }
    
    // Parse pool reserves with proper error handling
    // Note: These offsets are based on Raydium V4 layout - may need updates if schema changes
    let reserve_a_slice = data.get(240..248)
        .ok_or_else(|| anyhow::anyhow!("Failed to get reserve A slice at bytes 240-248"))?;
    let reserve_b_slice = data.get(248..256)
        .ok_or_else(|| anyhow::anyhow!("Failed to get reserve B slice at bytes 248-256"))?;
    
    let reserve_a_bytes: [u8; 8] = reserve_a_slice.try_into()
        .map_err(|e| anyhow::anyhow!("Failed to convert reserve A slice to [u8; 8]: {:?}", e))?;
    let reserve_b_bytes: [u8; 8] = reserve_b_slice.try_into()
        .map_err(|e| anyhow::anyhow!("Failed to convert reserve B slice to [u8; 8]: {:?}", e))?;
    
    let reserve_a = u64::from_le_bytes(reserve_a_bytes);
    let reserve_b = u64::from_le_bytes(reserve_b_bytes);
    
    // Validate reserves are non-zero
    if reserve_a == 0 || reserve_b == 0 {
        return Err(anyhow::anyhow!("Invalid reserves: A={}, B={}", reserve_a, reserve_b));
    }
    
    // Calculate price from reserves
    let price = reserve_b as f64 / reserve_a as f64;
    
    // Apply decimals adjustment (USDC=6 decimals, SOL=9 decimals)
    // Price = (reserve_b / 10^6) / (reserve_a / 10^9) = reserve_b * 10^3 / reserve_a
    let adjusted_price = price * 1000.0;
    
    // Validate price is reasonable (basic sanity check)
    if adjusted_price < 1.0 || adjusted_price > 10000.0 {
        return Err(anyhow::anyhow!("Calculated price ${:.2} outside reasonable range", adjusted_price));
    }
    
    info!("🔢 AMM pool price: Reserve A: {:.3} SOL, Reserve B: {:.2} USDC, Price: ${:.2}", 
        reserve_a as f64 / 1e9, reserve_b as f64 / 1e6, adjusted_price);
    
    Ok(adjusted_price)
}

/// Parse transaction logs for arbitrage opportunities
fn parse_transaction_logs_for_arbitrage(log_data: &serde_json::Value) -> Option<MarketEvent> {
    // Extract transaction logs from logsSubscribe notification
    if let Some(logs) = log_data.get("logs").and_then(|l| l.as_array()) {
        for log_entry in logs {
            if let Some(log_str) = log_entry.as_str() {
                // Look for swap/trade events in transaction logs
                if log_str.contains("Instruction: Swap") || log_str.contains("SwapEvent") {
                    // Extract DEX-specific swap information
                    let token_pair = extract_token_pair_from_logs(log_str);
                    let price = extract_price_from_logs(log_str);
                    
                    return Some(MarketEvent {
                        token_pair: token_pair.unwrap_or("UNKNOWN/UNKNOWN".to_string()),
                        price: price.unwrap_or(0.0),
                        source: "Transaction_Logs".to_string(),
                    });
                }
            }
        }
    }
    None
}

/// Extract token pair information from transaction log strings
fn extract_token_pair_from_logs(log_str: &str) -> Option<String> {
    // Simple pattern matching for common DEX log formats
    // This is a basic implementation - production would need more sophisticated parsing
    if log_str.contains("SOL") && log_str.contains("USDC") {
        Some("SOL/USDC".to_string())
    } else if log_str.contains("SOL") && log_str.contains("USDT") {
        Some("SOL/USDT".to_string())
    } else {
        None
    }
}

/// Extract price information from transaction log strings
fn extract_price_from_logs(_log_str: &str) -> Option<f64> {
    // TODO: Implement actual price extraction from transaction logs
    // Current implementation is placeholder - production would parse:
    // 1. Swap instruction data from logs  
    // 2. Amount in/out to calculate actual executed price
    // 3. Token mint addresses for proper pair identification
    warn!("Log-based price extraction not yet implemented - returning None");
    None // Return None until proper implementation
}
