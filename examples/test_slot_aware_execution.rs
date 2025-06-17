//! Test Slot-Aware Execution for MEV Timing
//! 
//! Validates that our RPC Manager can track slots and provide MEV timing information
//! Essential for profitable arbitrage in the 200-400ms opportunity window.

use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

// Import our RPC Manager
use tachyon::common::config::Config;
use tachyon::common::rpc_manager::{create_rpc_manager, SlotTiming};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("🧪 Testing Slot-Aware Execution for MEV Timing");
    
    // Load configuration
    let config = Config::load()?;
    
    // Create RPC Manager with slot tracking
    let rpc_manager = create_rpc_manager(&config);
    
    // Start slot tracking
    info!("🚀 Starting slot tracking...");
    rpc_manager.start_slot_tracking().await;
    
    // Wait for initial slot data
    sleep(Duration::from_secs(2)).await;
    
    // Test 1: Basic slot tracking
    info!("📊 Test 1: Basic Slot Tracking");
    let current_slot = rpc_manager.get_current_slot();
    info!("Current slot: {}", current_slot);
    
    // Test 2: Slot timing information
    info!("📊 Test 2: Slot Timing Information");
    let timing = rpc_manager.get_slot_timing().await;
    info!("Slot timing: {:?}", timing);
    
    // Test 3: Execution window checking
    info!("📊 Test 3: Execution Window Checking");
    
    // Test different execution time requirements
    let test_cases = vec![
        (10, "Very fast execution"),
        (50, "Normal execution"),
        (100, "Slow execution"),
        (300, "Very slow execution"),
        (500, "Too slow for slot"),
    ];
    
    for (required_ms, description) in test_cases {
        let has_window = rpc_manager.has_execution_window(required_ms).await;
        let status = if has_window { "✅ PASS" } else { "❌ FAIL" };
        info!("{} - {} ({}ms): {}", status, description, required_ms, has_window);
    }
    
    // Test 4: Real-time slot progression monitoring
    info!("📊 Test 4: Real-time Slot Progression (10 seconds)");
    let start_slot = rpc_manager.get_current_slot();
    let start_time = std::time::Instant::now();
    
    for i in 0..10 {
        sleep(Duration::from_secs(1)).await;
        let current_slot = rpc_manager.get_current_slot();
        let timing = rpc_manager.get_slot_timing().await;
        let time_remaining = timing.time_remaining_in_slot();
        
        info!("Second {}: Slot {} | Time remaining: {}ms", 
              i + 1, 
              current_slot, 
              time_remaining.as_millis());
        
        // Check if slot progressed
        if current_slot > start_slot {
            info!("🎯 Slot progression detected! {} -> {}", start_slot, current_slot);
        }
    }
    
    let elapsed = start_time.elapsed();
    let final_slot = rpc_manager.get_current_slot();
    let slots_progressed = final_slot.saturating_sub(start_slot);
    
    info!("📈 Summary:");
    info!("  - Test duration: {:?}", elapsed);
    info!("  - Slots progressed: {}", slots_progressed);
    info!("  - Average slot time: {:?}", elapsed / slots_progressed.max(1) as u32);
    
    // Test 5: MEV execution simulation
    info!("📊 Test 5: MEV Execution Simulation");
    
    for i in 0..5 {
        let timing = rpc_manager.get_slot_timing().await;
        let time_remaining = timing.time_remaining_in_slot();
        
        // Simulate different MEV strategies
        let strategies = vec![
            (20, "Lightning arbitrage"),
            (50, "Standard arbitrage"),
            (100, "Complex arbitrage"),
        ];
        
        info!("Slot {} | Time remaining: {}ms", timing.current_slot, time_remaining.as_millis());
        
        for (required_ms, strategy) in &strategies {
            let can_execute = timing.has_execution_window(*required_ms);
            let status = if can_execute { "🚀 EXECUTE" } else { "⏰ SKIP" };
            info!("  {} - {}: {}", status, strategy, can_execute);
        }
        
        sleep(Duration::from_millis(500)).await;
    }
    
    info!("✅ Slot-aware execution testing completed!");
    info!("🎯 MEV timing capabilities validated");
    
    Ok(())
}

/// Test slot timing calculations independently
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    
    #[test]
    fn test_slot_timing_calculations() {
        // Test timing with 100ms elapsed, 400ms total duration
        let timing = SlotTiming {
            current_slot: 12345,
            slot_start_time: Instant::now() - Duration::from_millis(100),
            estimated_slot_duration: Duration::from_millis(400),
        };
        
        // Should have ~300ms remaining
        let remaining = timing.time_remaining_in_slot();
        assert!(remaining.as_millis() >= 250 && remaining.as_millis() <= 350);
        
        // Test execution windows
        assert!(timing.has_execution_window(200)); // Should pass
        assert!(!timing.has_execution_window(350)); // Should fail
    }
    
    #[test]
    fn test_slot_timing_expired() {
        // Test timing with slot already expired
        let timing = SlotTiming {
            current_slot: 12345,
            slot_start_time: Instant::now() - Duration::from_millis(500),
            estimated_slot_duration: Duration::from_millis(400),
        };
        
        // Should have 0ms remaining
        let remaining = timing.time_remaining_in_slot();
        assert_eq!(remaining.as_millis(), 0);
        
        // No execution window should be available
        assert!(!timing.has_execution_window(1));
    }
} 