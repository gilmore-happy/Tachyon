#!/bin/bash

# Simple startup script that won't timeout

echo "🚀 Starting Solana Trading Bot..."

# Export PATH for Solana and Cargo
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
export PATH="$HOME/.cargo/bin:$PATH"

# Check if we have the binary built
if [ ! -f "target/release/solana-mev-bot" ]; then
    echo "⚠️  Binary not found. Building project..."
    cargo build --release || {
        echo "❌ Build failed. Please check errors."
        exit 1
    }
fi

# Create necessary directories
mkdir -p logs
mkdir -p src/transactions/cache

# Check config
if [ ! -f "config.json" ]; then
    echo "⚠️  config.json not found. Creating default..."
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
fi

echo "✅ Ready to run!"
echo ""
echo "Use one of these commands:"
echo "  ./run-paper-trading.sh  - Run paper trading mode"
echo "  ./run-devnet.sh        - Run on devnet"
echo "  cargo run --release    - Run directly"
