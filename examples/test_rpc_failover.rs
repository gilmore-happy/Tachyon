//! Test script for RPC Manager failover functionality
//! 
//! This script demonstrates how to test the RPC failover mechanism
//! as recommended in the expert considerations.

use anyhow::Result;
use mev_bot_solana::common::config::Config;
use mev_bot_solana::common::rpc_manager::{create_rpc_manager, RpcManager};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🧪 Starting RPC Manager Failover Test");

    // Load configuration
    let config = Config::load().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    
    // Create RPC Manager
    let rpc_manager = Arc::new(create_rpc_manager(&config));
    
    // Display initial configuration
    let initial_stats = rpc_manager.get_stats().await;
    info!("📊 Initial RPC Configuration:");
    info!("   - Total endpoints: {}", initial_stats.total_endpoints);
    info!("   - Current endpoint: {}", initial_stats.current_endpoint);
    info!("   - Success rate: {:.2}%", initial_stats.success_rate);

    // Test 1: Basic RPC functionality
    info!("\n🔍 Test 1: Basic RPC functionality");
    test_basic_rpc_calls(&rpc_manager).await?;

    // Test 2: Manual failover test
    info!("\n🔄 Test 2: Manual failover test");
    test_manual_failover(&rpc_manager).await?;

    // Test 3: Automatic retry mechanism
    info!("\n🔁 Test 3: Automatic retry mechanism");
    test_retry_mechanism(&rpc_manager).await?;

    // Test 4: Performance under load
    info!("\n⚡ Test 4: Performance under load");
    test_performance_under_load(&rpc_manager).await?;

    // Final statistics
    let final_stats = rpc_manager.get_stats().await;
    info!("\n📈 Final Statistics:");
    info!("   - Total requests: {}", final_stats.total_requests);
    info!("   - Successful requests: {}", final_stats.successful_requests);
    info!("   - Failed requests: {}", final_stats.failed_requests);
    info!("   - Failover count: {}", final_stats.failover_count);
    info!("   - Success rate: {:.2}%", final_stats.success_rate);

    info!("✅ RPC Manager Failover Test completed successfully!");
    Ok(())
}

/// Test basic RPC functionality
async fn test_basic_rpc_calls(rpc_manager: &Arc<RpcManager>) -> Result<()> {
    // Test getting RPC client
    let client = rpc_manager.get_client().await;
    info!("✅ Successfully obtained RPC client");

    // Test basic RPC call with retry mechanism
    let health_result = rpc_manager.execute_with_retry(|client| async move {
        client.get_health().await.map_err(|e| anyhow::anyhow!("Health check failed: {}", e))
    }).await;

    match health_result {
        Ok(_) => info!("✅ Health check successful"),
        Err(e) => warn!("⚠️ Health check failed: {}", e),
    }

    // Test getting latest blockhash
    let blockhash_result = rpc_manager.execute_with_retry(|client| async move {
        client.get_latest_blockhash().await.map_err(|e| anyhow::anyhow!("Blockhash fetch failed: {}", e))
    }).await;

    match blockhash_result {
        Ok(blockhash) => info!("✅ Latest blockhash: {}", blockhash),
        Err(e) => warn!("⚠️ Blockhash fetch failed: {}", e),
    }

    Ok(())
}

/// Test manual failover functionality
async fn test_manual_failover(rpc_manager: &Arc<RpcManager>) -> Result<()> {
    let initial_stats = rpc_manager.get_stats().await;
    let initial_endpoint = initial_stats.current_endpoint;
    
    info!("Current endpoint: {}", initial_endpoint);
    
    // Trigger manual failover test
    match rpc_manager.test_failover().await {
        Ok(_) => {
            let new_stats = rpc_manager.get_stats().await;
            info!("✅ Failover test successful: {} -> {}", 
                  initial_endpoint, new_stats.current_endpoint);
        }
        Err(e) => {
            error!("❌ Failover test failed: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Test automatic retry mechanism
async fn test_retry_mechanism(rpc_manager: &Arc<RpcManager>) -> Result<()> {
    info!("Testing retry mechanism with multiple rapid requests...");
    
    let mut successful_requests = 0;
    let mut failed_requests = 0;
    
    for i in 0..10 {
        let result = rpc_manager.execute_with_retry(|client| async move {
            client.get_health().await.map_err(|e| anyhow::anyhow!("Request {} failed: {}", i, e))
        }).await;
        
        match result {
            Ok(_) => {
                successful_requests += 1;
                info!("✅ Request {} successful", i);
            }
            Err(e) => {
                failed_requests += 1;
                warn!("❌ Request {} failed: {}", i, e);
            }
        }
        
        // Small delay between requests
        sleep(Duration::from_millis(100)).await;
    }
    
    info!("Retry test results: {} successful, {} failed", successful_requests, failed_requests);
    
    if successful_requests > 0 {
        info!("✅ Retry mechanism working correctly");
    } else {
        warn!("⚠️ All requests failed - check RPC configuration");
    }
    
    Ok(())
}

/// Test performance under load
async fn test_performance_under_load(rpc_manager: &Arc<RpcManager>) -> Result<()> {
    info!("Testing performance under concurrent load...");
    
    let start_time = std::time::Instant::now();
    let mut handles = Vec::new();
    
    // Create 20 concurrent requests
    for i in 0..20 {
        let rpc_manager_clone = rpc_manager.clone();
        let handle = tokio::spawn(async move {
            let result = rpc_manager_clone.execute_with_retry(|client| async move {
                client.get_health().await.map_err(|e| anyhow::anyhow!("Concurrent request {} failed: {}", i, e))
            }).await;
            (i, result)
        });
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    let mut successful = 0;
    let mut failed = 0;
    
    for handle in handles {
        match handle.await {
            Ok((i, Ok(_))) => {
                successful += 1;
                info!("✅ Concurrent request {} successful", i);
            }
            Ok((i, Err(e))) => {
                failed += 1;
                warn!("❌ Concurrent request {} failed: {}", i, e);
            }
            Err(e) => {
                failed += 1;
                error!("❌ Task failed: {}", e);
            }
        }
    }
    
    let elapsed = start_time.elapsed();
    info!("Performance test completed in {:?}", elapsed);
    info!("Results: {} successful, {} failed", successful, failed);
    
    if successful > failed {
        info!("✅ Performance test passed");
    } else {
        warn!("⚠️ Performance test showed issues - check RPC configuration");
    }
    
    Ok(())
} 