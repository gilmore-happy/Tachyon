#!/bin/bash

# Script to optimize the Solana Trading Bot codebase
# This script will:
# 1. Fix common warnings with cargo clippy
# 2. Format the code with cargo fmt
# 3. Analyze the code for performance issues
# 4. Update cache files with latest DEX data

set -e

echo "=== Solana Trading Bot Optimization Tool ==="
echo "This tool will optimize the codebase and update cache files"

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
  echo "Error: Please run this script from the project root directory"
  exit 1
fi

# Step 1: Run clippy to find common issues
echo "Running cargo clippy to find issues..."
cargo clippy -- -D warnings || {
  echo "Clippy found issues. Let's fix the most common ones automatically..."
  
  # Fix unused imports with cargo fix
  echo "Fixing unused imports..."
  cargo fix --allow-dirty --allow-staged
  
  # Fix formatting issues
  echo "Fixing code formatting..."
  cargo fmt
  
  echo "Basic fixes applied. Re-running clippy..."
  cargo clippy -- -D warnings || {
    echo "Some issues remain. Please fix them manually."
    echo "Common fixes:"
    echo "1. Unused variables: Prefix with underscore (_variable)"
    echo "2. Unused results: Add .unwrap(), .expect(), or use ? operator"
    echo "3. Naming conventions: Use snake_case for variables/functions"
    echo "4. Unnecessary parentheses: Remove them"
    echo "5. Unnecessary mut: Remove if variable is not modified"
  }
}

# Step 2: Update cache files (if user wants to)
read -p "Do you want to update DEX cache files? (y/n) " update_cache
if [ "$update_cache" == "y" ]; then
  echo "Updating cache files..."
  # Build and run the cache update script
  cargo run --bin optimize_cache || {
    echo "Cache update failed. Please check your RPC endpoints and network connection."
  }
fi

# Step 3: Run benchmarks if available
if [ -d "benches" ]; then
  echo "Running benchmarks to test optimizations..."
  cargo bench
fi

echo "Optimization process completed!"
echo "Please review the CLAUDE.md file for additional optimizations to consider."