#!/bin/bash
set -e

echo "🚀 Fast setup for Solana Trading Bot..."
echo "======================================"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    print_error "Rust is not installed. Please install Rust first:"
    echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
else
    print_status "Rust already installed ($(rustc --version))"
fi

# Check if Solana CLI is installed
if ! command -v solana &> /dev/null; then
    print_warning "Solana CLI not found. Installing..."
    sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
    export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
    echo 'export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"' >> ~/.bashrc
    print_status "Solana CLI installed"
else
    print_status "Solana CLI already installed ($(solana --version))"
fi

# Create necessary directories
echo -e "\n📁 Creating project directories..."
mkdir -p logs
mkdir -p src/transactions/cache
print_status "Directories created"

# Generate default config.json if not exists
if [ ! -f "config.json" ]; then
    echo -e "\n⚙️ Creating default config.json..."
    cat > config.json << EOF
{
  "tokens_to_trade": [
    {
      "address": "So11111111111111111111111111111111111111112",
      "symbol": "SOL"
    },
    {
      "address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
      "symbol": "USDC"
    }
  ],
  "min_profit_threshold": 20.0,
  "max_slippage": 0.02,
  "enable_paper_trading": true
}
EOF
    print_status "Default config.json created"
else
    print_status "config.json already exists"
fi

# Fetch dependencies only (don't build yet)
echo -e "\n📚 Fetching dependencies..."
cargo fetch
print_status "Dependencies fetched"

# Quick check build (not full release build)
echo -e "\n🔨 Running quick build check..."
if cargo check; then
    print_status "Build check passed"
else
    print_error "Build check failed - fix errors before running"
    exit 1
fi

echo -e "\n✅ Fast setup complete!"
echo ""
echo "Next steps:"
echo "1. Build the project: cargo build --release"
echo "2. Run paper trading: ./run-paper-trading.sh"
echo "3. Or run on devnet: ./run-devnet.sh"
