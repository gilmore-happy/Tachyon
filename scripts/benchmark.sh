#!/bin/bash

# MEV Bot Performance Benchmarking Suite
# This script runs comprehensive benchmarks and generates performance reports

set -e

echo "🚀 MEV Bot Performance Benchmarking Suite"
echo "=========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create benchmark results directory
RESULTS_DIR="benchmark_results/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo -e "${BLUE}📊 Results will be saved to: $RESULTS_DIR${NC}"

# Function to run benchmark with error handling
run_benchmark() {
    local bench_name=$1
    local description=$2
    
    echo -e "\n${YELLOW}🔧 Running $description...${NC}"
    
    if CARGO_INCREMENTAL=0 cargo bench --bench "$bench_name"; then
        echo -e "${GREEN}✅ $description completed successfully${NC}"
        
        # Move HTML reports to results directory
        if [ -d "target/criterion" ]; then
            cp -r target/criterion "$RESULTS_DIR/${bench_name}_report"
        fi
    else
        echo -e "${RED}❌ $description failed${NC}"
        return 1
    fi
}

# Function to run memory profiling
run_memory_profile() {
    echo -e "\n${YELLOW}🧠 Running memory profiling...${NC}"
    
    if command -v valgrind &> /dev/null; then
        echo "Using Valgrind for memory profiling..."
        CARGO_INCREMENTAL=0 cargo build --release
        valgrind --tool=massif --massif-out-file="$RESULTS_DIR/memory_profile.out" \
            target/release/mev_bot_solana --help > /dev/null 2>&1 || true
        
        if command -v ms_print &> /dev/null; then
            ms_print "$RESULTS_DIR/memory_profile.out" > "$RESULTS_DIR/memory_report.txt"
        fi
    else
        echo "Valgrind not available, skipping memory profiling"
    fi
}

# Function to run CPU profiling
run_cpu_profile() {
    echo -e "\n${YELLOW}⚡ Running CPU profiling...${NC}"
    
    if command -v perf &> /dev/null; then
        echo "Using perf for CPU profiling..."
        CARGO_INCREMENTAL=0 cargo build --release
        
        # Run a short profiling session
        timeout 30s perf record -g target/release/mev_bot_solana --help > /dev/null 2>&1 || true
        
        if [ -f "perf.data" ]; then
            perf report --stdio > "$RESULTS_DIR/cpu_profile.txt" 2>/dev/null || true
            rm -f perf.data
        fi
    else
        echo "perf not available, skipping CPU profiling"
    fi
}

# Function to generate performance summary
generate_summary() {
    echo -e "\n${BLUE}📋 Generating performance summary...${NC}"
    
    cat > "$RESULTS_DIR/README.md" << EOF
# MEV Bot Performance Benchmark Results

Generated on: $(date)
Rust Version: $(rustc --version)
System: $(uname -a)

## Benchmark Results

### Arbitrage Performance
- Path calculation benchmarks
- Opportunity evaluation speed
- Memory allocation efficiency

### Market Data Processing
- Cache operation performance
- Concurrent access patterns
- Data parsing efficiency

### Execution Performance
- Transaction building speed
- Priority queue operations
- Concurrent processing

### Memory Analysis
- Allocation patterns
- Memory usage optimization
- Leak detection

## Files in this directory:
- \`arbitrage_benchmarks_report/\` - Arbitrage calculation benchmarks
- \`market_data_benchmarks_report/\` - Market data processing benchmarks
- \`execution_benchmarks_report/\` - Transaction execution benchmarks
- \`memory_profile.out\` - Memory profiling data (if available)
- \`memory_report.txt\` - Memory usage report (if available)
- \`cpu_profile.txt\` - CPU profiling report (if available)

## How to view HTML reports:
Open the \`index.html\` file in each benchmark report directory.

## Performance Targets:
- Arbitrage calculation: <1ms per 1000 paths
- Cache operations: <100ns per operation
- Transaction building: <5ms per transaction
- Memory usage: <100MB steady state
EOF
}

# Main execution
echo -e "${BLUE}🔍 Checking system requirements...${NC}"

# Check if criterion is available
if ! cargo bench --help | grep -q "criterion" 2>/dev/null; then
    echo -e "${YELLOW}⚠️  Installing criterion for benchmarking...${NC}"
fi

# Run all benchmarks
echo -e "\n${GREEN}🏁 Starting benchmark suite...${NC}"

run_benchmark "arbitrage_benchmarks" "Arbitrage Performance Tests"
run_benchmark "market_data_benchmarks" "Market Data Processing Tests"
run_benchmark "execution_benchmarks" "Execution Performance Tests"
run_benchmark "rpc_performance_benchmarks" "RPC Performance Tests (Using Your Paid Endpoints)"

# Run profiling (optional, requires additional tools)
run_memory_profile
run_cpu_profile

# Generate summary
generate_summary

# Performance regression check
echo -e "\n${YELLOW}📈 Checking for performance regressions...${NC}"

if [ -f "benchmark_baseline.json" ]; then
    echo "Comparing against baseline..."
    # Here you could add logic to compare against previous results
    echo "Baseline comparison would go here"
else
    echo "No baseline found. Current results will serve as baseline."
    cp -r "$RESULTS_DIR" "benchmark_baseline"
fi

echo -e "\n${GREEN}🎉 Benchmark suite completed!${NC}"
echo -e "${BLUE}📊 Results saved to: $RESULTS_DIR${NC}"
echo -e "${BLUE}🌐 Open $RESULTS_DIR/*/index.html to view detailed reports${NC}"

# Quick performance summary
echo -e "\n${YELLOW}⚡ Quick Performance Summary:${NC}"
echo "- Check the HTML reports for detailed metrics"
echo "- Look for any red/slow benchmarks that need optimization"
echo "- Memory usage and allocation patterns in memory_report.txt"
echo "- CPU hotspots in cpu_profile.txt"

echo -e "\n${GREEN}✨ Ready to optimize for maximum profit! ✨${NC}" 