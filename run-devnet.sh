#!/bin/bash
# Run the MEV bot on devnet for testing with free SOL

echo "🚀 Starting MEV Bot on DEVNET..."
echo "📍 Using devnet RPC endpoints"

# Load the environment first
set -a
source .env
set +a

# Override mainnet URLs with devnet for testing
export RPC_URL=$DEVNET_RPC_URL
export RPC_URL_TX=$DEVNET_RPC_URL
export WSS_RPC_URL=$DEVNET_WSS_URL

echo "💳 Wallet: $(solana-keygen pubkey $PAYER_KEYPAIR_PATH)"
echo "🌐 Network: DEVNET"
echo ""

# Run the bot
cargo run
