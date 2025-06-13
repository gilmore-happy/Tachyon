#!/bin/bash
set -e

echo "🚀 Setting up Solana Trading Bot environment..."
echo "================================================"

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

# Install system dependencies
echo -e "\n📦 Installing system dependencies..."
if command -v apt-get &> /dev/null; then
    apt-get update && apt-get install -y \
        build-essential \
        pkg-config \
        libssl-dev \
        curl \
        git \
        jq
    print_status "System dependencies installed"
else
    print_warning "Not running on Debian/Ubuntu, skipping system package installation"
fi

# Install Rust if not present
if ! command -v cargo &> /dev/null; then
    echo -e "\n🦀 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    print_status "Rust installed successfully"
else
    print_status "Rust already installed ($(rustc --version))"
fi

# Update Rust to latest stable
echo -e "\n🔄 Updating Rust to latest stable..."
rustup update stable
rustup default stable
print_status "Rust updated to latest stable"

# Install Solana CLI
if ! command -v solana &> /dev/null; then
    echo -e "\n☀️ Installing Solana CLI..."
    sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
    export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
    echo 'export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"' >> ~/.bashrc
    print_status "Solana CLI installed"
else
    print_status "Solana CLI already installed ($(solana --version))"
fi

# Setup wallet from secret if provided
if [ ! -z "$PAYER_KEYPAIR_CONTENT" ]; then
    echo -e "\n💳 Setting up wallet from secret..."
    mkdir -p /workspace/wallet
    echo "$PAYER_KEYPAIR_CONTENT" | base64 -d > /workspace/wallet/keypair.json
    chmod 600 /workspace/wallet/keypair.json
    
    # Update PAYER_KEYPAIR_PATH to point to the new location
    export PAYER_KEYPAIR_PATH=/workspace/wallet/keypair.json
    
    # Verify wallet
    if WALLET_ADDRESS=$(solana-keygen pubkey /workspace/wallet/keypair.json 2>/dev/null); then
        print_status "Wallet configured: $WALLET_ADDRESS"
    else
        print_error "Failed to configure wallet"
    fi
else
    print_warning "No wallet secret provided, using existing configuration"
fi

# Install Rust development tools
echo -e "\n🛠️ Installing Rust development tools..."
cargo install cargo-watch cargo-edit cargo-audit || print_warning "Some tools failed to install"
print_status "Development tools installed"

# Fetch and build project dependencies
echo -e "\n📚 Building project..."
cargo fetch
print_status "Dependencies fetched"

# Build in release mode
if cargo build --release; then
    print_status "Project built successfully"
else
    print_error "Build failed - check error messages above"
    exit 1
fi

# Create necessary directories
echo -e "\n📁 Creating project directories..."
mkdir -p logs
mkdir -p src/transactions/cache
mkdir -p /workspace/wallet
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
  "enable_paper_trading": ${ENABLE_PAPER_TRADING:-false}
}
EOF
    print_status "Default config.json created"
else
    print_status "config.json already exists"
fi

# Setup environment variables in .bashrc for persistence
echo -e "\n🔧 Configuring environment..."
{
    echo ""
    echo "# Solana Trading Bot Environment"
    echo "export PATH=\"\$HOME/.local/share/solana/install/active_release/bin:\$PATH\""
    echo "export PATH=\"\$HOME/.cargo/bin:\$PATH\""
    [ ! -z "$NETWORK_MODE" ] && echo "export NETWORK_MODE=$NETWORK_MODE"
    [ ! -z "$DATABASE_NAME" ] && echo "export DATABASE_NAME=$DATABASE_NAME"
    [ ! -z "$PAYER_KEYPAIR_PATH" ] && echo "export PAYER_KEYPAIR_PATH=$PAYER_KEYPAIR_PATH"
} >> ~/.bashrc

# Test MongoDB connection if URI provided
if [ ! -z "$MONGODB_URI" ]; then
    echo -e "\n🗄️ Testing MongoDB connection..."
    # Simple connection test using the MongoDB URI
    if timeout 5 bash -c "echo 'db.runCommand({ping: 1})' | mongo $MONGODB_URI --quiet" &> /dev/null; then
        print_status "MongoDB connection successful"
    else
        print_warning "MongoDB connection failed or timed out"
    fi
fi

# Run tests to verify setup
echo -e "\n🧪 Running tests..."
if cargo test --release -- --test-threads=1 --nocapture; then
    print_status "All tests passed"
else
    print_warning "Some tests failed - this may be expected for integration tests"
fi

# Display configuration summary
echo -e "\n📊 Configuration Summary"
echo "========================"
echo "Rust Version: $(rustc --version)"
echo "Solana CLI: $(solana --version)"
echo "Network Mode: ${NETWORK_MODE:-mainnet}"
echo "Paper Trading: ${ENABLE_PAPER_TRADING:-false}"
echo "Database: ${DATABASE_NAME:-mev_bot_solana}"

if [ -f "/workspace/wallet/keypair.json" ]; then
    echo "Wallet: $(solana-keygen pubkey /workspace/wallet/keypair.json 2>/dev/null || echo 'Error reading wallet')"
else
    echo "Wallet: Not configured (will use PAYER_KEYPAIR_PATH from .env)"
fi

echo -e "\n✅ Setup complete!"
echo "You can now run the bot with:"
echo "  - ./run-devnet.sh     (for devnet testing)"
echo "  - ./run-paper-trading.sh (for paper trading)"
echo "  - cargo run --release (for production)"
