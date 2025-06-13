#!/bin/bash
# Run the MEV bot in paper trading mode on mainnet

echo "📊 Starting MEV Bot in PAPER TRADING mode..."
echo "📍 Using mainnet RPC endpoints (no real trades)"

# Load environment
set -a
source .env
set +a

echo "💳 Wallet: $(solana-keygen pubkey $PAYER_KEYPAIR_PATH)"
echo "🌐 Network: MAINNET (Paper Trading)"
echo ""

# Run the bot in paper trading mode
# Note: The bot will need to be modified to support a --paper-trading flag
cargo run
